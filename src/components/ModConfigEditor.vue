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
  instance.onDidChangeModelContent(() => { dirty.value = !!model?.getValue() })
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
   * 关闭前先释放编辑器：DOM 即将卸载，若等 watch 里再 dispose，
   * Monaco 会在容器被移除的过程中做最后一次重排，可能卡住主线程。
   * 先 dispose 再关弹窗，DOM 移除时已没有实例在监听尺寸变化。
   */
  disposeEditor()
  if (dirty.value) {
    message.warning(t('有未保存的修改，已取消'))
  }
  emit('update:show', false)
}

/**
 * 释放 Monaco 实例。
 *
 * 关键点：`editor.dispose()` 会**连带销毁它持有的 model**，
 * 之后若再调 `model.dispose()` 就是二次释放，Monaco 内部状态被破坏，
 * 表现出来就是关闭弹窗时整个应用卡死。
 * 所以先把 model 从编辑器上 detach，再各自销毁，并且保证只执行一次。
 */
function disposeEditor() {
  disposed = true
  const instance = editor.value
  const textModel = model
  editor.value = null
  model = null
  if (instance) {
    // 先摘掉 model，避免 dispose 时走内部销毁链
    try { instance.setModel(null) } catch { /* 已销毁 */ }
    try { instance.dispose() } catch { /* 已销毁 */ }
  }
  if (textModel) {
    try { textModel.dispose() } catch { /* 已销毁 */ }
  }
}

watch(
  () => props.show,
  (visible) => {
    if (visible) void load()
    else disposeEditor()
  },
)

// 切换 MOD（同一弹窗复用时）重建编辑器
watch(
  () => props.dllFile,
  () => { if (props.show) { disposeEditor(); void load() } },
)

onMounted(() => { if (props.show) void load() })
onBeforeUnmount(disposeEditor)
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
  <AppModal :show="show" :mask-closable="false" display-directive="if" :title="title"
    to=".page-scroll" max-width="min(1000px, 96%)"
    max-height="85%" min-height="460px" @update:show="(value: boolean) => emit('update:show', value)">
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
 * 卡片外观、遮罩范围与滚动全部交给 AppModal（通过 to=".page-scroll"）。
 * 这里只保留编辑器自身的两件事：让 Monaco 容器撑满可用高度、出错时的提示。
 */
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
 * Monaco 容器必须撑满剩余空间，但外层是「内容驱动高度」的滚动区，
 * flex: 1 在这里没有可分配的高度（父级高度由内容决定），会塌成 0。
 * 所以用 flex: 1 配合 min-height 给一个确定下限，再由 AppModal 的
 * max-height 封顶 —— 两者共同决定编辑器的实际高度。
 */
.config-editor {
  flex: 1 1 auto;
  min-height: 320px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  overflow: hidden;
}
.config-editor.hidden { visibility: hidden; min-height: 0; height: 0; }
</style>
