<script setup lang="ts">
/**
 * 通用「安装 MOD」弹窗。
 *
 * 三个入口共用同一套「解析 → 预览 → 立即安装」流程：
 *   - MOD 管理页 / MOD 安装页的「安装 MOD」按钮（本地文件或在线直链）
 *   - 从仓库卡片点「安装 / 重装 / 更新」（下载完的资产走同一套解析）
 *   - 拖放文件到窗口
 *
 * 为什么抽出来：三个入口如果各写一套预览，很容易出现「手动安装看得到元数据、
 * 在线安装看不到」这种不一致，而且安装清单是同一个后端函数创建的，
 * 前置信息也应该完全一致。
 *
 * 后端契约：
 *   analyzeModSource(kind, value) → ModPreview（落到暂存目录，不安装）
 *   confirmModInstall(previewId)  → 真正安装并从暂存删除
 *   cancelModPreview(previewId)   → 用户放弃时删暂存
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { NButton, NInput, NModal, NSpin, NTabPane, NTabs, NTag, useMessage } from 'naive-ui'
import { LinkOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { isTauri } from '../runtime'
import { tagColorFor } from '../theme'
import { t } from '../i18n'
import type { ModPreview, ModSourceKind } from '../types'

const props = defineProps<{
  show: boolean
  /** 已下载好的资产路径：由仓库安装流程传入，直接进入解析。 */
  pendingLocalPath?: string | null
}>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  (event: 'installed'): void
}>()

const message = useMessage()
const installTab = ref<'local' | 'online'>('local')
const installUrl = ref('')
/** 解析 / 安装进行中（两者共用一个忙碌态，按钮互斥）。 */
const installing = ref(false)
/** 解析中（拖放区显示 loading）。 */
const awaitingSource = ref(false)
/** 当前预览：null 表示还没选文件。 */
const preview = ref<ModPreview | null>(null)
/** 拖放高亮：Tauri 的系统光标始终是「禁止」，高亮是唯一的可放置提示。 */
const dragActive = ref(false)

/** 弹窗是否打开（与父组件的 v-model 同步）。 */
const open = computed({
  get: () => props.show,
  set: (value: boolean) => {
    if (value) emit('update:show', true)
    else void close()
  },
})

/** 取消/关闭时清理后端暂存，避免留垃圾文件。 */
async function discardPreview() {
  const current = preview.value
  preview.value = null
  if (current) await api.cancelModPreview(current.preview_id).catch(() => undefined)
}

/** 关闭弹窗：先清暂存再收面板。 */
async function close() {
  await discardPreview()
  installUrl.value = ''
  emit('update:show', false)
}

/** 统一的解析入口：本地路径或在线 URL。 */
async function analyze(kind: ModSourceKind, value: string) {
  awaitingSource.value = true
  try {
    await discardPreview()
    preview.value = await api.analyzeModSource(kind, value)
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    awaitingSource.value = false
  }
}

async function pickLocalMod() {
  const path = await api.chooseModFile()
  if (!path) return
  await analyze('local', path)
}

async function downloadAndAnalyze() {
  const url = installUrl.value.trim()
  if (!/^https?:\/\//i.test(url)) { message.warning(t('请输入 zip 或 DLL 的直链网址')); return }
  await analyze('url', url)
}

/** 放弃当前预览，回到选择/输入状态。 */
async function resetPreview() {
  await discardPreview()
  installUrl.value = ''
}

/** 用户确认安装。 */
async function confirmInstall() {
  const current = preview.value
  if (!current) return
  installing.value = true
  try {
    const result = await api.confirmModInstall(current.preview_id)
    result.ok ? message.success(result.message) : message.error(result.message)
    if (result.ok) {
      // 后端已在 confirm 里删掉暂存，这里只需清掉本地引用
      preview.value = null
      installUrl.value = ''
      emit('update:show', false)
      emit('installed')
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    installing.value = false
  }
}

/** 切换来源时丢弃上一份预览。 */
watch(installTab, () => { if (preview.value) void discardPreview() })

/** 仓库安装流程传入已下载资产时，直接进入解析。 */
watch(() => props.pendingLocalPath, (path) => {
  if (!path) return
  installTab.value = 'local'
  void analyze('local', path)
}, { immediate: true })

/** 打开时复位：每次打开都从「本地文件 + 空 URL」开始。 */
watch(() => props.show, (visible) => {
  if (!visible) return
  installTab.value = 'local'
  installUrl.value = ''
  preview.value = null
})

/* ===== 拖放：Tauri 的 webview 事件给绝对路径，不是浏览器 File 对象 ===== */

let dragUnlisten: (() => void) | null = null

/**
 * 注册拖放监听。
 *
 * 只在真实 Tauri 窗口里有效；注册结果写日志，
 * 方便区分「没收到事件」和「高亮样式没生效」。
 */
function attachDragDrop() {
  if (!isTauri() || dragUnlisten) return
  void (async () => {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview')
      dragUnlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload
        if (payload.type === 'over' || payload.type === 'enter') {
          dragActive.value = true
          return
        }
        if (payload.type === 'leave') {
          dragActive.value = false
          return
        }
        if (payload.type !== 'drop') return
        dragActive.value = false
        const path = payload.paths?.[0]
        if (!path) return
        if (!/\.(dll|zip)$/i.test(path)) {
          message.warning(t('只支持 .dll 或 .zip 文件'))
          return
        }
        // 拖入即进入解析：自动打开弹窗并切到本地来源
        emit('update:show', true)
        installTab.value = 'local'
        void analyze('local', path)
      })
    } catch (reason) {
      console.error('[drop] listener registration failed', reason)
    }
  })()
}

function detachDragDrop() {
  dragUnlisten?.()
  dragUnlisten = null
}

onMounted(attachDragDrop)
onBeforeUnmount(() => {
  detachDragDrop()
  void discardPreview()
})
</script>

<template>
  <n-modal :show="open" preset="card" :title="t('安装 MOD')" :bordered="false"
    style="width: min(720px, 92vw)" @update:show="(value: boolean) => { if (!value) void close() }">
    <n-tabs v-model:value="installTab" type="segment" animated>
      <n-tab-pane name="local" :tab="t('本地文件')">
        <!--
          拖放区：点击也能打开文件选择器。
          Tauri 接管 WebView2 的拖放后会固定显示「禁止」光标（它只发事件、
          不接受放置），CSS 无法覆盖那个系统光标。所以改用拖入时的高亮
          来表达「可以放」：进入即描边 + 底色提亮 + 加号放大，指针形态
          不再承担提示职责。
        -->
        <div v-if="!preview" class="drop-zone" :class="{ active: dragActive, busy: awaitingSource }"
          role="button" tabindex="0" @click="pickLocalMod" @keydown.enter.prevent="pickLocalMod">
          <n-spin v-if="awaitingSource" size="small" />
          <template v-else>
            <span class="drop-plus">+</span>
            <span class="drop-hint">{{ t('把 .dll 或 .zip 拖到这里，或点击选择文件') }}</span>
          </template>
        </div>
      </n-tab-pane>
      <n-tab-pane name="online" :tab="t('在线直链')">
        <div v-if="!preview" class="online-form">
          <p class="muted">{{ t('粘贴 zip 或 DLL 的下载直链，由管理器下载并解析。') }}</p>
          <n-input v-model:value="installUrl" :placeholder="t('https://example.com/mod.zip')" clearable />
          <div class="install-actions">
            <n-button type="primary" :loading="awaitingSource" :disabled="awaitingSource"
              @click="downloadAndAnalyze">
              <LinkOutline width="14" height="14" />{{ t('下载并解析') }}
            </n-button>
          </div>
        </div>
      </n-tab-pane>
    </n-tabs>

    <!--
      预览：单 mod 显示完整元数据；包显示逐 DLL 列表（mod / 依赖）。
      与仓库安装后看到的详情完全一致 —— 三个入口共用这一个组件。
    -->
    <div v-if="preview" class="install-preview">
      <div class="preview-head">
        <b>{{ preview.package_name }}</b>
        <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
          {{ preview.kind === 'single' ? t('单个 MOD') : t('MOD 包') }}
        </n-tag>
        <span class="muted">
          {{ t('{count} 个 DLL · {mods} 个 MOD · {deps} 个依赖',
            { count: preview.total_dlls, mods: preview.mod_count, deps: preview.dependency_count }) }}
        </span>
      </div>

      <!-- 单 mod：逐项展示解析出的元数据 -->
      <div v-if="preview.kind === 'single'" class="preview-grid">
        <div class="preview-item"><span class="muted">{{ t('名称') }}</span><b>{{ preview.entries[0]?.name || '—' }}</b></div>
        <div class="preview-item"><span class="muted">GUID</span><b class="mono">{{ preview.entries[0]?.guid || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('版本') }}</span><b class="mono">{{ preview.entries[0]?.version || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('标题') }}</span><b>{{ preview.entries[0]?.title || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('产品名') }}</span><b>{{ preview.entries[0]?.product_name || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('作者') }}</span><b>{{ preview.entries[0]?.company || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('原始文件名') }}</span><b class="mono">{{ preview.entries[0]?.original_filename || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('文件') }}</span><b class="mono">{{ preview.entries[0]?.path || '—' }}</b></div>
      </div>

      <!-- 包：只列出每个 DLL 的名称 + 版本，依赖单独标注 -->
      <div v-else class="preview-list">
        <div v-for="entry in preview.entries" :key="entry.path" class="preview-row">
          <n-tag size="tiny" :bordered="false" class="state-tag"
            :color="tagColorFor(entry.role === 'dependency' ? 'muted' : 'accent')">
            {{ entry.role === 'dependency' ? t('依赖') : t('MOD') }}
          </n-tag>
          <b class="preview-name">{{ entry.name || entry.path }}</b>
          <span class="muted mono">{{ entry.version || '—' }}</span>
        </div>
        <div v-if="!preview.entries.length" class="muted">{{ t('这个压缩包里没有 DLL 文件') }}</div>
      </div>

      <div class="install-actions">
        <n-button secondary :disabled="installing" @click="resetPreview">
          {{ t('重新选择') }}
        </n-button>
        <n-button type="primary" :loading="installing" @click="confirmInstall">
          {{ t('立即安装') }}
        </n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
/*
 * 拖放区：虚线矩形 + 居中加号，点击等效于「选择文件」。
 * 拖入时叠加四个信号（实线描边 / 底色 / 外发光 / 加号放大），
 * 因为系统光标始终显示「禁止」，只能由区域自身表达可放置。
 */
.drop-zone {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-height: 140px;
  padding: 20px;
  border: 2px dashed var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-sunken);
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease,
    box-shadow .16s ease, transform .16s ease;
  user-select: none;
}
.drop-zone:hover,
.drop-zone:focus-visible { border-color: var(--oc2-accent); outline: none; }
.drop-zone.active {
  border-style: solid;
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 14%, var(--oc2-sunken));
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--oc2-accent) 28%, transparent);
}
.drop-zone.active .drop-plus { transform: scale(1.25); }
.drop-zone.active .drop-hint { color: var(--oc2-text-1); }
.drop-zone.busy { cursor: progress; }
.drop-plus {
  font-size: 34px;
  line-height: 1;
  font-weight: 300;
  color: var(--oc2-accent);
  transition: transform .16s ease;
}
.drop-hint {
  font-size: 12.5px;
  color: var(--oc2-text-3);
  transition: color .16s ease;
}
/* 预览区：与详情区同款的紧凑信息密度 */
.install-preview {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--oc2-row-line);
}
.preview-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.preview-head b { font-size: 13.5px; color: var(--oc2-text-1); }
.preview-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
}
.preview-item { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.preview-item span { font-size: 11px; }
.preview-item b {
  font-size: 12px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow-wrap: anywhere;
}
.preview-list { display: flex; flex-direction: column; gap: 6px; max-height: 280px; overflow-y: auto; }
.preview-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border-radius: 8px;
  background: var(--oc2-sunken);
}
.preview-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 底部操作行：靠右 */
.install-actions { margin-top: 4px; display: flex; justify-content: flex-end; gap: 8px; }
.online-form { display: flex; flex-direction: column; gap: 10px; }
</style>
