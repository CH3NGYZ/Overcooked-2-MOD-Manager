import assert from 'node:assert/strict'
import test from 'node:test'
import { createRefreshState } from '../src/refreshState.ts'
import { refreshReleaseItem, runReleaseBatch } from '../src/releaseRefresh.ts'

/**
 * 「刷一个回来一个」的守门测试。
 *
 * ## 这条约定是什么
 *
 * 点一次「检查更新」会排上十几个仓库/地图，但并发只有 4。用户要看到的节奏是
 * **刷完一个就出一个结论**：这 4 个里谁先回来，谁的那张卡就先从「刷新中」
 * 变成「最新版 / 可更新 / 刷新失败」，剩下排队的继续转。
 *
 * ## 之前坏在哪
 *
 * 状态机把整批目标先写进 `queued`，轮到某个目标时想把它从队列里挪走，
 * 条件却写成 `if (refreshing.value[id])` —— 那一刻它必然还没在飞，
 * 条件永远不成立。于是排队标记要等整批 `finally` 才清：所有卡片一直显示
 * 「刷新中」，直到最后一个请求回来才一起变成结论，观感就是
 * 「结果一批一起回来」。失败条目更晚：只有整批结束的后端重扫才纠正它。
 *
 * 所以这里锁的是三件事：
 *   1. 单条完成 → 它自己的 `refreshing` / `queued` 立刻清空、结果立刻写回；
 *   2. 失败同样当场写回（把错误交给 `fail`），不等整批；
 *   3. 写回发生在关标志**之前**（否则会先闪一帧「不在刷新中、结论还是旧的」）。
 */

/** 让出几个事件循环，等已经排队的微任务/定时任务跑完。 */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function deferred() {
  let resolve
  let reject
  const promise = new Promise((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}

const release = (tag_name) => ({ tag_name, name: tag_name, body: '', prerelease: false, draft: false, assets: [] })

test('刷一个回来一个：单条完成立刻落地，不等整批', async () => {
  const state = createRefreshState(2)
  const gates = new Map(['a', 'b', 'c', 'd'].map((id) => [id, deferred()]))
  const landed = []

  const batch = runReleaseBatch({
    state,
    items: ['a', 'b', 'c', 'd'].map((id) => ({ id })),
    trigger: 'bound',
    fetch: (item) => gates.get(item.id).promise,
    apply: (item, releases) => {
      // 写回早于关标志：此刻它仍然算「刷新中」，卡片不会先闪回旧结论
      assert.equal(state.refreshing.value[item.id], true, `${item.id} 写回时还应处于刷新中`)
      landed.push([item.id, releases[0].tag_name])
    },
    errorText: (item) => `失败 ${item.id}`,
  })

  await tick()
  assert.deepEqual(Object.keys(state.refreshing.value).sort(), ['a', 'b'], '并发 2：先飞 a/b')
  assert.deepEqual(Object.keys(state.queued.value).sort(), ['c', 'd'], 'c/d 在排队')

  gates.get('a').resolve([release('1.0.0')])
  await tick()
  assert.equal(state.refreshing.value.a, undefined, 'a 完成后不再显示刷新中')
  assert.equal(state.queued.value.a, undefined, 'a 完成后也不在排队里')
  assert.deepEqual(landed, [['a', '1.0.0']], 'a 的结论立刻落地，不等 b/c/d')
  assert.deepEqual(Object.keys(state.refreshing.value).sort(), ['b', 'c'], '空出的槽位立刻接 c')
  assert.deepEqual(Object.keys(state.queued.value).sort(), ['d'], 'c 接手后只剩 d 排队')

  gates.get('b').resolve([release('2.0.0')])
  gates.get('c').resolve([release('3.0.0')])
  gates.get('d').resolve([release('4.0.0')])
  const outcome = await batch
  assert.equal(outcome.refreshed, 4)
  assert.deepEqual(outcome.errors, [])
  assert.deepEqual(landed.map(([id]) => id).sort(), ['a', 'b', 'c', 'd'])
  assert.deepEqual(state.refreshing.value, {}, '整批结束后没有残留的刷新中')
  assert.deepEqual(state.queued.value, {}, '整批结束后没有残留的排队标记')
})

test('失败也是单条落地：当场写回，不等整批重扫', async () => {
  const state = createRefreshState(1)
  const gates = new Map([['x', deferred()], ['y', deferred()]])
  const failures = []

  const batch = runReleaseBatch({
    state,
    items: [{ id: 'x' }, { id: 'y' }],
    trigger: 'bound',
    fetch: (item) => gates.get(item.id).promise,
    fail: (item, message) => {
      assert.equal(state.refreshing.value[item.id], true, '失败写回也要早于关标志')
      failures.push([item.id, message])
    },
    errorText: (item, reason) => `${item.id}: ${reason.message}`,
  })

  await tick()
  assert.deepEqual(Object.keys(state.queued.value), ['y'], '并发 1：y 还在排队')
  assert.equal(state.queued.value.x, undefined, 'x 轮到时立刻退出排队')

  gates.get('x').reject(new Error('网络失败'))
  await tick()
  assert.deepEqual(failures, [['x', 'x: 网络失败']], 'x 的失败当场写回')
  assert.equal(state.refreshing.value.x, undefined, 'x 失败后不再显示刷新中')
  assert.equal(state.refreshing.value.y, true, 'y 接手开始刷')
  assert.equal(state.queued.value.y, undefined, 'y 开始后退出排队')

  gates.get('y').resolve([release('9.9.9')])
  const outcome = await batch
  assert.deepEqual(outcome.errors, ['x: 网络失败'])
  assert.equal(outcome.refreshed, 2)
})

test('同一目标已经在刷时，重复请求直接跳过而不是并发两次', async () => {
  const state = createRefreshState(1)
  const gate = deferred()
  let calls = 0
  const spec = {
    fetch: () => { calls += 1; return gate.promise },
    errorText: () => '不应该走到这里',
  }

  const first = refreshReleaseItem(state, { id: 'r' }, spec)
  assert.equal(state.refreshing.value.r, true)
  assert.equal(await refreshReleaseItem(state, { id: 'r' }, spec), null)

  assert.equal(calls, 1, '第二次请求被跳过，没有真的再发一次')
  gate.resolve([release('1.0.0')])
  assert.equal(await first, null)
  assert.equal(state.refreshing.value.r, undefined)
})
