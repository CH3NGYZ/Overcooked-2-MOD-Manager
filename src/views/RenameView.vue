<script setup lang="ts">
/**
 * MOD 改名（多彩名字）：把逐字符颜色/字号编译成 MOD 可识别的富文本标签串，
 * 可复制或直接推送给正在运行的多彩名字 MOD。
 * 生成算法在 src/colorfulName.ts，推送走 Rust 命令（前端 CSP 不允许直连本地端口）。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { useMessage } from 'naive-ui'
import {
  NButton, NColorPicker, NInput, NInputNumber, NProgress, NSlider, NSpin, NTabPane, NTabs,
} from 'naive-ui'
import { ColorWandOutline, CopyOutline, CloudUploadOutline, RefreshOutline } from '@vicons/ionicons5'
import {
  DEFAULT_COLOR, DEFAULT_SIZE, MAX_NAME_BYTES, buildShortestRichText, isNameOverLimit, utf16LeBytes,
} from '../colorfulName'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { setPageTitleOverride } from '../pageHeader'

const message = useMessage()

/** MOD 内置的常用调色板（与游戏内命名习惯一致）。 */
const PALETTE = [
  '#000000', '#ffffff', '#ffa43b', '#5fa6de', '#ff0000',
  '#ffcc00', '#33cc33', '#0066ff', '#9933ff', '#ff66cc',
  '#666666', '#cccccc', '#990000', '#996600', '#999900',
  '#006600', '#006666', '#000099', '#660066', '#cc0066',
]

const rawName = ref('')
const name = ref('')
const colors = ref<string[]>([])
const sizes = ref<number[]>([])
const selected = ref<{ start: number; end: number } | null>(null)
const picker = ref(DEFAULT_COLOR)
const sizeStep = ref(0)
const pushing = ref(false)
const previewEl = ref<HTMLElement | null>(null)

/** 逐字符样式：颜色、字号数组与字符一一对应。 */
const chars = computed(() => [...name.value])
const code = computed(() =>
  buildShortestRichText(chars.value.map((char, index) => ({
    char,
    color: colors.value[index] ?? DEFAULT_COLOR,
    size: sizes.value[index] ?? DEFAULT_SIZE,
  }))),
)
const byteLength = computed(() => utf16LeBytes(code.value))
const overLimit = computed(() => isNameOverLimit(code.value))
const percent = computed(() => Math.min(100, Math.round((byteLength.value / MAX_NAME_BYTES) * 100)))

/**
 * 两个阶段左右滑动：
 *   input     = 输入名字
 *   customize = 开始自定义
 *
 * 页头是固定的（由 App 渲染，不参与页面滑动），本页只负责：
 *   1. 用 setPageTitleOverride 换标题文字，App 侧做淡入淡出；
 *   2. 用 Teleport 把第二阶段的操作按钮送进页头右侧。
 * 面板本身的横移用 n-tabs 的 animated —— 它自带方向感知的滑动过渡，
 * 比手写 keyframes 更容易和主题的缓动对齐。
 */
const stage = ref<'input' | 'customize'>('input')
watch(stage, (value) => setPageTitleOverride(value === 'customize' ? '开始自定义' : null))
onBeforeUnmount(() => setPageTitleOverride(null))

/** n-tabs 用 tab name 表达阶段。 */
const stageName = computed({
  get: () => stage.value,
  set: (value: string) => { stage.value = value === 'customize' ? 'customize' : 'input' },
})

function startEdit() {
  const text = rawName.value.trim()
  if (!text) { message.warning(t('请输入名字')); return }
  name.value = text
  colors.value = chars.value.map(() => DEFAULT_COLOR)
  sizes.value = chars.value.map(() => DEFAULT_SIZE)
  selected.value = null
  sizeStep.value = 0
  picker.value = DEFAULT_COLOR
  stage.value = 'customize'
  void nextTick(clearNativeSelection)
}

function resetAll() {
  name.value = ''
  colors.value = []
  sizes.value = []
  selected.value = null
  // 回到第一阶段：面板左滑回第一步，标题也跟着变回「改多彩名」
  stage.value = 'input'
}

/** 读取预览区里用户实际选中的字符范围。 */
function readSelection(): { start: number; end: number } | null {
  const element = previewEl.value
  const selection = window.getSelection()
  if (!element || !selection || !selection.rangeCount || selection.isCollapsed) return null
  const range = selection.getRangeAt(0)
  if (!element.contains(range.startContainer) || !element.contains(range.endContainer)) return null
  const spans = [...element.querySelectorAll<HTMLElement>('[data-index]')]
  const indexOf = (node: Node) => {
    const host = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement)
    const span = host?.closest<HTMLElement>('[data-index]')
    return span ? spans.indexOf(span) : -1
  }
  const start = indexOf(range.startContainer)
  const end = indexOf(range.endContainer)
  if (start < 0 || end < 0) return null
  return { start: Math.min(start, end), end: Math.max(start, end) + 1 }
}

function updateSelection() {
  const range = readSelection()
  if (range) {
    selected.value = range
    // 同步第一个字符的样式到控件，方便接着微调
    picker.value = colors.value[range.start] ?? DEFAULT_COLOR
    sizeStep.value = 0
  }
}

function clearNativeSelection() {
  window.getSelection()?.removeAllRanges()
}

/** 只改颜色，不改字号。 */
function applyColor(color: string) {
  const range = selected.value
  if (!range) { message.warning(t('请先选择要修改的文字')); return }
  picker.value = color
  const next = [...colors.value]
  for (let index = range.start; index < range.end; index += 1) next[index] = color.toLowerCase()
  colors.value = next
}

/** 字号按「基准 + 增量」应用，避免连续拖动叠加。 */
function applySize(step: number) {
  const range = selected.value
  if (!range) { message.warning(t('请先选择要修改的文字')); return }
  const next = [...sizes.value]
  for (let index = range.start; index < range.end; index += 1) {
    const base = sizes.value[index] ?? DEFAULT_SIZE
    next[index] = Math.max(1, Math.min(71, base + step))
  }
  sizes.value = next
  sizeStep.value = 0
}

function resetSelectionStyle() {
  const range = selected.value
  if (!range) { message.warning(t('请先选择要修改的文字')); return }
  const nextColors = [...colors.value]
  const nextSizes = [...sizes.value]
  for (let index = range.start; index < range.end; index += 1) {
    nextColors[index] = DEFAULT_COLOR
    nextSizes[index] = DEFAULT_SIZE
  }
  colors.value = nextColors
  sizes.value = nextSizes
  picker.value = DEFAULT_COLOR
  sizeStep.value = 0
}

async function copyCode() {
  if (overLimit.value) { message.error(t('名字代码超过限制')); return }
  try {
    await api.copyText(code.value)
    message.success(t('已复制，可粘贴到 MOD 菜单'))
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  }
}

async function pushToMod() {
  if (overLimit.value) { message.error(t('名字代码超过限制')); return }
  pushing.value = true
  try {
    const result = await api.pushColorfulName(code.value)
    result.ok ? message.success(result.message || t('已推送到 街机MOD')) : message.error(result.message)
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    pushing.value = false
  }
}

onBeforeUnmount(clearNativeSelection)
</script>

<template>
  <!--
    第二阶段的操作按钮 teleport 到固定页头的右侧。
    「重新输入」紧挨标题（放在标题补充区），「复制代码」在
    「推送到 街机MOD」左边，最右是推送。
  -->
  <Teleport v-if="stage === 'customize'" to="#page-header-title-extra" defer>
    <n-button size="small" secondary @click="resetAll">
      <RefreshOutline width="14" height="14" />{{ t('重新输入') }}
    </n-button>
  </Teleport>
  <Teleport v-if="stage === 'customize'" to="#page-header-actions" defer>
    <n-button size="small" secondary :disabled="overLimit" @click="copyCode">
      <CopyOutline width="14" height="14" />{{ t('复制代码') }}
    </n-button>
    <n-button size="small" type="primary" :loading="pushing" :disabled="overLimit" @click="pushToMod">
      <CloudUploadOutline width="14" height="14" />{{ t('推送到 街机MOD') }}
    </n-button>
  </Teleport>

  <div class="rename-stage">
    <n-tabs v-model:value="stageName" class="stage-tabs" :tabs-padding="0" animated>
      <!-- 阶段一：输入名字 -->
      <n-tab-pane name="input">
        <section class="panel">
          <div class="panel-head">
            <b>{{ t('输入名字') }}</b>
            <span class="muted">{{ t('支持任意字符，例如中文、英文与符号') }}</span>
          </div>
          <n-input v-model:value="rawName" type="textarea" :autosize="{ minRows: 2, maxRows: 4 }"
            :placeholder="t('在这里输入要修改的名字')" @keydown.enter.prevent="startEdit" />
          <div class="row-actions">
            <n-button type="primary" @click="startEdit">
              <ColorWandOutline width="15" height="15" />{{ t('开始自定义') }}
            </n-button>
          </div>
        </section>
      </n-tab-pane>

      <!-- 阶段二：选字改色 + 生成代码 -->
      <n-tab-pane name="customize">
        <section class="panel">
          <div class="panel-head">
            <b>{{ t('选择要修改的文字') }}</b>
            <span class="muted">{{ t('拖动选中预览里的字符，再应用颜色或字号') }}</span>
          </div>
          <div ref="previewEl" class="preview" @mouseup="updateSelection" @keyup="updateSelection">
            <span v-for="(char, index) in chars" :key="index" :data-index="index" class="preview-char"
              :style="{ color: colors[index], fontSize: `${sizes[index]}px` }">{{ char }}</span>
          </div>

          <div class="controls">
            <div class="control-block">
              <span class="control-label">{{ t('选择颜色') }}</span>
              <!-- 色板与自定义取色都用 Naive 的 color-picker（自带色块、取色器、清空） -->
              <n-color-picker v-model:value="picker" :show-alpha="false" size="small" class="picker"
                :modes="['hex']" :swatches="PALETTE"
                @update:value="(value: string) => applyColor(value)" />
            </div>

            <div class="control-block">
              <span class="control-label">{{ t('字号增量') }}</span>
              <div class="size-row">
                <n-slider :value="sizeStep" :min="-34" :max="36" :step="1" class="size-slider"
                  @update:value="(value: number) => (sizeStep = value)"
                  @dragend="applySize(sizeStep)" />
                <n-input-number :value="sizeStep" size="small" :min="-34" :max="36" class="size-input"
                  @update:value="(value: number | null) => applySize(value ?? 0)" />
                <n-button size="small" secondary @click="resetSelectionStyle">
                  <RefreshOutline width="14" height="14" />{{ t('重置样式') }}
                </n-button>
              </div>
            </div>
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <b>{{ t('生成的名字代码') }}</b>
            <!--
              容量指示：圆角标签本身就是进度条。
              n-progress 的填充色画在标签底层，文字叠在上层，
              这样不再需要标签之外单独占一行的进度条。
            -->
            <span class="byte-chip" :class="{ over: overLimit }">
              <n-progress class="byte-chip-bar" type="line" :percentage="percent" :show-indicator="false"
                :height="20" :border-radius="999" :status="overLimit ? 'error' : 'success'" />
              <span class="byte-chip-text">{{ byteLength }} / {{ MAX_NAME_BYTES }} B</span>
            </span>
          </div>
          <n-input :value="code" type="textarea" readonly :autosize="{ minRows: 2, maxRows: 6 }" class="code" />
          <p v-if="overLimit" class="over muted">{{ t('名字代码超过 MOD 的 238 字节限制，请减少代码长度') }}</p>
          <p v-else class="muted">{{ t('复制后粘贴到游戏内 MOD 菜单，或直接推送到正在运行的 MOD') }}</p>
        </section>

        <n-spin v-if="pushing" class="push-hint" size="small" />
      </n-tab-pane>
    </n-tabs>
  </div>
</template>

<style scoped>
.rename-stage {
  width: 100%;
}
/*
 * 隐藏 n-tabs 的标签导航：阶段切换由「开始自定义 / 重新输入」按钮驱动，
 * 标签条本身不该出现在界面上。用 visibility 而不是 display，
 * 因为 n-tabs 依赖导航元素测量位置来播放滑动过渡（display:none 会让它为 0）。
 */
.stage-tabs :deep(.n-tabs-nav) {
  display: none;
}
/* 面板主体不额外加内边距（间距由 .rename-stage 与各 .panel 负责） */
.stage-tabs :deep(.n-tab-pane) {
  padding: 0;
}
/*
 * Naive 的 animated 默认只位移 32px，用于标签页内的轻量过渡。
 * 这里要做「整屏左滑」，把位移放大到一个内容宽度，
 * 并保持与原来一致的缓动与时长。
 */
.stage-tabs :deep(.n-tab-pane.next-transition-enter-from),
.stage-tabs :deep(.n-tab-pane.prev-transition-leave-to) {
  transform: translateX(100%);
}
.stage-tabs :deep(.n-tab-pane.next-transition-leave-to),
.stage-tabs :deep(.n-tab-pane.prev-transition-enter-from) {
  transform: translateX(-100%);
}
.stage-tabs :deep(.n-tab-pane.next-transition-leave-active),
.stage-tabs :deep(.n-tab-pane.prev-transition-leave-active),
.stage-tabs :deep(.n-tab-pane.next-transition-enter-active),
.stage-tabs :deep(.n-tab-pane.prev-transition-enter-active) {
  transition:
    transform .26s cubic-bezier(.32, .72, .35, 1),
    opacity .26s cubic-bezier(.32, .72, .35, 1);
}
.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
}
.panel-head {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.panel-head b { font-size: 13.5px; color: var(--oc2-text-1); }
.panel-head .muted { font-size: 11.5px; }
/*
 * 容量标签：圆角胶囊内部就是进度条。
 * n-progress 铺满整个胶囊作为背景轨道（轨道色透出为标签底色），
 * 文字绝对定位压在上面，因此不需要额外一行的进度条。
 */
.byte-chip {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 92px;
  height: 20px;
  border-radius: 999px;
  overflow: hidden;
  flex: 0 0 auto;
}
.byte-chip-bar {
  position: absolute;
  inset: 0;
  /* n-progress 自带 100% 宽度和间距，这里只让它填满胶囊 */
  margin: 0;
}
.byte-chip-text {
  position: relative;
  font-size: 11px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--oc2-text-1);
  white-space: nowrap;
}
.byte-chip.over .byte-chip-text { color: #f87171; }
.row-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
/* 预览区：模拟游戏内名字输入框的浅色底，黑色文字才看得见；
   逐字符 span 按行内文字流排布，拖动选择与普通文本一致。 */
.preview {
  display: block;
  min-height: 74px;
  padding: 12px 14px;
  border: 1px dashed var(--oc2-row-line);
  border-radius: 10px;
  background: #f2f4f7;
  white-space: pre-wrap;
  line-height: 1.5;
  word-break: break-word;
  user-select: text;
  cursor: text;
}
.preview-char { white-space: pre; }
.controls {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.control-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.control-label { font-size: 12px; color: var(--oc2-text-3); }
/* NColorPicker 渲染的是 Fragment，根元素拿不到 scoped 的 data-v 属性，
   所以宽度只能用 :deep 从父级选择器命中。 */
.control-block :deep(.n-color-picker) { width: 220px; }
.size-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.size-slider { flex: 1; min-width: 120px; }
.size-input { width: 90px; flex: 0 0 auto; }
.code :deep(textarea) { font-family: Consolas, monospace; font-size: 12px; }
.over { color: #f87171; }
.push-hint { align-self: center; }
</style>
