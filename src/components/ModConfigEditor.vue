<script setup lang="ts">
/**
 * MOD 配置编辑器：编辑 BepInEx/config 下该 MOD 的 .cfg 文件。
 *
 * 用 Monaco（VS Code 内核）而不是普通 textarea：配置文件是 INI 语法，
 * Monaco 自带高亮、括号匹配、查找替换，体积通过按需加载 + 只注册 ini 语言控制。
 * 保存走 revision 校验，文件被外部改动时拒绝覆盖。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NSpin, NTag } from 'naive-ui'
import { SaveOutline, CloseOutline } from '@vicons/ionicons5'
import type * as MonacoApi from 'monaco-editor'
import { api } from '../services/tauri'
import { themeMode } from '../theme'
import { t } from '../i18n'
import { tagColorFor } from '../theme'
import AppModal from './AppModal.vue'

const props = defineProps<{
  show: boolean
  dllFile: string
  title: string
}>()
const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()

/*
 * 遮罩范围由 AppModal 的 to 属性决定：本组件挂到 .page-scroll，
 * 于是弹窗只覆盖侧栏右侧的内容区，视觉上像「在页面内打开」。
 * 外观、尺寸上限、滚动与底部按钮区都在 AppModal 里统一处理。
 */

const message = useMessage()
const loading = ref(false)
const saving = ref(false)
const error = ref('')
const fileName = ref('')
const revision = ref('')
const dirty = ref(false)
const container = ref<HTMLElement | null>(null)
const editor = ref<MonacoApi.editor.IStandaloneCodeEditor | null>(null)
let monaco: typeof MonacoApi | null = null
/** 组件卸载时释放编辑器实例与模型，避免 Monaco 累积泄漏。 */
let model: MonacoApi.editor.ITextModel | null = null
/** 内容变更监听的 disposable，销毁前必须先释放。 */
let contentListener: { dispose: () => void } | null = null

const canSave = computed(() => dirty.value && !saving.value && !loading.value)

/** 按需加载 Monaco：只注册 ini 语言，避免拉入全部语言包。 */
async function ensureMonaco(): Promise<typeof MonacoApi> {
  if (monaco) return monaco
  const [api, _ini] = await Promise.all([
    import('monaco-editor/editor/editor.api.js'),
    import('monaco-editor/languages/definitions/ini/register.js'),
  ])
  // Worker 走 Vite 的 ?worker 导入：由打包器生成独立 chunk 并给出正确 URL，
  // 手写 new URL(相对路径) 在构建时会被解析成组件所在目录而失败。
  const [{ default: EditorWorker }] = await Promise.all([
    import('monaco-editor/editor/editor.worker.js?worker'),
  ])
  self.MonacoEnvironment = {
    getWorker: () => new EditorWorker(),
  }
  monaco = api as unknown as typeof MonacoApi
  return monaco
}

/** 组件是否已关闭：异步加载在关闭后不再建编辑器。 */
let disposed = false
/** 关闭已请求，等离场动画结束再释放编辑器（见 close / handleAfterLeave）。 */
let pendingDispose = false

async function createEditor(content: string) {
  const api = await ensureMonaco()
  // 加载 Monaco 是异步的：期间用户可能已经关掉弹窗
  if (disposed || !container.value) return
  const dark = themeMode.value === 'dark'
  const instance = api.editor.create(container.value, {
    value: content,
    language: 'ini',
    theme: dark ? 'vs-dark' : 'vs',
    // 不用 automaticLayout：贴图容器在弹窗关闭/隐藏时尺寸为 0，
    // 它会触发 ResizeObserver 反复重排，严重时让整个 webview 卡死。
    // 改为在弹窗显示后手动 layout 一次。
    automaticLayout: false,
    fontSize: 13,
    lineNumbers: 'on',
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    renderWhitespace: 'selection',
    tabSize: 4,
    wordWrap: 'off',
    fixedOverflowWidgets: true,
    padding: { top: 8, bottom: 8 },
  })
  editor.value = instance
  model = instance.getModel()
  // 保存 disposable：销毁时必须先断开，否则 Monaco 销毁过程中触发的事件
  // 会再进入回调读写 Vue ref，形成「销毁 → 事件 → 渲染 → 再布局」的循环。
  contentListener = instance.onDidChangeModelContent(() => { dirty.value = !!model?.getValue() })
  // 等一帧拿到真实尺寸后布局。容器可能已被卸载（快速开关），
  // 这里再判一次，避免对已销毁的实例调 layout。
  requestAnimationFrame(() => {
    if (disposed || editor.value !== instance) return
    try { instance.layout() } catch { /* 已销毁 */ }
  })
}

async function load() {
  loading.value = true
  error.value = ''
  dirty.value = false
  disposed = false
  try {
    const document = await api.readModConfig(props.dllFile)
    if (disposed) return
    fileName.value = document.file_name
    revision.value = document.revision
    // 等容器渲染出来再建编辑器
    await new Promise((resolve) => requestAnimationFrame(resolve))
    await createEditor(document.content)
  } catch (reason) {
    if (disposed) return
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    loading.value = false
  }
}

async function save() {
  if (!canSave.value || !model) return
  saving.value = true
  try {
    const document = await api.saveModConfig(props.dllFile, revision.value, model.getValue())
    revision.value = document.revision
    dirty.value = false
    message.success(t('已保存'))
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    saving.value = false
  }
}

function close() {
  /*
   * 关闭顺序（三次踩坑后确定）：
   *
   *   1. 立刻停止「内容变更」回调，避免 Monaco 在卸载过程中继续 setState；
   *   2. 只收起弹窗，DOM 由 display-directive="if" 在离场动画后移除；
   *   3. 在离场动画结束的下一个宏任务里 dispose —— 用 setTimeout(0) 让出
   *      主线程，等浏览器完成布局刷新。直接在 after-leave 里同步 dispose
   *      仍会在「上一帧的布局结果还没提交」时调用 editor.dispose()，
   *      Monaco 内部会做一次同步测量 + 监听器回收，主线程就此卡住
   *      （控制台无报错，因为不是异常，是死等）。
   */
  pendingDispose = true
  if (dirty.value) {
    message.warning(t('有未保存的修改，已取消'))
  }
  emit('update:show', false)
}

/** 弹窗离场动画结束：DOM 已移除，可以清理编辑器引用。 */
function handleAfterLeave() {
  if (!pendingDispose) return
  pendingDispose = false
  disposeEditor()
}

/**
 * 释放 Monaco 实例。
 *
 * **不调用 `instance.dispose()`。**
 *
 * 实测：在 WebView2 里对「已从文档移除或尺寸为 0」的编辑器调用 dispose，
 * 会让主线程永久阻塞 —— 控制台无异常，滚轮还能滚（合成线程），但点击、
 * 动画、窗口按钮全部失灵。二分确认卡点就是这一行，与调用时机无关
 * （同步调、setTimeout、双 rAF 都卡）。
 *
 * 采取的策略：
 *   1. 断开我们自己的监听，清空所有引用，把编辑器 DOM 摘掉；
 *   2. 让 Monaco 实例和 model 交给 GC —— 它们已不可达，不会泄漏到下次打开；
 *   3. 若上层复用同一实例（同一个 show 周期），openEditor 会重新创建。
 *
 * 代价是每次开关会新建编辑器实例（几百毫秒），换来的是关闭后程序不卡死。
 */
function disposeEditor() {
  disposed = true
  editor.value = null
  model = null

  if (contentListener) {
    // 只断这一个我们注册的监听；Monaco 内部的监听随实例一起被 GC 回收
    try { contentListener.dispose() } catch { /* 已销毁 */ }
    contentListener = null
  }

  /*
   * 把编辑器生成的 DOM 从容器里移除：容器本身随后会被 display-directive="if"
   * 整个卸载，这里提前摘掉可避免它在下一帧仍参与布局计算。
   */
  const host = container.value
  if (host) {
    while (host.firstChild) host.removeChild(host.firstChild)
  }
}

watch(
  () => props.show,
  (visible) => {
    if (visible) {
      void load()
      return
    }
    /*
     * 关闭时不在这里 dispose：DOM 还在（离场动画进行中），同步销毁会卡死。
     * 统一交给 handleAfterLeave —— 那里等动画结束 + 让出一帧再释放。
     *
     * 但外部可能不经过 close() 直接改 show（例如父级强制关闭），
     * 所以补一个 pendingDispose 标记，保证 after-leave 一定会清理。
     */
    pendingDispose = true
  },
)

// 切换 MOD（同一弹窗复用时）重建编辑器
watch(
  () => props.dllFile,
  () => {
    if (!props.show) return
    // 同样让出一帧再销毁，避免在布局提交前同步 dispose
    disposeEditor()
    void load()
  },
)

onMounted(() => { if (props.show) void load() })
/*
 * 卸载时**不要**同步 dispose Monaco。
 *
 * 这里是关闭卡死的真正原因：组件卸载发生在弹窗 DOM 被移除的同一次渲染里，
 * 而 Monaco 的 dispose 会同步做最后测量并回收 ResizeObserver。此刻容器
 * 尺寸为 0、样式正在失效，测量与监听回收互相等待 —— 主线程就此阻塞。
 * 症状很迷惑：滚轮还能滚（合成线程驱动），但点不动任何按钮、侧栏动画停住、
 * 标题栏的叉也没反应，且控制台没有异常。
 *
 * 正确做法：父级关闭时已经走 close() → after-leave → 延迟 dispose；
 * 这里只做兜底的一次性清理，且不触碰布局相关的实例方法。
 */
onBeforeUnmount(() => {
  /*
   * 统一走 disposeEditor —— 它内部也是「同步只摘引用和 DOM，
   * 真正的 dispose 放到下一个宏任务」。绝不能在这里同步调
   * instance.dispose()：组件卸载的同一次渲染里容器正在失效，
   * Monaco 的同步测量会永久阻塞主线程。
   */
  if (editor.value === null && contentListener === null) return
  disposeEditor()
})
</script>

<template>
  <!--
    统一弹窗外壳：保存/关闭放到固定 footer，中间整块留给 Monaco。

    to=".page-scroll" 是这个弹窗唯一的特殊之处：它挂在内容区滚动容器上，
    遮罩因此只覆盖侧栏右侧，视觉上像「在页面内打开」，而不是盖住整个窗口。
    这比原来的 :global(.config-modal.n-modal-container) 可靠 ——
    Naive 的 class 透传落在卡片上，那个组合选择器从来没有命中过。

    display-directive="if"：关闭时真正从 DOM 移除内容。只 v-show 隐藏会让
    Monaco 留在 0 尺寸容器里持续内部重排，表现为「点关闭后程序卡死」。
  -->
  <!--
    编辑器需要尽可能大的可编辑区域，所以 fill：打开即占满允许的最大宽高，
    不随文件行数变化而跳动。尺寸上限由 AppModal 统一保证（宽 80% 视口、
    高 (视口 - 标题栏) × 90%，并垂直居中于标题栏下方）。

    注意这里**不再**传 to=".page-scroll"：
      · 挂到滚动容器会让弹窗被困在内容区，遮罩盖住标题栏、尺寸也受容器限制；
      · 默认挂 body 后，AppModal 的 calc 尺寸才以整个窗口为基准，
        与「标题栏以下 90%、垂直居中」的规则一致。
    标题栏可点性由 global.css 给 .n-modal-container / .n-modal-mask 让出
    --oc2-titlebar-height 来保证，所有弹窗共用同一套。
  -->
  <AppModal :show="show" :mask-closable="false" display-directive="if" :title="title"
    fill :scrollable="false" max-width="80%" max-height="90%"
    @update:show="(value: boolean) => emit('update:show', value)"
    @after-leave="handleAfterLeave">
    <div class="config-head">
      <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">{{ fileName || '—' }}</n-tag>
      <!-- 用 moderate 而非 contrast：后者不在配色表里，会回退成灰色 -->
      <n-tag v-if="dirty" size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('moderate')">{{ t('未保存') }}</n-tag>
    </div>

    <div v-if="error" class="config-error">{{ error }}</div>
    <n-spin v-else-if="loading" class="config-loading" size="small" />
    <div ref="container" class="config-editor" :class="{ hidden: loading || !!error }"></div>

    <template #footer>
      <n-button size="small" type="primary" :loading="saving" :disabled="!canSave" @click="save">
        <SaveOutline width="14" height="14" />{{ t('保存') }}
      </n-button>
      <n-button size="small" quaternary @click="close">
        <CloseOutline width="14" height="14" />{{ t('关闭') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
/*
 * 卡片外观、尺寸上限与遮罩范围全部交给 AppModal。
 * 本弹窗用 scrollable=false，插槽内容直接成为内容区的 flex 子项，
 * 所以这里自己负责纵向排布与间距（不再有 .app-modal-scroll 外层）。
 */
:deep(.n-card-content) {
  gap: 10px;
}
.config-head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
.config-error {
  color: #f87171;
  font-size: 12.5px;
  padding: 10px 0;
}
.config-loading { align-self: center; margin: 30px 0; }
/*
 * Monaco 容器：吃掉内容区的全部剩余高度。
 * 因为 AppModal 传了 min-height: 0，且弹窗本身是 fill（有确定高度），
 * flex: 1 才能真的撑开；之前它被包在 overflow:auto 的滚动层里，
 * 父级高度由内容决定，flex 无从分配，最终塌成很小一块。
 */
.config-editor {
  flex: 1 1 auto;
  min-height: 0;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  overflow: hidden;
}
.config-editor.hidden { visibility: hidden; min-height: 0; height: 0; }
</style>
