<script setup lang="ts">
/**
 * MOD 改名（多彩名字）：把逐字符颜色/字号编译成 MOD 可识别的富文本标签串，
 * 可复制或直接推送给正在运行的多彩名字 MOD。
 * 生成算法在 src/colorfulName.ts，推送走 Rust 命令（前端 CSP 不允许直连本地端口）。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import {
  NButton, NColorPicker, NInput, NInputNumber, NProgress, NSlider, NSpin, NTabPane, NTabs,
} from 'naive-ui'
import { ColorWandOutline, CopyOutline, CloudUploadOutline, RefreshOutline } from '@vicons/ionicons5'
import { useNotify } from '../useNotify'
import {
  DEFAULT_COLOR, DEFAULT_SIZE, MAX_NAME_BYTES, buildShortestRichText, isNameOverLimit, utf16LeBytes,
} from '../colorfulName'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { setPageTitleOverride } from '../pageHeader'

const message = useNotify()

/**
 * MOD 内置的常用调色板（与游戏内命名习惯一致）。
 *
 * 前两个是游戏里最常用的橙、蓝（rgb(255,163,57) / rgb(91,163,220)），
 * 放在最前面方便直接点。原来的 #ffa43b / #5fa6de 是这两个色的近似值，
 * 已用精确值取代，避免同一色系在调色板里出现两份。
 */
const PALETTE = [
  '#ffa339', '#5ba3dc', '#000000', '#ffffff', '#ff0000',
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

/**
 * 「未选择文字」提示的限频。
 *
 * 用户未选中文字就调样式时，连续操作（拖动滑块、拖动取色器）
 * 会每秒触发几十次，每次都弹一条通知会把右下角刷屏。
 *
 * 规则（按用户确认）：
 *   - 点颜色块：一次点击 = 一次提示（click 本身就是离散动作，直接调用 warnNoSelection）
 *   - 滑块拖动：整个按下→拖动→抬起只提示一次（beginGesture 开始，endGesture 结束）
 *   - 自定义取色器拖动：同上
 *
 * 实现上用一个「手势」计数：手势内只放行第一次。离散动作（点击色块）
 * 不走手势，因此每次点击都能提示。
 */
let gestureDepth = 0
let warnedInGesture = false

/** 开始一个连续手势（滑块/取色器按下）。 */
function beginGesture() {
  gestureDepth += 1
  warnedInGesture = false
}

/** 结束一个连续手势（滑块/取色器抬起）。 */
function endGesture() {
  gestureDepth = Math.max(0, gestureDepth - 1)
  if (gestureDepth === 0) warnedInGesture = false
}

/** 未选中文字时的提示；连续手势内只提示一次。 */
function warnNoSelection() {
  if (gestureDepth > 0) {
    if (warnedInGesture) return
    warnedInGesture = true
  }
  message.warning(t('请先选择要修改的文字'))
}

/*
 * 自定义取色器的手势。
 *
 * n-color-picker 的面板默认 Teleport 到 body，拖动发生在面板里，
 * 宿主元素上的 mousedown / mouseup 收不到 —— 这正是之前「拖动调色板
 * 时左下角通知刷屏」的原因：手势从没开始过，每次 update:value 都当作
 * 离散操作各弹一条。
 *
 * 改用 Naive 自己的两个事件：
 *   - update:value：拖动中持续触发 → 第一次触发时懒开启手势；
 *   - complete：内部在 document 上监听 mouseup，抬手必触发（拖出面板也算）。
 * 再叠加 update:show=false 兜底（拖到一半用键盘或点外部关掉面板）。
 *
 * 懒开启是必要的：若把手势挂在 show 上、又在 complete 时结束，
 * 面板保持打开期间的第二次拖动就没有手势可用了（show 不会重复触发），
 * 又退回到刷屏。
 */
let paletteGesture = false

function beginPaletteGesture() {
  if (paletteGesture) return
  paletteGesture = true
  beginGesture()
}

function endPaletteGesture() {
  if (!paletteGesture) return
  paletteGesture = false
  endGesture()
}

/**
 * 应用颜色。
 *
 * @param continuous 是否来自取色器的连续拖动。色块点击是离散动作，
 *                   每次点击都该有反馈，所以不传。
 */
function applyColor(color: string, continuous = false) {
  if (continuous) beginPaletteGesture()
  const range = selected.value
  if (!range) { warnNoSelection(); return }
  picker.value = color
  const next = [...colors.value]
  for (let index = range.start; index < range.end; index += 1) next[index] = color.toLowerCase()
  colors.value = next
}

/**
 * 选中范围内的基准字号（本轮拖动开始前的值）。
 * 拖动时 sizes 会被本轮改过，必须回到起点才能正确叠加增量。
 */
let sizeBaseline: number[] | null = null

/** 拖动滑块时实时预览：立刻按「基准 + 增量」重算，不等到松手。 */
function previewSize(step: number) {
  sizeStep.value = step
  const range = selected.value
  if (!range) {
    // 拖动过程中持续触发，这里靠手势限频只提示一次
    warnNoSelection()
    return
  }
  if (!sizeBaseline) sizeBaseline = [...sizes.value]
  const next = [...sizes.value]
  for (let index = range.start; index < range.end; index += 1) {
    next[index] = Math.max(1, Math.min(71, (sizeBaseline[index] ?? DEFAULT_SIZE) + step))
  }
  sizes.value = next
}

/** 松手或输入框提交：结束本轮调整，增量归零并清掉基准。 */
function endSizeAdjust(value: number) {
  if (!selected.value) {
    warnNoSelection()
    sizeStep.value = 0
    sizeBaseline = null
    endGesture()
    return
  }
  previewSize(value)
  sizeStep.value = 0
  sizeBaseline = null
  endGesture()
}

/** 输入框直接给绝对值：按「目标值 - 基准」换算增量后应用。 */
function applySizeInput(target: number) {
  const range = selected.value
  if (!range) { warnNoSelection(); return }
  const next = [...sizes.value]
  for (let index = range.start; index < range.end; index += 1) {
    next[index] = Math.max(1, Math.min(71, (sizes.value[index] ?? DEFAULT_SIZE) + target))
  }
  sizes.value = next
  sizeStep.value = 0
  sizeBaseline = null
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
    message.success(t('已复制，可粘贴到街机MOD 的多彩名字菜单'))
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
              <!--
                预设色块全部平铺出来（点击即应用），后面跟一个「其他颜色」入口
                打开 Naive 取色器。平铺用 n-button 的色块形态，不需要自绘。
              -->
              <div class="palette-row">
                <!-- 色块点击是离散动作：每次点击都能提示未选文字 -->
                <button v-for="color in PALETTE" :key="color" type="button" class="palette-swatch"
                  :class="{ active: picker.toLowerCase() === color.toLowerCase() }"
                  :style="{ background: color }" :title="color" @click="applyColor(color)" />
                <!--
                  自定义取色器是连续手势：拖动中持续 update:value，抬手触发
                  complete（Naive 在 document 上监听 mouseup，拖出面板也算）。
                  未选文字时整个拖动只提示一次，不会刷屏。
                -->
                <n-color-picker v-model:value="picker" :show-alpha="false" size="small" class="picker-custom"
                  :modes="['hex']"
                  @update:value="(value: string) => applyColor(value, true)"
                  @complete="endPaletteGesture"
                  @update:show="(show: boolean) => { if (!show) endPaletteGesture() }" />
              </div>
            </div>

            <div class="control-block">
              <span class="control-label">{{ t('字号增量') }}</span>
              <div class="size-row">
                <!--
                  拖动即预览；按下→拖动→抬起算一个手势，
                  未选文字时整个手势只提示一次。
                -->
                <n-slider :value="sizeStep" :min="-34" :max="36" :step="1" class="size-slider"
                  @mousedown="beginGesture"
                  @update:value="previewSize"
                  @dragend="endSizeAdjust(sizeStep)" />
                <n-input-number :value="sizeStep" size="small" :min="-34" :max="36" class="size-input"
                  @update:value="(value: number | null) => applySizeInput(value ?? 0)" />
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
              超限时在它右边直接显示红色警告，不再单独占一行提示。
            -->
            <span class="byte-chip" :class="{ over: overLimit }">
              <n-progress class="byte-chip-bar" type="line" :percentage="percent" :show-indicator="false"
                :height="20" :border-radius="999" :status="overLimit ? 'error' : 'success'" />
              <span class="byte-chip-text">{{ byteLength }} / {{ MAX_NAME_BYTES }} B</span>
            </span>
            <span v-if="overLimit" class="limit-warning">{{ t('超出 238 字节限制') }}</span>
          </div>
          <n-input :value="code" type="textarea" readonly :autosize="{ minRows: 2, maxRows: 6 }" class="code" />
        </section>
      </n-tab-pane>
    </n-tabs>
  </div>

  <!--
    推送中的遮罩：覆盖「侧栏右侧、标题栏下方」的内容区并居中。
    放在 n-tabs 之外，避免随面板切换被卸载。
  -->
  <Teleport to="body">
    <div v-if="pushing" class="push-mask">
      <n-spin size="large" :description="t('正在推送到 街机MOD')" />
    </div>
  </Teleport>
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
/*
 * 面板内间距：两个 tab-pane 各自包含多张卡片，
 * 这里补回纵向 gap（原先由已删除的 .colorful 容器提供）。
 */
.stage-tabs :deep(.n-tab-pane) {
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
/*
 * 面板容器：Naive 在 animated 切换时会给 .n-tabs-pane-wrapper 写一个
 * max-height 并靠过渡动画过渡它。两个阶段的卡片数量不同（阶段一 1 张、
 * 阶段二 2 张），如果 max-height 在动画结束时停在旧面板的高度上，
 * 底部卡片就会被裁掉一条，直到下一帧才恢复 —— 也就是「动画末期缺一块」。
 *
 * 这里让 wrapper 高度由内容决定（max-height: none），
 * 高度变化本身不再做高度动画；横向滑动已经提供了足够的过渡感，
 * 而高度动画在「两个面板高度差很大」时本来就会显得突兀。
 */
.stage-tabs :deep(.n-tabs-pane-wrapper) {
  max-height: none !important;
  transition: none;
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
/*
 * 色板：预设色全部平铺 + 末尾一个「其他颜色」取色器。
 * 色块本身是普通 button（只是一个色块，Naive 没有专门的纯色块组件），
 * 取色器仍是 n-color-picker，负责任意颜色。
 */
.palette-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
}
.palette-swatch {
  width: 24px;
  height: 24px;
  padding: 0;
  border: 2px solid var(--oc2-row-line);
  border-radius: 6px;
  cursor: pointer;
  transition: transform .14s ease, border-color .14s ease;
}
.palette-swatch:hover { transform: scale(1.12); border-color: var(--oc2-text-2); }
.palette-swatch.active {
  border-color: var(--oc2-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 40%, transparent);
}
/* NColorPicker 渲染的是 Fragment，根元素拿不到 scoped 的 data-v 属性，
   所以宽度只能用 :deep 从父级选择器命中。 */
.palette-row :deep(.n-color-picker) { width: 150px; }
.size-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.size-slider { flex: 1; min-width: 120px; }
.size-input { width: 90px; flex: 0 0 auto; }
.code :deep(textarea) { font-family: Consolas, monospace; font-size: 12px; }
/* 超限警告：紧跟容量胶囊右侧，不单独占行 */
.limit-warning {
  font-size: 11.5px;
  font-weight: 600;
  color: #f87171;
  white-space: nowrap;
}
/*
 * 推送遮罩：覆盖内容区（让出侧栏与标题栏）并在其中居中。
 * 用与弹窗一致的做法 —— 让出 --oc2-titlebar-height 与 --oc2-rail-width，
 * 这样转圈落在用户真正在看的区域中心，而不是整个窗口中心。
 */
.push-mask {
  position: fixed;
  top: var(--oc2-titlebar-height, 56px);
  left: var(--oc2-rail-width, 108px);
  right: 0;
  bottom: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--oc2-body) 72%, transparent);
  backdrop-filter: blur(2px);
  z-index: var(--oc2-layer-page, 2500);
}
</style>
