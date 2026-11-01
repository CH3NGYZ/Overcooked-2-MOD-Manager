import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 限并发执行器（`src/concurrency.ts`）。
 *
 * 刷新链路里所有「最多同时跑 4 组」的约定都压在它身上：
 * 仓库 Release、仓库源文档、地图 Release。它写错了不会报错，
 * 只会让并发悄悄变成 1（慢）或者不限（把匿名 API 额度打光），
 * 所以这里把「并发上限」和「结果归位」都钉死。
 */
const { mapWithConcurrency } = await import('../src/concurrency.ts')

/** 让出一轮事件循环，用于让 worker 有机会启动 / 完成。 */
function tick(ms = 1) {
  return new Promise((resolve) => { setTimeout(resolve, ms) })
}

test('结果按输入下标归位，与完成顺序无关', async () => {
  // 故意让第一个任务最后完成：结果必须是 [a0, b1, c2]，不是完成顺序
  const result = await mapWithConcurrency(['a', 'b', 'c'], 3, async (item, index) => {
    await tick(index === 0 ? 20 : 1)
    return `${item}${index}`
  })
  assert.deepEqual(result, ['a0', 'b1', 'c2'])
})

test('同一时刻在飞的任务不超过上限', async () => {
  let inFlight = 0
  let peak = 0
  await mapWithConcurrency(Array.from({ length: 10 }, (_, index) => index), 4, async () => {
    inFlight += 1
    peak = Math.max(peak, inFlight)
    await tick()
    inFlight -= 1
  })
  // 4 个 worker 在同一个同步段里启动，所以峰值一定是 4 而不是「最多 4」
  assert.equal(peak, 4)
})

test('上限大于任务数时按实际任务数开 worker', async () => {
  let inFlight = 0
  let peak = 0
  await mapWithConcurrency([1, 2], 4, async () => {
    inFlight += 1
    peak = Math.max(peak, inFlight)
    await tick()
    inFlight -= 1
  })
  assert.equal(peak, 2)
})

test('空列表直接返回空数组，不调用 worker', async () => {
  let calls = 0
  const result = await mapWithConcurrency([], 4, async () => { calls += 1 })
  assert.deepEqual(result, [])
  assert.equal(calls, 0)
})

test('非法上限（0 / 负数 / 小数 / 非数字 / 无穷大）退化成串行，绝不变成不限并发', async () => {
  for (const limit of [0, -3, 1.9, Number.NaN, Number.POSITIVE_INFINITY]) {
    let inFlight = 0
    let peak = 0
    const result = await mapWithConcurrency([1, 2, 3], limit, async (item) => {
      inFlight += 1
      peak = Math.max(peak, inFlight)
      await tick()
      inFlight -= 1
      return item
    })
    assert.deepEqual(result, [1, 2, 3], `limit=${limit}`)
    assert.equal(peak, 1, `limit=${limit} 应当退化为串行`)
  }
})

test('worker 抛错时向上抛，不吞成 undefined 结果', async () => {
  await assert.rejects(
    () => mapWithConcurrency([1, 2, 3], 2, async (item) => {
      if (item === 2) throw new Error('boom')
      return item
    }),
    /boom/,
  )
})
