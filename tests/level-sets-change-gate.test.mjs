import assert from 'node:assert/strict'
import test from 'node:test'
import { createChangeGate } from '../src/levelSetsChangeGate.ts'

/**
 * 「磁盘被人从外部改了」闸门的守门测试。
 *
 * ## 这条约定是什么
 *
 * 后端 `level_watch` 把系统文件事件按 600ms 防抖压成一声
 * `level-sets-changed`；前端 `useLevelSetsExternalChange` 用这里的
 * `createChangeGate` 决定这一声要不要真的重读。
 *
 * ## 锁死的行为
 *
 *   1. 事件到达**不立刻读**，等 `delay` 合并窗口；连发多声只读一次。
 *   2. 合并窗口内再来事件不额外排队（仍然只读一次）。
 *   3. **读了之后马上又来一声，必须再读一次** —— 这曾经被判成「自己写盘的
 *      回声」丢掉，代价是连续删两个目录 / 开页后立刻去资源管理器删东西时，
 *      第二声永远补不回来，列表一直停在旧数据。
 *   4. 页面正忙（开关/删除/导入半路上）丢弃：那时读盘是半成品，忙碌结束时
 *      页面自己会重读最终状态。
 *   5. 定时器到点时页面已经忙起来 → 放弃这次读取，不补读。
 *   6. `dispose()` 后清掉挂起的定时器、不再读。
 *
 * 定时器是注入的，所以下面全是确定性的时间线断言，不睡觉。
 */

/** 可推进的假定时器。 */
function fakeHarness() {
  let clock = 0
  let seq = 0
  const timers = new Map()
  return {
    setTimer: (callback, ms) => {
      const id = ++seq
      timers.set(id, { callback, at: clock + ms })
      return id
    },
    clearTimer: (id) => {
      timers.delete(id)
    },
    /** 推进时间并运行到期的定时器（到期时间相同按创建顺序）。 */
    advance(ms) {
      const target = clock + ms
      for (;;) {
        let nextId = null
        let nextAt = Infinity
        for (const [id, timer] of timers) {
          if (timer.at <= target && timer.at < nextAt) {
            nextAt = timer.at
            nextId = id
          }
        }
        if (nextId === null) break
        const timer = timers.get(nextId)
        timers.delete(nextId)
        clock = timer.at
        timer.callback()
      }
      clock = target
    },
    get pending() {
      return timers.size
    },
  }
}

function makeGate(overrides = {}) {
  const harness = fakeHarness()
  const state = { busy: false, reloads: 0 }
  const gate = createChangeGate({
    delay: 400,
    isBusy: () => state.busy,
    reload: () => {
      state.reloads += 1
    },
    setTimer: harness.setTimer,
    clearTimer: harness.clearTimer,
    ...overrides,
  })
  return { gate, harness, state }
}

test('外部事件在合并窗口结束后重读一次', () => {
  const { gate, harness, state } = makeGate()
  gate.notify()
  assert.equal(state.reloads, 0, '事件到达时不该立刻读')
  harness.advance(399)
  assert.equal(state.reloads, 0, '窗口内还没到')
  harness.advance(1)
  assert.equal(state.reloads, 1, '窗口结束后读一次')
})

test('一次操作连发多声只重读一次', () => {
  const { gate, harness, state } = makeGate()
  gate.notify()
  harness.advance(100)
  gate.notify()
  harness.advance(100)
  gate.notify()
  harness.advance(400)
  assert.equal(state.reloads, 1, '三声合并成一次')
  assert.equal(harness.pending, 0)
})

test('读完立刻又来一声：必须再读一次（不能当回声吞掉）', () => {
  const { gate, harness, state } = makeGate()
  gate.notify()
  harness.advance(400)
  assert.equal(state.reloads, 1)

  // 连续删两个目录：第二声紧跟在第一次重读之后，这一声是用户的真实改动。
  gate.notify()
  harness.advance(400)
  assert.equal(state.reloads, 2, '第二次外部改动照常读')
})

test('重读进行中到达的事件也要读（异步重读不吞后续事件）', async () => {
  const harness = fakeHarness()
  let reloads = 0
  const resolvers = []
  const gate = createChangeGate({
    delay: 400,
    isBusy: () => false,
    reload: () => {
      reloads += 1
      return new Promise((resolve) => resolvers.push(resolve))
    },
    setTimer: harness.setTimer,
    clearTimer: harness.clearTimer,
  })

  gate.notify()
  harness.advance(400)
  assert.equal(reloads, 1)

  // 第一次重读还没落地，磁盘又变了：这条事件不能被丢掉。
  gate.notify()
  harness.advance(400)
  assert.equal(reloads, 2)

  for (const resolve of resolvers) resolve()
  await Promise.resolve()
})

test('忙碌时丢弃事件（不排队、不在结束时补读）', () => {
  const { gate, harness, state } = makeGate()
  state.busy = true
  gate.notify()
  harness.advance(1000)
  assert.equal(state.reloads, 0)
  assert.equal(harness.pending, 0, '忙时不排定时器')

  state.busy = false
  gate.notify()
  harness.advance(400)
  assert.equal(state.reloads, 1, '闲下来后的事件照常读')
})

test('定时器到点时页面已经忙起来：这次读取放弃，不补读', () => {
  const { gate, harness, state } = makeGate()
  gate.notify()
  harness.advance(200)
  state.busy = true
  harness.advance(200)
  assert.equal(state.reloads, 0, '半成品状态不读')
})

test('dispose 后清掉挂起的定时器，不再重读', () => {
  const { gate, harness, state } = makeGate()
  gate.notify()
  gate.dispose()
  assert.equal(harness.pending, 0)
  harness.advance(5000)
  assert.equal(state.reloads, 0)
})
