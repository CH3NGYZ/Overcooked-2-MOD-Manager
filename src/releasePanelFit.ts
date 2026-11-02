/**
 * 「下载安装」tab 的高度分配与滚轮接力 —— 纯计算部分。
 *
 * 这一块要同时满足三件事：
 * 1. 提交日志区不许被挤没（有最低高度）；
 * 2. 「资产 / 安装」一行贴在面板底边、留在抽屉可视区里；
 * 3. 日志比可用高度还长时在日志框内部滚，而不是把按钮顶出视口。
 *
 * 早先的写法是「面板只涨不缩 + 日志 flex-basis:auto 起步」：日志一长，面板
 * 跟着长高，安装行被推到抽屉折叠线以下，用户得先滚抽屉才能看见按钮 ——
 * 正是这次要修掉的行为。
 *
 * 现在的分工：JS 只做一件 CSS 拿不到的事 —— 量出抽屉被 Naive 裁出来的可视
 * 高度，把它写成面板的显式高度（`100vh - 常数` 那类估算会随头部、分段 tab、
 * 行高一起漂移，旧实现实测在 720 高的窗口里多算 47px）。定高之后，面板内部
 * 的 flex 链自己把剩余空间分给日志框：日志长了就在框内滚，短了就长高补齐，
 * 安装行靠 `margin-top: auto` 永远贴底。**不再**去量「除日志框以外的固定
 * 高度」——那个值必须在面板高度失效后量才有意义，而「让内联高度失效」与
 * Vue 的渲染时机互相赛跑，实测会量出 21px 这种脏值。
 *
 * 这里只放不碰 DOM 的纯函数（DOM 量测留在组件里），node:test 可以直接跑边界值。
 */

/** 日志框最低高度（px）：再矮就等于把「提交日志」这块挤没了。 */
export const RELEASE_LOG_MIN_HEIGHT = 140

/** 一个滚动盒子的三个关键尺寸（`scrollTop` / `clientHeight` / `scrollHeight`）。 */
export interface ScrollBoxMetrics {
  scrollTop: number
  clientHeight: number
  scrollHeight: number
}

/**
 * 面板该锁成多高：抽屉可视高度（已扣掉内容层的内边距）。
 *
 * 小数向上取整会让面板比可视区高一点点、把抽屉 body 顶出一条 1px 的滚动条，
 * 所以这里统一向下取整。
 */
export function releasePanelHeight(viewportHeight: number): number {
  return Math.max(0, Math.floor(viewportHeight))
}

/**
 * 锁高之后日志框还够不够最低高度。
 *
 * 不够就说明这个窗口放不下「日志下限 + 安装行」，硬锁会把安装行裁掉（`.detail`
 * 是 `overflow: clip`），不如回退成不锁高度、让抽屉 body 自己滚。
 */
export function fitsMinLogHeight(logHeight: number, minHeight: number = RELEASE_LOG_MIN_HEIGHT): boolean {
  return logHeight + 1 >= minHeight
}

/** 还能不能朝 `deltaY` 的方向继续滚（上下各留 1px 容差，避免小数取整抖动）。 */
export function canScrollInDirection(metrics: ScrollBoxMetrics, deltaY: number): boolean {
  if (Math.abs(deltaY) < 1) return false
  if (deltaY > 0) return metrics.scrollTop + metrics.clientHeight < metrics.scrollHeight - 1
  return metrics.scrollTop > 1
}

export interface WheelHandoffInput {
  deltaY: number
  /** 鼠标下最内层那个滚动盒子（日志框）；不在日志框里时为 null。 */
  inner: ScrollBoxMetrics | null
  /** 抽屉自己的滚动容器。 */
  outer: ScrollBoxMetrics
  /** 抽屉背后页面的滚动容器。 */
  page: ScrollBoxMetrics
}

/**
 * 滚轮到边界后要不要把这一下交给**抽屉背后的页面**。
 *
 * 抽屉被 Teleport 到 `body`，背后的 `.page-scroll` 不是它的滚动祖先，浏览器自己
 * 的滚动链走不到那里；用户把抽屉滚到顶还继续滚，本意就是「上面的页面往上走」。
 * 内层（日志框）或抽屉自己还能滚时一律不接管，交回浏览器原生滚动。
 */
export function wheelNeedsHandoff(input: WheelHandoffInput): boolean {
  if (Math.abs(input.deltaY) < 1) return false
  if (input.inner && canScrollInDirection(input.inner, input.deltaY)) return false
  if (canScrollInDirection(input.outer, input.deltaY)) return false
  return canScrollInDirection(input.page, input.deltaY)
}
