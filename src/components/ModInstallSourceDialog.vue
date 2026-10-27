<script setup lang="ts">
/**
 * 「安装 MOD」——选择来源并取得预览。
 *
 * 与 `ModInstallDialog`（只展示预览）分工明确：
 *   本组件负责**拿到**一份 ModPreview，方式有三种：
 *     · 本地文件（文件选择器）
 *     · 在线直链（后端下载一次到暂存）
 *     · 拖放文件（Tauri webview 事件给绝对路径）
 *   拿到后 emit `preview`，由宿主交给 ModInstallDialog 展示。
 *
 * 这样「怎么获取」和「怎么展示/确认」互不纠缠：新增来源只需在这里加
 * 一个分支，预览弹窗完全不用改。
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { NButton, NInput, NSpin, NTabPane, NTabs, useMessage } from 'naive-ui'
import { LinkOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { isTauri } from '../runtime'
import { t } from '../i18n'
import AppModal from './AppModal.vue'
import type { ModPreview, ModSourceKind } from '../types'

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /** 已经拿到一份可展示的预览。 */
  (event: 'preview', preview: ModPreview): void
}>()

const message = useMessage()
const sourceTab = ref<'local' | 'online'>('local')
const url = ref('')
const busy = ref(false)
const dragActive = ref(false)

const open = ref(props.show)
watch(() => props.show, (value) => { open.value = value })
watch(open, (value) => {
  emit('update:show', value)
  if (value) {
    sourceTab.value = 'local'
    url.value = ''
  }
})

/** 把后端返回的预览交给宿主，收起本面板。 */
function handOff(preview: ModPreview) {
  emit('preview', preview)
  open.value = false
}

/** 统一解析入口：本地路径或在线 URL。 */
async function analyze(kind: ModSourceKind, value: string) {
  busy.value = true
  try {
    handOff(await api.analyzeModSource(kind, value))
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    busy.value = false
  }
}

async function pickLocal() {
  const path = await api.chooseModFile()
  if (path) await analyze('local', path)
}

async function downloadOnline() {
  const trimmed = url.value.trim()
  if (!/^https?:\/\//i.test(trimmed)) {
    message.warning(t('请输入 zip 或 DLL 的直链网址'))
    return
  }
  await analyze('url', trimmed)
}

/* ===== 拖放：Tauri 给绝对路径，不是浏览器 File 对象 ===== */

let dragUnlisten: (() => void) | null = null

onMounted(() => {
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
        // 拖入即安装：自动打开本面板并开始解析
        open.value = true
        sourceTab.value = 'local'
        void analyze('local', path)
      })
    } catch (reason) {
      console.error('[drop] listener registration failed', reason)
    }
  })()
})

onBeforeUnmount(() => { dragUnlisten?.(); dragUnlisten = null })
</script>

<template>
  <!-- 复用统一弹窗外壳：不传 #footer，内容区独占剩余高度并自己滚动 -->
  <AppModal :show="open" :title="t('安装 MOD')" max-width="min(620px, 92%)"
    max-height="min(600px, 80%)" @update:show="(value: boolean) => (open = value)">
    <n-tabs v-model:value="sourceTab" type="segment" animated>
      <n-tab-pane name="local" :tab="t('本地文件')">
        <!--
          拖放区：点击也能打开文件选择器。
          Tauri 接管 WebView2 的拖放后系统光标固定是「禁止」（UIPI 之外还有
          DROPEFFECT_NONE），CSS 覆盖不了，所以用四重高亮表达「可以放」。
        -->
        <div class="drop-zone" :class="{ active: dragActive, busy }"
          role="button" tabindex="0" @click="pickLocal" @keydown.enter.prevent="pickLocal">
          <n-spin v-if="busy" size="small" />
          <template v-else>
            <span class="drop-plus">+</span>
            <span class="drop-hint">{{ t('把 .dll 或 .zip 拖到这里，或点击选择文件') }}</span>
          </template>
        </div>
      </n-tab-pane>
      <n-tab-pane name="online" :tab="t('在线直链')">
        <div class="online-form">
          <p class="muted">{{ t('粘贴 zip 或 DLL 的下载直链，由管理器下载并解析。') }}</p>
          <n-input v-model:value="url" :placeholder="t('https://example.com/mod.zip')" clearable />
          <div class="actions">
            <n-button type="primary" :loading="busy" :disabled="busy" @click="downloadOnline">
              <LinkOutline width="14" height="14" />{{ t('下载并解析') }}
            </n-button>
          </div>
        </div>
      </n-tab-pane>
    </n-tabs>
  </AppModal>
</template>

<style scoped>
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
.online-form { display: flex; flex-direction: column; gap: 12px; }
.actions { display: flex; justify-content: flex-end; }
</style>
