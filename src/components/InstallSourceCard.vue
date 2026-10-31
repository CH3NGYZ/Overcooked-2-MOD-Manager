<script setup lang="ts">
/**
 * 安装来源卡片 —— 「本地文件 / 在线直链」两种取源方式只保留这一份实现。
 *
 * 与 `ModInstallDialog`（只展示预览）分工明确：本组件只负责**取到来源**并
 * `emit('select')`；拿到来源之后的事（MOD 走 `analyze_mod_source` 出预览、
 * 地图走 `stage_map_source` 出关卡集）由宿主决定，所以这里**不 import `api`**，
 * 连文件选择器都由宿主以 `pickFile` 传进来。
 *
 * 为什么要下沉：MOD 安装页、MOD 管理页、地图下载页都要「本地选文件 / 贴直链 /
 * 拖进窗口」这三件事。各自写一份的代价不是多几十行代码，而是三处校验规则、
 * 提示文案和拖放行为会慢慢分叉 —— 之前就是两页各挂一个来源弹窗。
 *
 * 三种取源方式：
 *   · 本地文件（`pickFile` 打开系统文件选择器）
 *   · 在线直链（只校验协议，能不能下载由宿主/后端判断）
 *   · 整窗拖放（Tauri webview 的 `onDragDropEvent` 给绝对路径，不是浏览器
 *     `File` 对象；浏览器预览里没有这个事件，`isTauri()` 挡掉）
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { NButton, NInput, NSpin, NTabPane, NTabs } from 'naive-ui'
import { LinkOutline } from '@vicons/ionicons5'
import { isTauri } from '../runtime'
import { t } from '../i18n'
import { useNotify } from '../useNotify'
import { backendMessageOf } from '../locale/backend'
import AppModal from './AppModal.vue'
import type { InstallSource } from '../types'

const props = withDefaults(defineProps<{
  show: boolean
  /**
   * 允许的扩展名（不带点）。同时决定提示文案里的类型列表，
   * 例如 `['dll', 'zip']` → 「把 .dll / .zip 拖到这里」。
   */
  accept?: string[]
  /** 卡片标题；不传按 MOD 安装来源的「安装 MOD」。 */
  title?: string
  /**
   * 打开系统文件选择器的实现。由宿主提供：MOD 传 `api.chooseModFile`
   * （DLL / ZIP），地图传 `api.chooseLevelSetZip`（ZIP）。返回 null = 用户取消。
   */
  pickFile: () => Promise<string | null>
  /**
   * 宿主正在处理上一个来源（下载 / 解析 / 暂存）。期间显示转圈并禁用入口，
   * 避免用户以为没反应又点一次。成功与否由宿主关闭卡片决定。
   */
  busy?: boolean
}>(), {
  accept: () => ['dll', 'zip'],
  title: '',
  busy: false,
})

const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /** 用户选好了一个来源：本地文件绝对路径，或在线直链 URL。 */
  (event: 'select', source: InstallSource): void
}>()

const message = useNotify()
const sourceTab = ref<'local' | 'online'>('local')
const url = ref('')
const dragActive = ref(false)

/**
 * 提示文案里的类型列表：`.dll / .zip`。
 *
 * 用语言无关的「/」连接而不是「或 / or / 또는」：扩展名本来就是英文，
 * 让每个语言表各自拼连接词只会多出五个容易写错的占位符。
 */
const typeLabel = computed(() => props.accept.map((extension) => `.${extension}`).join(' / '))

/** 扩展名白名单（`accept` 里都是代码里写死的字面量后缀，无需转义）。 */
const acceptPattern = computed(() => new RegExp(`\\.(${props.accept.join('|')})$`, 'i'))

function matchesAccept(path: string) {
  return acceptPattern.value.test(path)
}

const open = ref(props.show)
watch(() => props.show, (value) => { open.value = value })
watch(open, (value) => {
  emit('update:show', value)
  if (value) {
    sourceTab.value = 'local'
    url.value = ''
  }
})

/** 本地文件：选择器 → 校验扩展名 → 交给宿主。 */
async function pickLocal() {
  if (props.busy) return
  let path: string | null
  try {
    path = await props.pickFile()
  } catch (reason) {
    message.error(backendMessageOf(reason))
    return
  }
  if (!path) return
  if (!matchesAccept(path)) {
    message.warning(t('tools.onlyFileTypesAreSupported', { types: typeLabel.value }))
    return
  }
  emit('select', { kind: 'local', value: path })
}

/** 在线直链：只挡掉明显不是 URL 的输入，真正能不能下载由后端判断。 */
function downloadOnline() {
  if (props.busy) return
  const trimmed = url.value.trim()
  if (!/^https?:\/\//i.test(trimmed)) {
    message.warning(t('tools.enterADirectUrlForFiles', { types: typeLabel.value }))
    return
  }
  emit('select', { kind: 'url', value: trimmed })
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
        // 宿主正在处理上一条来源：这次拖入直接忽略，避免并发两条安装流程
        if (props.busy) return
        const path = payload.paths?.[0]
        if (!path) return
        if (!matchesAccept(path)) {
          message.warning(t('tools.onlyFileTypesAreSupported', { types: typeLabel.value }))
          return
        }
        // 拖入即安装：自动打开卡片并把路径交给宿主，省掉「再点一次选择文件」
        open.value = true
        sourceTab.value = 'local'
        emit('select', { kind: 'local', value: path })
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
  <AppModal :show="open" :title="title || t('tools.installMod')" max-width="min(620px, 92%)"
    max-height="min(600px, 80%)" @update:show="(value: boolean) => (open = value)">
    <n-tabs v-model:value="sourceTab" type="segment" animated>
      <n-tab-pane name="local">
        <template #tab>{{ t('tools.localFile') }}</template>
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
            <span class="drop-hint">{{ t('tools.dropFilesHereOrClickToChoose', { types: typeLabel }) }}</span>
          </template>
        </div>
      </n-tab-pane>
      <n-tab-pane name="online">
        <template #tab>{{ t('tools.directUrl') }}</template>
        <div class="online-form">
          <p class="muted">{{ t('tools.pasteADirectDownloadUrlForFiles', { types: typeLabel }) }}</p>
          <n-input v-model:value="url" :placeholder="t('https://example.com/mod.zip')" clearable />
          <div class="actions">
            <n-button type="primary" :loading="busy" :disabled="busy" @click="downloadOnline">
              <LinkOutline width="14" height="14" />{{ t('tools.downloadAndAnalyze') }}
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
