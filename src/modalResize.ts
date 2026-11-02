/**
 * 弹窗尺寸变化的过渡判定 —— 纯计算部分。
 *
 * 弹窗尺寸是「内容自然尺寸，受上限约束」：内容一换（切 tab、展开详情、错误提示
 * 出现、列表从 3 条变 12 条），Naive 直接把卡片从旧尺寸跳到新尺寸，视觉上像被
 * 猛拍一下。这里把「要不要给这次变化补一段 200ms 的宽高过渡」的判定抽成纯函数，
 * 组件只负责观察尺寸与播动画。
 *
 * 不该补过渡的情况（都有实测或明确的道理）：
 *   · 正在播上一段尺寸过渡 —— 从中途接着算会两段动画抢同一个属性，抖动；
 *   · 视口尺寸变了 —— 那是用户拖窗口/切界面缩放，跟着补动画反而拖着走；
 *   · 系统开了「减少动态效果」—— 动画对这类用户是干扰；
 *   · 从 0 尺寸长出来 —— 那是弹窗刚挂载，属于进场动画的范畴，不该再来一段。
 */

/** 过渡时长（ms）：比 Naive 进场动画（约 200ms）同档，开关弹窗时观感一致。 */
export const MODAL_RESIZE_DURATION = 200

/** 与 Naive 的 `fade-in-scale-up` 同一条缓动，尺寸变化和进场用同一种节奏。 */
export const MODAL_RESIZE_EASING = 'cubic-bezier(.4, 0, .2, 1)'

/** 小于这个像素差视为同一次布局的取整抖动，不补过渡。 */
export const MODAL_RESIZE_MIN_DELTA = 2

export interface ModalSize {
  width: number
  height: number
}

export interface ModalResizeDecision {
  from: ModalSize
  to: ModalSize
  /** 上一段尺寸过渡还在播。 */
  animating: boolean
  /** 视口尺寸变了（窗口缩放 / 界面缩放）。 */
  viewportChanged?: boolean
  /** 系统「减少动态效果」。 */
  reducedMotion?: boolean
}

export function shouldAnimateModalResize(input: ModalResizeDecision): boolean {
  if (input.animating) return false
  if (input.viewportChanged) return false
  if (input.reducedMotion) return false
  // 刚挂载：0 宽/0 高是还没布局，交给进场动画，别当成尺寸突变
  if (input.from.width <= 0 || input.from.height <= 0) return false
  if (input.to.width <= 0 || input.to.height <= 0) return false
  const delta = Math.max(
    Math.abs(input.to.width - input.from.width),
    Math.abs(input.to.height - input.from.height),
  )
  return delta >= MODAL_RESIZE_MIN_DELTA
}

/**
 * 两个关键帧：从旧尺寸到新尺寸。
 *
 * 结束帧就是内容当前的天然尺寸，动画结束（`fill: none`）后元素回到 `auto`，
 * 与末帧数值一致，不会在最后一帧跳一下。
 */
export function modalResizeKeyframes(from: ModalSize, to: ModalSize): Keyframe[] {
  return [
    { width: `${from.width}px`, height: `${from.height}px` },
    { width: `${to.width}px`, height: `${to.height}px` },
  ]
}
