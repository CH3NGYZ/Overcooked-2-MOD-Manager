<script setup lang="ts">
/**
 * 统一的应用弹窗外壳 —— 前端所有弹窗都走这里。
 *
 * 解决两个反复出现的布局问题：
 *   1. Naive 的 n-modal + preset="card" 高度由内容决定，内容一长就超出可视
 *      区域，底部按钮被顶到视口外；窗口本身不可滚动时直接够不到。
 *   2. 反过来，内容很短时又会被写成固定高度的弹窗拉出一大片空白。
 *
 * 规则：尺寸是「上限」而不是「定值」——
 *   宽：内容自然宽度，最多 max-width（默认标题栏下方区域的 95%）
 *   高：内容自然高度，最多 max-height（默认标题栏下方区域的 95%）
 * 所以短内容自动收缩，长内容到上限后由中间区域滚动，
 * 头部与 #footer 始终固定，任何情况下都不会超出可视区域。
 *
 * 关键实现细节（都是踩过的坑）：
 *   · 高度约束必须用 :content-style 内联下发。Naive 的 .n-card-content 自带
 *     `flex: 1` 却**没有** `min-height: 0`，而 flex item 的 min-height 默认是
 *     auto —— 只靠外部 class 改不动，内容会把 card 撑破，并同时出现
 *     「弹窗不变高、内容被裁、明明溢出却滚不动」三个症状。
 *   · Naive 在 preset="card" 下把传入的 class 合并到「同时带 n-modal 与
 *     n-card 的同一个元素」上，内容区类名是 .n-card-content（不是
 *     .n-card__content）。
 */
import { computed, nextTick, onBeforeUnmount, toRefs, useId, watch } from 'vue'
import { NModal } from 'naive-ui'
import {
  MODAL_RESIZE_DURATION,
  MODAL_RESIZE_EASING,
  modalResizeKeyframes,
  shouldAnimateModalResize,
  type ModalSize,
} from '../modalResize'

const props = withDefaults(defineProps<{
  show: boolean
  /** 卡片标题；不传则不渲染头部。 */
  title?: string
  /** 最大宽度（上限，不是定值）。 */
  maxWidth?: string
  /** 最大高度（上限，不是定值）。 */
  maxHeight?: string
  /**
   * 是否直接撑满上限（宽高都用 100%），而不是随内容自适应。
   *
   * 默认 `false`：短内容自动收缩，长内容到上限后内部滚动。
   * 编辑器这类「内容本身就需要尽可能大」的弹窗设为 `true`，
   * 打开即占满允许的最大区域，不随内容多少跳动。
   */
  fill?: boolean
  /**
   * 内容区是否自己滚动。
   *
   * 默认 `true`：内容包在 `.app-modal-scroll` 里（`overflow-y: auto`），
   * 长内容在内部滚动，头部与 footer 固定。
   *
   * 设 `false` 时不包裹滚动层，插槽内容直接成为内容区 flex 子项 ——
   * 供需要「占满整块区域、内部自己管理滚动」的内容使用
   * （例如配置编辑器的多行文本域：它要拿到确定高度才能撑满）。
   */
  scrollable?: boolean
  /** 内容区最小高度：内容很少时也不至于缩成一条，默认不设。 */
  minHeight?: string
  bordered?: boolean
  maskClosable?: boolean
  /**
   * 是否允许用户自行关闭弹窗（点遮罩、按 Esc、点右上角关闭按钮）。
   *
   * 默认 `true`：三条隐式关闭路径全部可用，与加这个 prop 之前的行为一致。
   * 只有「确认危险操作」这类必须明确二选一的弹窗传 `false` ——
   * 误点遮罩或按 Esc 不能等于放弃，用户只能在底部按钮里选。
   * 底部按钮走 `update:show` 显式事件，不受这个 prop 影响。
   */
  closable?: boolean
  /**
   * `if` = 关闭时真正从 DOM 移除内容。
   * 只 v-show 会让内部的大型内容（如文本域滚动位置、富文本）留在不可见
   * 容器里继续参与布局计算。
   */
  displayDirective?: 'if' | 'show'
  /** 额外类名，用于页面定制配色等。 */
  cardClass?: string
  /**
   * 挂载目标。默认 `body`（全窗口居中、层级最高）。
   * 传选择器可把遮罩限制在某个容器内 —— 配置编辑器用它挂到
   * `.page-scroll`，让遮罩只覆盖侧栏右侧的内容区。
   */
  to?: string
}>(), {
  title: '',
  /*
   * 用百分比而不是 vw/vh：界面缩放是 WebView zoom，vw/vh 不随之缩放，
   * 高缩放下会算出比可见区域更大的尺寸。百分比相对已缩放的容器计算。
   */
  maxWidth: '95%',
  maxHeight: '95%',
  fill: false,
  scrollable: true,
  minHeight: '',
  bordered: false,
  maskClosable: true,
  closable: true,
  displayDirective: 'if',
  cardClass: '',
  to: undefined,
})

const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /**
   * 离场动画（Naive 的 fade-in-scale-up：缩小 + 渐隐）播完之后触发。
   * 宿主若要在关闭时清空内容（如丢弃预览、释放编辑器），应等到这里，
   * 否则内容会先消失、只剩一个空框在缩小，看不到渐隐。
   */
  (event: 'after-leave'): void
}>()

/*
 * 模板只能访问 setup 作用域的顶层绑定，所以把 props 解成独立常量。
 * 变量名不能与 prop 同名 —— 同名时 vue-tsc 会把模板里的标识符解析成
 * 组件实例属性（报 Property 'show' does not exist on ComponentPublicInstance）。
 */
const {
  show: visible,
  title: heading,
  maxWidth: widthLimit,
  maxHeight: heightLimit,
  fill: fillLimit,
  scrollable: scrollableContent,
  minHeight: heightFloor,
  bordered: hasBorder,
  maskClosable: maskClosableEnabled,
  closable: allowClose,
  displayDirective: directive,
  cardClass: extraClass,
  to: mountTarget,
} = toRefs(props)

/*
 * 尺寸全部以 Naive 的容器为基准计算，不用 vw/vh，也不用百分比。
 *
 * 两条历史坑：
 *   1. `vw`/`vh` 不随 WebView zoom 缩放 —— 200% 缩放下 80vw 会超出可见区域。
 *   2. **百分比高度在这里根本无效**：.n-modal-scroll-content 只有
 *      `min-height: 100%`，没有确定高度，`max-height: 90%` 无法解析，
 *      浏览器按 `none` 处理，约束直接失效（内容多长弹窗就多高，按钮被挤出屏幕）。
 *
 * 所以改用 calc 直接对「扣除标题栏后的视口」求值：
 *   --oc2-titlebar-height 已在 global.css 定义，
 *   100vh - 标题栏 = 弹窗真正可用的高度。
 * 各调用方的 maxHeight 也必须传 calc 形式，不能再传百分比。
 */
const TITLEBAR = 'var(--oc2-titlebar-height, 56px)'

/** 宽/高上限：以“扣除标题栏的视口”为基准，留固定边距。 */
function toCssLimit(value: string, axis: 'width' | 'height'): string {
  const trimmed = value.trim()
  // 百分比转成 calc：宽度相对视口宽度，高度相对“视口高度 - 标题栏”
  const percent = /^(\d+(?:\.\d+)?)%$/.exec(trimmed)
  if (percent) {
    const ratio = Number(percent[1]) / 100
    return axis === 'width'
      ? `calc(100vw * ${ratio})`
      : `calc((100vh - ${TITLEBAR}) * ${ratio})`
  }
  return trimmed
}

const modalStyle = computed(() => {
  const w = toCssLimit(widthLimit.value, 'width')
  const h = toCssLimit(heightLimit.value, 'height')
  return {
    // fill：直接占满上限，打开即最大区域，不随内容多少跳动
    width: fillLimit.value ? w : 'auto',
    minWidth: fillLimit.value ? undefined : 'min(320px, 92vw)',
    maxWidth: w,
    height: fillLimit.value ? h : 'auto',
    maxHeight: h,
  }
})

/**
 * 内容区约束：min-height: 0 是让 flex 子项可收缩的关键，
 * 缺了它内容只会在自然高度上继续长，滚动条永远不会出现。
 */
const contentStyle = {
  flex: '1 1 auto',
  minHeight: '0',
  display: 'flex',
  flexDirection: 'column' as const,
  overflow: 'hidden',
}

/* ===== 尺寸变化过渡 ===== */
/*
 * 弹窗尺寸随内容变（切 tab、展开详情、错误提示出现），Naive 是直接跳过去的。
 * 这里观察卡片自己的尺寸，把「上一次 → 这一次」补成 200ms 的宽高过渡。
 *
 * 为什么用带实例号的类名去查卡片元素：`preset="card"` 下 Naive 把 `class`
 * 合到「同时带 `.n-modal` 与 `.n-card` 的同一个元素」上，而 `NModal` 组件实例
 * 的 `$el` 是一段 Teleport 注释节点，拿不到卡片本身。带实例号的类名让每个
 * 弹窗都能在自己的模板里定位到自己那张卡片（同时开两个弹窗也不会串）。
 */
const instanceMarker = `app-modal-instance-${useId()}`

let sizeObserver: ResizeObserver | null = null
let sizeAnim: Animation | null = null
let lastSize: ModalSize | null = null
let lastViewport = { width: 0, height: 0 }
/** 上一段过渡还没播完：期间 Observer 每帧都会回报中间尺寸，不能再起一段。 */
let resizing = false

function cardElement(): HTMLElement | null {
  return document.querySelector<HTMLElement>(`.${instanceMarker}`)
}

function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

function readSize(element: HTMLElement): ModalSize {
  return { width: element.offsetWidth, height: element.offsetHeight }
}

function handleCardResize() {
  const element = cardElement()
  if (!element) return
  const size = readSize(element)
  const viewport = { width: window.innerWidth, height: window.innerHeight }
  const viewportChanged =
    viewport.width !== lastViewport.width || viewport.height !== lastViewport.height
  lastViewport = viewport
  const from = lastSize
  lastSize = size
  if (!from) return
  const animate = shouldAnimateModalResize({
    from,
    to: size,
    animating: resizing,
    viewportChanged,
    reducedMotion: prefersReducedMotion(),
  })
  if (!animate) return
  sizeAnim?.cancel()
  resizing = true
  sizeAnim = element.animate(modalResizeKeyframes(from, size), {
    duration: MODAL_RESIZE_DURATION,
    easing: MODAL_RESIZE_EASING,
  })
  sizeAnim.finished
    // cancel() 会让 finished reject，这不是错误
    .catch(() => undefined)
    .finally(() => {
      resizing = false
      sizeAnim = null
    })
}

function observeCardSize() {
  const element = cardElement()
  if (!element) return
  stopObservingCardSize()
  lastSize = readSize(element)
  lastViewport = { width: window.innerWidth, height: window.innerHeight }
  sizeObserver = new ResizeObserver(handleCardResize)
  sizeObserver.observe(element)
}

function stopObservingCardSize() {
  sizeObserver?.disconnect()
  sizeObserver = null
  sizeAnim?.cancel()
  sizeAnim = null
  resizing = false
  lastSize = null
}

/*
 * 只在弹窗真正显示时观察（`displayDirective: 'if'` 下关闭会把内容从 DOM 移除，
 * 观察一个已经消失的元素没有意义）。等一帧再查：Naive 是在 show 之后的渲染里
 * 才把卡片挂进 Teleport 目标的。
 */
watch(visible, async (shown) => {
  if (!shown) {
    stopObservingCardSize()
    return
  }
  await nextTick()
  observeCardSize()
}, { immediate: true })

onBeforeUnmount(stopObservingCardSize)
</script>

<template>
  <n-modal :show="visible" preset="card" :title="heading || undefined" :bordered="hasBorder"
    :closable="allowClose" :mask-closable="allowClose && maskClosableEnabled" :close-on-esc="allowClose"
    :display-directive="directive" :to="mountTarget" class="app-modal"
    :class="[extraClass, instanceMarker]" :style="modalStyle" :content-style="contentStyle"
    @update:show="(value: boolean) => emit('update:show', value)"
    @after-leave="emit('after-leave')">
    <!-- 头部右侧附加内容（筛选器、计数等），透传 Naive 原生槽位 -->
    <template v-if="$slots['header-extra']" #header-extra>
      <slot name="header-extra" />
    </template>
    <!--
      scrollable（默认）：内容包在滚动层里，长内容内部滚动。
      scrollable=false：插槽内容直接成为内容区的 flex 子项，
      由它自己占满并管理滚动（配置编辑器的文本域需要确定高度才能撑满）。
    -->
    <div v-if="scrollableContent" class="app-modal-scroll"
      :style="heightFloor ? { minHeight: heightFloor } : undefined">
      <slot />
    </div>
    <slot v-else />
    <!-- 固定操作区：不传就完全不渲染，内容区自动占满剩余高度 -->
    <div v-if="$slots.footer" class="app-modal-footer">
      <slot name="footer" />
    </div>
  </n-modal>
</template>

<style scoped>
.app-modal-scroll {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  /* 给滚动条一点呼吸，避免贴着卡片内边距 */
  padding-right: 2px;
}
/*
 * 底部操作区固定在滚动区之外，只有它自己有上边框与内边距，
 * 所以内容滚到底时不会和按钮粘在一起。
 */
.app-modal-footer {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
  padding-top: 12px;
  border-top: 1px solid var(--oc2-row-line);
  margin-top: 12px;
}
</style>
