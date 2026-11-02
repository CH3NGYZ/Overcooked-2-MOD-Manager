/**
 * 「磁盘上的关卡集被人从外部改过」事件的前端闸门。
 *
 * ## 这一声该不该读
 *
 * 后端 `level_watch` 已经把系统文件事件按 600ms 防抖压成一次
 * `level-sets-changed`，前端还要回答一个问题：这一声是不是「自己刚写过」
 * 的回声？删一张卡、切一次开关、导入一个地图集，落的都是同一批目录，
 * 产生的也是同一种事件。
 *
 * 答案只有两种做法：
 *
 * - 按时间猜（「页面自己刚重读过，`quiet` 窗口内的事件一定是回声」）。
 *   曾经就是这么写的，代价是一次**永远补不回来**的漏读：连续删两个目录、
 *   或者开页后立刻去资源管理器删东西，第二声落在窗口里被丢弃，列表就一直
 *   停着旧数据 —— 而「手动删了目录列表要自动更新」正是这个功能存在的理由。
 * - 不猜，收到就重读。这里选这条：`info` 有磁盘内容缓存，重扫实测
 *   **37–44 ms**（52 个关卡集，第二位小数都不抖），比一次误判的代价小得多。
 *   一次操作连发多声由 `delay` 合并，净成本是一次几十毫秒的后台扫盘。
 *
 * 唯一保留的丢弃是**页面正忙**（`isBusy()`）：开关/删除/导入半路上读盘会
 * 读到半成品（目录搬到一半、zip 解到一半）。忙碌结束时页面自己一定重读，
 * 那次读到的就是最终状态，所以丢掉的这一声不需要补。
 *
 * 定时器与时钟都可注入，纯逻辑模块（不依赖 Vue/Tauri），Node 测试直接跑。
 */

export interface ChangeGateOptions {
  /** 合并窗口：一次操作会连发多声事件，第一声之后等这么久只重读一次。 */
  delay: number
  /** 页面是否正忙（忙时丢弃事件，见文件头）。 */
  isBusy: () => boolean
  /** 真正重读。 */
  reload: () => void | Promise<void>
  /** 可注入的定时器（测试用）。 */
  setTimer?: (callback: () => void, ms: number) => unknown
  clearTimer?: (handle: unknown) => void
}

export interface ChangeGate {
  /** 后端推来一声「磁盘变了」。 */
  notify: () => void
  /** 卸载时清掉挂起的定时器。 */
  dispose: () => void
}

export function createChangeGate(options: ChangeGateOptions): ChangeGate {
  const setTimer = options.setTimer ?? ((callback, ms) => setTimeout(callback, ms))
  const clearTimer = options.clearTimer ?? ((handle) => clearTimeout(handle as ReturnType<typeof setTimeout>))

  let pending: unknown = null
  let disposed = false

  function fire() {
    pending = null
    if (disposed) return
    // 定时器等的这段时间里用户可能已经开始下一次操作，读半成品没有意义。
    if (options.isBusy()) return
    void options.reload()
  }

  return {
    notify() {
      if (disposed) return
      if (options.isBusy()) return
      // 已经排上队就让它去读：一次操作连发的事件合并成一次重读。
      if (pending !== null) return
      pending = setTimer(fire, options.delay)
    },
    dispose() {
      disposed = true
      if (pending !== null) clearTimer(pending)
      pending = null
    },
  }
}
