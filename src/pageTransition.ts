import { ref } from 'vue'

/**
 * 页面切换的「动画 + 滚动」统一调度。
 *
 * 之前切页和跨页定位各管各的：切页先把 scrollTop 归零再用位移滑入，
 * 而定位目标又立刻 scrollIntoView —— 两个平滑动作同时作用在同一片内容上，
 * 视觉上就是「打架」（先弹回顶部、又滑到别处）。
 *
 * 现在统一规则（由 App.vue 的导航 watch 决定动画类型）：
 *   普通切页      → 上下滑动，并回到顶部
 *   带 focus 目标 → 只淡入（page-fade），滚动交给页面在过渡结束后执行
 * 同一时刻只有一个平滑动作在跑。
 */
export type PageAnimation = 'page-down' | 'page-up' | 'page-fade'

/** 当前切页使用的过渡名，绑定到 <Transition :name>。 */
export const pageAnimation = ref<PageAnimation>('page-down')

let pendingReady = false
let waiters: Array<() => void> = []

/** 本次导航是否需要等过渡结束再滚动（由 App.vue 在路由变化时调用）。 */
export function expectPageScroll(wait: boolean) {
  pendingReady = wait
  if (!wait) releaseAll()
}

/** 过渡结束（Transition 的 after-enter）：释放等待滚动的调用方。 */
export function endNavigation() {
  pendingReady = false
  releaseAll()
}

function releaseAll() {
  const list = waiters
  waiters = []
  list.forEach((resolve) => resolve())
}

/**
 * 等到本次页面过渡结束再执行滚动。
 * 不需要等待时立即返回；万一过渡钩子没触发（同页 query 变化等），
 * 用 600ms 超时兜底，避免调用方永久挂起。
 */
export function waitForPageReady(timeoutMs = 600): Promise<void> {
  if (!pendingReady) return Promise.resolve()
  return new Promise((resolve) => {
    waiters.push(resolve)
    window.setTimeout(() => {
      const index = waiters.indexOf(resolve)
      if (index >= 0) {
        waiters.splice(index, 1)
        resolve()
      }
    }, timeoutMs)
  })
}
