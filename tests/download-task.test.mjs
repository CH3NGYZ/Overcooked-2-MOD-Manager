import assert from 'node:assert/strict'
import test from 'node:test'
import {
  beginDownloadTask,
  dismissDownloadFailure,
  downloadTaskState,
  endDownloadTask,
  failDownloadTask,
  retryDownloadTask,
} from '../src/downloadTask.ts'

/**
 * 下载失败「原地重试」的状态机（`src/downloadTask.ts`）。
 *
 * ## 这条规则是什么
 *
 * 后端的失败路径直接 `Err` 返回给调用方，**不发事件**（`archive.rs` 的
 * `emit_progress_event` 只在下载回调里被调用）。所以卡片要靠调用方把失败
 * 报告过来，再由卡片给「重试」按钮 —— 重试要能原样重跑这次下载。
 *
 * ## 为什么这里只测状态机
 *
 * 卡片长什么样、按钮在哪，属于 `DownloadStrip.vue`（实机验收看）。这一层
 * 出错的后果全是「界面上说不清」：重试点了没反应、旧任务的成功把新任务的
 * 失败清掉、用户取消之后弹出一张「可重试」的红卡片。这些都没法靠肉眼
 * 稳定复现，所以钉在这里。
 *
 * ## 三个「不能」
 *
 *   1. 失败之后流程 `finally` 里的 `end` 不能把失败态清掉；
 *   2. 旧任务迟到的 `end` / `fail` 不能改写新任务的状态；
 *   3. 用户主动取消（`download.cancelled`）不是失败，不能弹重试卡片。
 */

/** 让出事件循环，等重试闭包里的异步流程推进。 */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

/** 不关心重试动作的用例用它占位。 */
function noop() {}

/** 每个用例都从「没有任务、没有失败」开始，避免用例之间互相踩状态。 */
function reset() {
  dismissDownloadFailure()
  const running = downloadTaskState.running
  if (running) endDownloadTask(running.id)
}

test('失败进入错误态：标题、原因（已翻译）、重试闭包都留着', () => {
  reset()
  let calls = 0
  const id = beginDownloadTask({ label: 'RW 关卡合集', retry: () => { calls += 1 } })
  failDownloadTask(id, 'download.firstByteTimeout')

  assert.equal(downloadTaskState.running, null, '失败后不再有正在进行的任务')
  const failure = downloadTaskState.failure
  assert.ok(failure, '失败必须点亮卡片')
  assert.equal(failure.id, id)
  assert.equal(failure.label, 'RW 关卡合集')
  // 后端 tag 由 backendMessageOf 翻译，不能把 tag 直接摆到界面上
  assert.equal(failure.message, '下载首字节等待超过 10 秒')
  assert.equal(calls, 0, '报告失败本身不触发重试')

  // 各入口的 finally 会再调一次 end：任务号已经失效，不能清掉刚报的失败
  endDownloadTask(id)
  assert.ok(downloadTaskState.failure, 'end 不能清掉已经报出来的失败')

  reset()
})

test('成功结束之后不残留失败态', () => {
  reset()
  const failed = beginDownloadTask({ label: 'A', retry: noop })
  failDownloadTask(failed, 'download.noCandidate')
  assert.ok(downloadTaskState.failure)

  // 用户回到页面重新发起一次下载（不是点卡片上的重试）
  const next = beginDownloadTask({ label: 'B', retry: noop })
  assert.equal(downloadTaskState.failure, null, '新任务开始时旧的失败已经过期')

  endDownloadTask(next)
  assert.equal(downloadTaskState.running, null)
  assert.equal(downloadTaskState.failure, null, '成功结束不能残留失败态')
})

test('旧任务迟到的结束与失败都不影响新任务', () => {
  reset()
  const first = beginDownloadTask({ label: '旧任务', retry: noop })
  const second = beginDownloadTask({ label: '新任务', retry: noop })

  // 旧任务的失败迟到了：新任务已经在跑，不能把它顶掉换成错误态
  failDownloadTask(first, 'download.firstByteTimeout')
  assert.equal(downloadTaskState.failure, null)
  assert.equal(downloadTaskState.running?.id, second, '新任务仍在进行')

  // 旧任务的成功也迟到了：更不能结束新任务
  endDownloadTask(first)
  assert.equal(downloadTaskState.running?.id, second)
  assert.equal(downloadTaskState.failure, null)

  reset()
})

test('旧任务的成功不能清掉新任务的失败', () => {
  reset()
  const first = beginDownloadTask({ label: '旧任务', retry: noop })
  const second = beginDownloadTask({ label: '新任务', retry: noop })
  failDownloadTask(second, 'download.empty')
  assert.ok(downloadTaskState.failure)

  endDownloadTask(first)
  assert.equal(
    downloadTaskState.failure?.id,
    second,
    '新任务报的失败必须留着，不能被旧任务的收尾清掉',
  )

  reset()
})

test('原地重试：点击当下先清失败态，再跑重试闭包', () => {
  reset()
  let calls = 0
  const id = beginDownloadTask({ label: 'X', retry: () => { calls += 1 } })
  failDownloadTask(id, 'download.firstByteTimeout')

  retryDownloadTask()
  assert.equal(downloadTaskState.failure, null, '点重试的同一帧就要离开错误态')
  assert.equal(calls, 1, '重试闭包必须被调用')
  assert.equal(downloadTaskState.running, null, '闭包没开始新任务时不该有进行中状态')

  // 没有失败态时再点一次是空操作，不能重复跑闭包
  retryDownloadTask()
  assert.equal(calls, 1)
})

test('重试再次失败会重新点亮卡片，并且还能再点一次', async () => {
  reset()
  let attempts = 0
  let failNext = true

  const retry = () => {
    attempts += 1
    if (failNext) run()
  }
  /** 模拟一个入口流程：领任务号 → 下载失败 → 报失败。 */
  const run = () => {
    const id = beginDownloadTask({ label: 'RW 关卡合集', retry })
    failDownloadTask(id, 'download.firstByteTimeout')
  }

  run()
  assert.ok(downloadTaskState.failure)

  retryDownloadTask()
  await tick()
  assert.equal(attempts, 1)
  assert.ok(downloadTaskState.failure, '重试也失败时卡片必须重新出现')

  failNext = false
  retryDownloadTask()
  await tick()
  assert.equal(attempts, 2)
  assert.equal(downloadTaskState.failure, null)
})

test('用户取消不是失败，不弹可重试的卡片', () => {
  reset()
  const cancelled = beginDownloadTask({ label: 'X', retry: noop })
  failDownloadTask(cancelled, 'download.cancelled')
  assert.equal(downloadTaskState.failure, null, '取消后不该出现红色卡片')
  assert.equal(downloadTaskState.running, null)

  // Error 形态（IPC 包装）同样要认出来
  const wrapped = beginDownloadTask({ label: 'Y', retry: noop })
  failDownloadTask(wrapped, new Error('download.cancelled'))
  assert.equal(downloadTaskState.failure, null)
})

test('关闭失败卡片：只清状态，不跑重试', () => {
  reset()
  let calls = 0
  const id = beginDownloadTask({ label: 'X', retry: () => { calls += 1 } })
  failDownloadTask(id, 'download.incomplete')
  assert.ok(downloadTaskState.failure)

  dismissDownloadFailure()
  assert.equal(downloadTaskState.failure, null)
  assert.equal(calls, 0)
})
