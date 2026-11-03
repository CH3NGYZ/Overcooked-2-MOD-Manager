import assert from 'node:assert/strict'
import test from 'node:test'
import {
  TUTORIAL_PROGRESS_VERSION,
  TUTORIAL_STORAGE_KEY,
  clearTutorialProgress,
  isTutorialCompleted,
  markTutorialCompleted,
  readTutorialProgress,
} from '../src/tutorial/storage.ts'

/**
 * 教程进度的守门测试。
 *
 * ## 锁死的行为
 *
 *   1. 键名与版本号是**契约**：换键名等于让所有人重看，换版本号是**主动**
 *      让所有人重看。两者都不该被顺手指改，所以这里钉住字面量。
 *   2. 没存过 / 存坏了 / 版本对不上，一律当「没看过」，绝不因为解析失败
 *      就不弹教程（宁可多讲一遍，也别在新装机器上什么都不发生）。
 *   3. 存储不可用（隐私模式、配额满）时读返回默认值、写直接放弃，
 *      不允许把异常抛到启动链路上 —— 教程坏了不能连累应用启动。
 *   4. 「重新播放」清掉键，下一次启动才会重新自动开始。
 */

function fakeStorage(initial = {}) {
  const map = new Map(Object.entries(initial))
  return {
    map,
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => { map.set(key, String(value)) },
    removeItem: (key) => { map.delete(key) },
  }
}

test('键名与版本号是契约', () => {
  assert.equal(TUTORIAL_STORAGE_KEY, 'oc2-tutorial')
  assert.equal(TUTORIAL_PROGRESS_VERSION, 2)
})

test('没存过就是没看过', () => {
  const storage = fakeStorage()
  assert.equal(isTutorialCompleted(storage), false)
  assert.deepEqual(readTutorialProgress(storage), { version: TUTORIAL_PROGRESS_VERSION })
})

test('标记完成后能读回，并带 ISO 时间', () => {
  const storage = fakeStorage()
  markTutorialCompleted(storage)
  assert.equal(isTutorialCompleted(storage), true)
  const progress = readTutorialProgress(storage)
  assert.equal(progress.version, TUTORIAL_PROGRESS_VERSION)
  assert.ok(progress.completedAt)
  assert.equal(Number.isNaN(Date.parse(progress.completedAt)), false, 'completedAt 必须是合法时间')
})

test('旧版本进度视为没看过（格式换了要重讲）', () => {
  const storage = fakeStorage({ [TUTORIAL_STORAGE_KEY]: JSON.stringify({ version: 1, completedAt: '2026-01-01T00:00:00Z' }) })
  assert.equal(isTutorialCompleted(storage), false)
})

test('内容坏掉不影响启动', () => {
  for (const raw of ['', 'not json', 'null', '[]', '{"version":2,"completedAt":123}']) {
    const storage = fakeStorage({ [TUTORIAL_STORAGE_KEY]: raw })
    assert.equal(isTutorialCompleted(storage), false, `raw=${raw}`)
    assert.equal(readTutorialProgress(storage).version, TUTORIAL_PROGRESS_VERSION)
  }
})

test('清掉进度后重新变成没看过', () => {
  const storage = fakeStorage()
  markTutorialCompleted(storage)
  assert.equal(isTutorialCompleted(storage), true)
  clearTutorialProgress(storage)
  assert.equal(isTutorialCompleted(storage), false)
  assert.equal(storage.map.size, 0)
})

test('存储不可用时静默降级', () => {
  const throwing = {
    getItem() { throw new Error('SecurityError') },
    setItem() { throw new Error('QuotaExceededError') },
    removeItem() { throw new Error('SecurityError') },
  }
  assert.equal(isTutorialCompleted(throwing), false)
  assert.doesNotThrow(() => markTutorialCompleted(throwing))
  assert.doesNotThrow(() => clearTutorialProgress(throwing))
  assert.equal(isTutorialCompleted(null), false)
  assert.doesNotThrow(() => markTutorialCompleted(null))
})
