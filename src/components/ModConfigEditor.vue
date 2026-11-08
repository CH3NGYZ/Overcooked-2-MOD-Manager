<script setup lang="ts">
/**
 * MOD 配置编辑器：编辑 BepInEx/config 下该 MOD 的 .cfg 文件。
 *
 * **为什么用 `n-input` 而不是 Monaco。**
 *
 * 这里编辑的是 BepInEx 的 INI 配置文件，通常几十行、结构简单，用户要做的
 * 是改个别值（语言、开关、数值），不需要跳转、重构或语法分析。Monaco 带来
 * 的能力在这个场景几乎用不到，却引入了无法回避的问题：
 *   · 每次开合都要在 WebView2 里创建/丢弃实例，历史上多次出现主线程永久
 *     阻塞（`editor.dispose()`、手动摘 DOM、在 `height: 0` 容器里初始化）；
 *   · 打包体积约 2.7 MB，另加一个独立 Editor Worker；
 *   · 容器尺寸必须由外部保证，而弹窗的高度链是 flex 派生的，二者容易打架，
 *     表现为编辑器只渲染一半高度。
 *
 * `n-input type="textarea"` 是 Naive 原生组件，靠 CSS 撑满容器、由浏览器
 * 自己处理滚动与选择，没有实例生命周期，也就没有卡死面。等宽字体 + 保留
 * 换行 + 禁用拼写检查，对 INI 足够。
 *
 * 保存走 revision 校验：文件被外部改动时后端拒绝覆盖，避免冲掉别人的修改。
 */
import { computed, ref, watch } from 'vue'
import { NButton, NInput, NTag } from 'naive-ui'
import { SaveOutline, CloseOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { tagColorFor } from '../theme'
import AppModal from './AppModal.vue'
import { backendMessageOf } from '../locale/backend'
import { useGameOperationGate } from '../composables/useGameOperationGate'

const props = defineProps<{
  show: boolean
  /**
   * 数据源：
   *   · `mod`（默认）—— 某个 MOD 在 `BepInEx/config` 下的配置文件，按 `dllFile` 定位；
   *   · `bepinex` —— BepInEx 框架自身的 `BepInEx.cfg`，与具体 MOD 无关。
   * 两种来源的读写命令不同，但界面与 revision 校验完全一致，所以合并到一个组件。
   */
  source?: 'mod' | 'bepinex'
  /** MOD 配置文件对应的 DLL；`source === 'bepinex'` 时忽略。 */
  dllFile?: string
  title: string
}>()
const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()

const message = useNotify()
const { blockModOperation } = useGameOperationGate()
const loading = ref(false)
const saving = ref(false)
const error = ref('')
const fileName = ref('')
const revision = ref('')
const dirty = ref(false)
/** 编辑器正文：直接用 n-input 的 v-model，不再有独立实例。 */
const content = ref('')

const canSave = computed(() => dirty.value && !saving.value && !loading.value)

/** 当前数据源；缺省为 MOD 配置。 */
const source = computed(() => props.source ?? 'mod')

/** 读取当前数据源的文件内容。 */
function readDocument() {
  return source.value === 'bepinex'
    ? api.readBepInExConfig()
    : api.readModConfig(props.dllFile ?? '')
}

/** 写回当前数据源。 */
function saveDocument(nextRevision: string, nextContent: string) {
  return source.value === 'bepinex'
    ? api.saveBepInExConfig(nextRevision, nextContent)
    : api.saveModConfig(props.dllFile ?? '', nextRevision, nextContent)
}

/**
 * 加载令牌：父组件在同一 tick 里改 `dllFile` 与 `show`，
 * 关闭后立即重开时会有过期请求回来，用它丢弃迟到的结果。
 */
let loadToken = 0

async function load() {
  const token = ++loadToken
  loading.value = true
  error.value = ''
  dirty.value = false
  try {
    const document = await readDocument()
    if (token !== loadToken) return
    fileName.value = document.file_name
    revision.value = document.revision
    content.value = document.content
  } catch (reason) {
    if (token !== loadToken) return
    error.value = backendMessageOf(reason)
  } finally {
    if (token === loadToken) loading.value = false
  }
}

async function save() {
  if (blockModOperation()) return
  if (!canSave.value) return
  saving.value = true
  try {
    const document = await saveDocument(revision.value, content.value)
    revision.value = document.revision
    dirty.value = false
    message.success(t('common.saved'))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    saving.value = false
  }
}

function close() {
  if (dirty.value) message.warning(t('tools.unsavedChangesWereDiscarded'))
  emit('update:show', false)
}

/**
 * 只在「打开」或「换了数据源 / MOD」时重新读取。
 *
 * 用组合键而不是分别 watch show / dllFile：两者在同一 tick 一起变化时，
 * 分开监听会触发两次读取。用 immediate 覆盖首次挂载即为打开的情况。
 * `bepinex` 数据源没有 dllFile，用固定的 `source` 值当 key。
 */
watch(
  () => (props.show ? (source.value === 'bepinex' ? 'bepinex' : props.dllFile ?? '') : ''),
  (key) => {
    if (!key) return
    void load()
  },
  { immediate: true },
)

/** 用户编辑正文：标记未保存。 */
function onInput(value: string) {
  content.value = value
  dirty.value = true
}

/** 外部换了 dllFile 但弹窗仍打开（正常操作不会发生）：清掉脏标记重读。 */
watch(
  () => props.dllFile,
  () => { if (props.show && source.value === 'mod') void load() },
)
</script>

<template>
  <!--
    统一弹窗外壳：保存/关闭在固定 footer，中间整块留给编辑区。
    尺寸上限由 AppModal 统一保证（宽 95% 视口、高 (视口 - 标题栏) × 95%）。
  -->
  <AppModal :show="show" :mask-closable="false" display-directive="if" :title="title"
    fill :scrollable="false" max-width="95%" max-height="95%"
    @update:show="(value: boolean) => emit('update:show', value)">
    <div class="config-head">
      <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">{{ fileName || '—' }}</n-tag>
      <n-tag v-if="dirty" size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('moderate')">{{ t('tools.unsaved') }}</n-tag>
    </div>

    <div v-if="error" class="config-error">{{ error }}</div>
    <!--
      多行文本域：flex 撑满剩余高度，等宽字体保留 INI 的缩进与对齐。
      spellcheck / autocorrect 关掉，否则中文与路径会被输入法/拼写检查改写。
      这不是表单校验字段，所以不套 n-form-item。
    -->
    <n-input v-else :value="content" type="textarea" class="config-text" :loading="loading"
      :placeholder="loading ? t('modsOnline.readingConfiguration') : ''" :disabled="loading" spellcheck="false"
      autocorrect="off" autocapitalize="off" @update:value="onInput" />

    <template #footer>
      <n-button size="small" type="primary" :loading="saving" :disabled="!canSave" @click="save">
        <SaveOutline width="14" height="14" />{{ t('modsOnline.save') }}
      </n-button>
      <n-button size="small" quaternary @click="close">
        <CloseOutline width="14" height="14" />{{ t('nav.close') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
/*
 * 卡片外观、尺寸上限与遮罩范围全部交给 AppModal。
 * 本弹窗用 scrollable=false，插槽内容直接成为内容区的 flex 子项，
 * 所以这里自己负责纵向排布与间距。
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
/*
 * 文本域吃掉内容区的全部剩余高度。
 *
 * 高度链：AppModal 的 .n-card-content 是 flex 且有 min-height: 0，
 * 本组件是 scrollable=false，插槽内容直接成为它的子项；所以这里
 * flex: 1 + min-height: 0 能真的撑开。n-input 内部两层
 * （.n-input / textarea）都要 100% 高度，否则只显示默认 3 行。
 */
.config-text {
  flex: 1 1 auto;
  min-height: 0;
  height: auto;
}
.config-text :deep(.n-input__textarea),
.config-text :deep(.n-input__textarea-el) {
  height: 100%;
}
.config-text :deep(.n-input__textarea-el) {
  font-family: Consolas, 'Courier New', monospace;
  font-size: 13px;
  line-height: 1.5;
  /* INI 文件常有对齐用的空格，保留它们；超长行横向滚动而不是折行 */
  white-space: pre;
  overflow-wrap: normal;
  overflow-x: auto;
  resize: none;
}
</style>
