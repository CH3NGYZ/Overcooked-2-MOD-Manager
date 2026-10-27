/**
 * 动效规格：统一时长与缓动，让自绘动画与 Naive 自带动画同源。
 *
 * 设计取舍（依据 ui-animation 的决策框架）：
 *   - **不做列表入场 stagger。** MOD 管理/安装是高频页面，每次进入都重播
 *     stagger 只会拖慢感知；指南明确「one-shot only，高频列表不要重播」。
 *   - **不替换 Naive 的既有动画。** 折叠用 `n-collapse-transition`、
 *     弹窗用 modal 自带的 fade-in-scale-up、进度条用 `n-progress`。
 *     自写 keyframes 会与它们抢时间轴。
 *   - **动的是「异步结果落地」**：刷新后标签从「检查中」变成确定状态、
 *     安装完成 —— 这些变化用户需要注意到，而静态换色很容易被忽略。
 *     见 ModsLocalView 的 status-pulse 与 isStatusPulsing。
 *
 * 这里只放数值常量，不放逻辑。
 */

/**
 * 标准缓动：与 Naive 的 --n-bezier 一致（cubic-bezier(.4, 0, .2, 1)）。
 * 自绘过渡与组件动画衔接时不会看出接缝。
 */
export const EASE_STANDARD = 'cubic-bezier(0.4, 0, 0.2, 1)'

/** 状态脉冲的 CSS 动画时长（毫秒）。ModsLocalView 的 pulse 计时器与 CSS keyframes 都用它。 */
export const PULSE_DURATION = 900

/** 悬停/按压这类高频反馈：入场几乎立即，出场略慢，避免闪烁。 */
export const HOVER_ENTER_MS = 90
export const HOVER_EXIT_MS = 140
