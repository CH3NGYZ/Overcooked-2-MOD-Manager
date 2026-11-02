/**
 * 「磁盘上的关卡集被人从外部改过」事件的前端闸门。
 *
 * ## 为什么需要闸门，而不是收到事件就重读
 *
 * 后端 `level_watch` 已经把系统文件事件按 600ms 防抖压成一次
 * `level-sets-changed`，但这一声仍然可能来自**我们自己**：删除一张卡、
 * 切一次开关、导入一个地图集，落的都是同一批目录。若照单全收，每次操作
 * 都会在页面自己重读之后再白扫一遍（冷扫几十到上百 MB 的 `info`），
 * 用户看到的是「点一下卡一下两秒」。
 *
 * 闸门要挡的两种事件：
 *
 * 1. **页面自己刚重读过**（`touch()`）—— 之后 `quiet` 毫秒内到达的事件
 *    一定是这次操作自己的回声，页面数据已经是最新的，再读纯属浪费。
 * 2. **页面正忙**（`isBusy()`）—— 开关/删除/导入正在半路上，这时读盘会
 *    读到半成品（目录搬到一半、zip 解到一半）。丢掉的这次事件不用补，
 *    因为忙碌本身结束时页面会重读一次。
 *
 * 反过来，用户真在资源管理器里删了东西时，距上次 `touch()` 早过了
 * `quiet` 窗口，事件必然放行 —— 这条路径才是这个功能存在的理由。
 *
 * 定时器与时钟都可注入，纯逻辑模块（不依赖 Vue/Tauri），Node 测试直接跑。
 */

export interface ChangeGateOptions {
  /** 合并窗口：一次操作会连发多声事件，第一声之后等这么久只重读一次。 */
  delay: number
  /** 页面自己重读之后的静默窗口，用来吞掉自己的回声。 */
  quiet: number
  /** 页面是否正忙（忙时丢弃事件，见文件头第 2 条）。 */
  isBusy: () => boolean
  /** 真正重读；返回 promise 时闸门会等它落地再重新计时。 */
  reload: () => void | Promise<void>
  /** 可注入的时钟与定时器（测试用）。 */
  now?: () => number
  setTimer?: (callback: () => void, ms: number) => unknown
  clearTimer?: (handle: unknown) => void
}

export interface ChangeGate {
  /** 后端推来一声「磁盘变了」。 */
  notify: () => void
  /** 页面自己刚重读过数据（页面所有主动重读都要调）。 */
  touch: () => void
  /** 卸载时清掉挂起的定时器。 */
  dispose: () => void
}

export function createChangeGate(options: ChangeGateOptions): ChangeGate {
  const now = options.now ?? (() => Date.now())
  const setTimer = options.setTimer ?? ((callback, ms) => setTimeout(callback, ms))
  const clearTimer = options.clearTimer ?? ((handle) => clearTimeout(handle as ReturnType<typeof setTimeout>))

  /** 页面自己上一次重读完成的时间；`-Infinity` 表示本次挂载还没读过。 */
  let lastTouch = Number.NEGATIVE_INFINITY
  let pending: unknown = null
  let disposed = false

  function fire() {
    pending = null
    if (disposed) return
    // 定时器等的这段时间里用户可能已经开始下一次操作，读半成品没有意义。
    if (options.isBusy()) return
    const result = options.reload()
    if (result && typeof (result as Promise<void>).then === 'function') {
      void Promise.resolve(result).then(() => { lastTouch = now() })
    } else {
      lastTouch = now()
    }
  }

  return {
    notify() {
      if (disposed) return
      if (options.isBusy()) return
      if (now() - lastTouch < options.quiet) return
      // 已经排上队就让它去读：一次操作连发的事件合并成一次重读。
      if (pending !== null) return
      pending = setTimer(fire, options.delay)
    },
    touch() {
      lastTouch = now()
    },
    dispose() {
      disposed = true
      if (pending !== null) clearTimer(pending)
      pending = null
    },
  }
}
