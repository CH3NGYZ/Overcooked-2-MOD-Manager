/**
 * 全局下载任务的「当前状态」与「失败后原地重试」。
 *
 * ## 为什么需要这个模块
 *
 * 后端的 `download-progress` 只报进度：失败路径直接 `Err` 返回给调用方，
 * **不发任何事件**（`src-tauri/src/archive.rs` 的 `emit_progress_event` 只在
 * 下载回调里被调用）。所以首字节等待超过 `FIRST_BYTE_TIMEOUT` 这类失败发生时，
 * 下载卡片收不到通知，会一直停在「正在连接」—— 用户看到的是一张永不结束的
 * 进度条，既没有错误说明也没有重试入口。
 *
 * 失败只有调用方知道（那个 `Err` 是返回给它的），所以由调用方把结论报告到
 * 这里，卡片再从这里读。也正因如此，本模块必须记下**重试闭包**：只有发起
 * 下载的那段代码知道这次下载的仓库 id / tag / 资产名 / 本地路径，卡片自己
 * 拼不出「再来一次」的调用。
 *
 * ## 为什么用递增任务号
 *
 * `download-progress` 事件里**没有任务 id**，迟到的旧事件与并发的两个下载
 * 无法从事件本身区分。状态只能由「最近开始的那个任务」改写：旧任务的成功
 * 不能清掉新任务的失败，旧任务的失败也不能顶掉新任务。所以
 * `beginDownloadTask` 领一个递增号，`end` / `fail` 带上自己的号，号对不上
 * 就整条丢弃。
 */
import { reactive } from 'vue'
// 带扩展名：本模块由 `tests/download-task.test.mjs` 直接 import 真实源码
// （与 `modRelations.ts` / `refreshState.ts` 那一批纯逻辑模块同一约定）。
import { backendMessageOf } from './locale/backend.ts'

export interface DownloadTaskOptions {
  /** 失败卡片上的标题：条目名、地图名、产品名这类可读文案（可以是后端 tag）。 */
  label: string
  /** 原地重试：只重跑「这次下载」这一段，已经成功的后续步骤不重跑。 */
  retry: () => void | Promise<void>
}

interface RunningTask extends DownloadTaskOptions {
  id: number
}

export interface DownloadFailure extends DownloadTaskOptions {
  id: number
  /** 已翻译的失败原因（`backendMessageOf` 的输出）。 */
  message: string
}

export interface DownloadTaskState {
  /** 正在进行的下载；`null` 表示没有（或已经结束）。 */
  running: RunningTask | null
  /** 最近一次失败；`null` 表示没有需要用户处理的失败。 */
  failure: DownloadFailure | null
}

/**
 * 模块级单例（`DownloadStrip` 是全局唯一的下载卡片，状态也只需要一份）。
 *
 * 只读使用：除本模块导出的几个函数外，不要在别处直接赋值。
 */
export const downloadTaskState = reactive<DownloadTaskState>({
  running: null,
  failure: null,
})

/** 递增任务号：只允许最近开始的任务改写状态。 */
let sequence = 0

/** 开始一次下载，返回本次任务号（`end` / `fail` 必须带上它）。 */
export function beginDownloadTask(options: DownloadTaskOptions): number {
  sequence += 1
  const id = sequence
  downloadTaskState.running = { id, label: options.label, retry: options.retry }
  // 新任务开始时上一轮的失败已经过期：卡片要立刻从错误态回到进度态
  downloadTaskState.failure = null
  return id
}

/**
 * 下载成功结束（或用户取消后收尾）。
 *
 * 号对不上说明这是旧任务的收尾，整条丢弃 —— 否则一个慢流程结束时会清掉
 * 新任务刚报的失败，用户点重试也看不到任何结论。
 */
export function endDownloadTask(id: number): void {
  const running = downloadTaskState.running
  if (!running || running.id !== id) return
  downloadTaskState.running = null
  // 最近的任务成功结束，之前残留的失败结论作废
  downloadTaskState.failure = null
}

/**
 * 用户主动取消不是失败：取消后如果还弹一张「可重试」的红色卡片，
 * 等于把用户的意图当成故障。取消标记同样是后端全局的，
 * `cancelDownload()` 之后所有在飞下载都会收到这个 tag。
 */
function isCancelled(reason: unknown): boolean {
  const raw = reason instanceof Error
    ? reason.message
    : typeof reason === 'string' ? reason : ''
  return raw === 'download.cancelled'
}

/** 报告一次下载失败；`reason` 是后端 tag、`Error` 或 `{ message }`。 */
export function failDownloadTask(id: number, reason: unknown): void {
  const running = downloadTaskState.running
  // 号对不上 = 旧任务的失败：新任务已经开始了，不能让旧结论顶掉它
  if (!running || running.id !== id) return
  downloadTaskState.running = null
  if (isCancelled(reason)) {
    downloadTaskState.failure = null
    return
  }
  downloadTaskState.failure = {
    id,
    label: running.label,
    retry: running.retry,
    message: backendMessageOf(reason),
  }
}

/** 用户关掉失败卡片：不重试，也不再显示原因。 */
export function dismissDownloadFailure(): void {
  downloadTaskState.failure = null
}

/**
 * 原地重试：先清失败态，再跑重试闭包。
 *
 * 先清是因为重试会重新 `begin` 一次下载，卡片必须先离开错误态；否则闭包在
 * 第一个 await 之前就抛错时，界面会停在「红色卡片 + 已经点过重试」的矛盾状态。
 */
export function retryDownloadTask(): void {
  const failure = downloadTaskState.failure
  downloadTaskState.failure = null
  if (!failure) return
  try {
    /*
     * 重试闭包是各入口的完整流程，它自己会用同一个任务号把新的失败报回来；
     * 这里只兜住同步抛出与漏掉的 rejection，不让一次点击把异常甩进事件循环。
     */
    void Promise.resolve(failure.retry()).catch(() => undefined)
  } catch (reason) {
    downloadTaskState.failure = { ...failure, message: backendMessageOf(reason) }
  }
}
