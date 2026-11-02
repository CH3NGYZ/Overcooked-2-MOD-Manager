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
 * ## 为什么需要它（而不是收到就读）
 *
 * 删一张卡、切一次开关、导入一个地图集，写的都是同一批目录，产生的也是
 * 同一种事件。照单全收就会在页面自己刚重读完后再冷扫一遍（几十到上百 MB
 * 的 `info`），用户点一下卡两秒。所以这里锁死三件事：
 *
 *   1. 页面自己重读之后（`touch()`）的静默窗口内，事件是回声，丢弃；
 *   2. 页面正忙（开关/删除/导入半路上）时丢弃 —— 那时读盘会读到半成品，
 *      忙碌结束时页面本来就会重读；
 *   3. 其余情况必须放行，且连续多声只合并成一次重读。
 *
 * 时钟与定时器都是注入的，所以下面全部是确定性的时间线断言，不睡觉。
 */

/** 可推进的假时钟 + 假定时器。 */
function fakeHarness() {
  let clock = 0
  let seq = 0
  const timers = new Map()
  return {
    now: () => clock,
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
    quiet: 2500,
    isBusy: () => state.busy,
    reload: () => {
      state.reloads += 1
    },
    now: harness.now,
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

test('页面自己刚重读过时，回声被吞掉', () => {
  const { gate, harness, state } = makeGate()
  gate.touch()
  gate.notify()
  harness.advance(400)
  assert.equal(state.reloads, 0, '静默窗口内不读')

  // 静默窗口过后，真正的外部改动必须放行 —— 这才是功能本体。
  harness.advance(2100)
  gate.notify()
  harness.advance(400)
  assert.equal(state.reloads, 1)
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

test('异步重读：落地时间算作 touch 时刻', async () => {
  const harness = fakeHarness()
  let reloads = 0
  let resolveReload
  const gate = createChangeGate({
    delay: 400,
    quiet: 2500,
    isBusy: () => false,
    reload: () => {
      reloads += 1
      return new Promise((resolve) => {
        resolveReload = resolve
      })
    },
    now: harness.now,
    setTimer: harness.setTimer,
    clearTimer: harness.clearTimer,
  })

  gate.notify()
  harness.advance(400)
  assert.equal(reloads, 1)

  // 重读还没落地：这段时间里的事件不该被当成「刚读过」。
  harness.advance(5000)
  resolveReload()
  await Promise.resolve()
  await Promise.resolve()

  // 落地时刻已经过去 0ms，回声应被吞。
  gate.notify()
  harness.advance(400)
  assert.equal(reloads, 1, '落地后的回声被静默窗口挡住')
})
