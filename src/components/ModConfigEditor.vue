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

const props = defineProps<{
  show: boolean
  dllFile: string
  title: string
}>()
const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()

/**
 * 弹窗挂载点：sidebar 右侧的内容区。
 *
 * n-modal 默认 Teleport 到 body 并铺满整个窗口（含侧栏）；
 * 这里改为挂到 .page-scroll 元素上，遮罩只覆盖内容区，
 * 侧栏保持可交互，视觉上也符合「在容器内弹出」。
 * 注意必须传真实 DOM 元素（字符串选择器在挂载时机上不可靠）。
 */
const mountTarget = ref<HTMLElement | null>(null)

function resolveMountTarget() {
  if (typeof document === 'undefined') return
  mountTarget.value = document.querySelector<HTMLElement>('.page-scroll')
}

onMounted(resolveMountTarget)
watch(() => props.show, (visible) => { if (visible) resolveMountTarget() })

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

async function createEditor(content: string) {
  const api = await ensureMonaco()
  if (!container.value) return
  const dark = themeMode.value === 'dark'
  editor.value = api.editor.create(container.value, {
    value: content,
    language: 'ini',
    theme: dark ? 'vs-dark' : 'vs',
    automaticLayout: true,
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
  model = editor.value.getModel()
  editor.value.onDidChangeModelContent(() => { dirty.value = !!model?.getValue() })
}

async function load() {
  loading.value = true
  error.value = ''
  dirty.value = false
  try {
    const document = await api.readModConfig(props.dllFile)
    fileName.value = document.file_name
    revision.value = document.revision
    // 等容器渲染出来再建编辑器
    await new Promise((resolve) => requestAnimationFrame(resolve))
    await createEditor(document.content)
  } catch (reason) {
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
  if (dirty.value) {
    message.warning(t('有未保存的修改，已取消'))
  }
  emit('update:show', false)
}

function disposeEditor() {
  editor.value?.dispose()
  editor.value = null
  model?.dispose()
  model = null
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
  <n-modal :show="show && !!mountTarget" :mask-closable="false" class="config-modal" :to="mountTarget ?? undefined"
    @update:show="(value: boolean) => emit('update:show', value)">
    <div class="config-shell">
      <div class="config-head">
        <b class="config-title">{{ title }}</b>
        <n-tag size="tiny" :bordered="false" class="state-tag state-tag--muted">{{ fileName || '—' }}</n-tag>
        <n-tag v-if="dirty" size="tiny" :bordered="false" class="state-tag state-tag--warn">{{ t('未保存') }}</n-tag>
        <span class="config-actions">
          <n-button size="small" type="primary" :loading="saving" :disabled="!canSave" @click="save">
            <SaveOutline width="14" height="14" />{{ t('保存') }}
          </n-button>
          <n-button size="small" quaternary @click="close">
            <CloseOutline width="14" height="14" />{{ t('关闭') }}
          </n-button>
        </span>
      </div>

      <div v-if="error" class="config-error">{{ error }}</div>
      <n-spin v-else-if="loading" class="config-loading" size="small" />
      <div ref="container" class="config-editor" :class="{ hidden: loading || !!error }"></div>
    </div>
  </n-modal>
</template>

<style scoped>
/* 挂到 .page-scroll 后，弹窗按该容器定位（见模板里的 :to） */
.config-shell {
  display: flex;
  flex-direction: column;
  gap: 10px;
  /* 内容区宽度/高度留出边距，不按整个窗口算 */
  width: min(1000px, calc(100% - 48px));
  height: min(640px, calc(100% - 48px));
  max-height: calc(100% - 48px);
  padding: 14px 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 14px;
  background: var(--oc2-card);
}
/* 弹窗容器铺满 .page-scroll，使百分比尺寸有参照 */
:global(.page-scroll > .n-modal-container) {
  position: absolute;
  inset: 0;
  /* 弹窗内的滚动仍交给编辑器自身 */
  overflow: hidden;
}
.config-head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
.config-title {
  font-size: 14px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.config-actions {
  display: flex;
  gap: 6px;
  margin-left: auto;
  flex: 0 0 auto;
}
.config-error {
  color: #f87171;
  font-size: 12.5px;
  padding: 10px 0;
}
.config-loading { align-self: center; margin: 30px 0; }
.config-editor {
  flex: 1 1 0;
  min-height: 0;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  overflow: hidden;
}
.config-editor.hidden { visibility: hidden; height: 0; }
</style>
