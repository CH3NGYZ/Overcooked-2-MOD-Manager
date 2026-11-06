import assert from 'node:assert/strict'
import test from 'node:test'

/** 连点解锁：真实源码直接执行（Node 24 类型擦除）。 */
const {
  SECRET_ENTRY_TAPS,
  SECRET_ENTRY_WINDOW_MS,
  recordTap,
  tapSequenceReady,
} = await import('../src/secretEntry.ts')

const tapMany = (times, windowMs = SECRET_ENTRY_WINDOW_MS) => {
  let history = []
  for (const at of times) history = recordTap(history, at, windowMs)
  return history
}

test('快速连点 5 次才解锁，少于 5 次不解锁', () => {
  const times = [0, 120, 240, 360, 480]
  let history = []
  for (let i = 0; i < times.length; i += 1) {
    history = recordTap(history, times[i])
    assert.equal(tapSequenceReady(history), i === times.length - 1, `第 ${i + 1} 次不该提前解锁`)
  }
  assert.equal(SECRET_ENTRY_TAPS, 5)
  assert.equal(history.length, 5)
})

test('慢慢点凑不满：每次间隔超过窗口时旧点击被丢掉', () => {
  const step = SECRET_ENTRY_WINDOW_MS + 500
  const history = tapMany([0, step, step * 2, step * 3, step * 4])
  assert.equal(history.length, 1)
  assert.equal(tapSequenceReady(history), false)
})

test('窗口边界：恰好等于窗口时过期，差 1ms 仍算连点', () => {
  const atBoundary = recordTap([0], SECRET_ENTRY_WINDOW_MS)
  assert.deepEqual(atBoundary, [SECRET_ENTRY_WINDOW_MS])

  const justInside = recordTap([0], SECRET_ENTRY_WINDOW_MS - 1)
  assert.deepEqual(justInside, [0, SECRET_ENTRY_WINDOW_MS - 1])
})

test('未来时间戳（时间回拨）被丢弃，之后仍能正常解锁', () => {
  let history = recordTap([10_000], 0)
  assert.deepEqual(history, [0])
  for (const at of [100, 200, 300, 400]) history = recordTap(history, at)
  assert.equal(tapSequenceReady(history), true)
})

test('达成后调用方清空序列，第 6 次点击不会再次立即解锁', () => {
  let history = tapMany([0, 100, 200, 300, 400])
  assert.equal(tapSequenceReady(history), true)
  history = []
  history = recordTap(history, 600)
  assert.equal(tapSequenceReady(history), false)
})

test('纯函数：不修改传入的序列', () => {
  const original = Object.freeze([0, 100, 200])
  const next = recordTap(original, 300)
  assert.deepEqual([...original], [0, 100, 200])
  assert.deepEqual(next, [0, 100, 200, 300])
})

test('次数与窗口可由调用方覆盖，默认常量只用于标题栏', () => {
  const history = tapMany([0, 50], 500)
  assert.equal(tapSequenceReady(history, 2), true)
  assert.equal(tapSequenceReady(history, 3), false)
  const expired = recordTap([0], 600, 500)
  assert.deepEqual(expired, [600])
})
