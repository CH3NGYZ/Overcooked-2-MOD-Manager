import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 教程进度（已看步骤 ID）的纯逻辑测试。
 *
 * ## 为什么单独测这三个函数
 *
 * 「只播新增步骤」的全部判断都压在 `progress.ts` 的三个纯函数上，而引擎那侧
 * 一旦算错，用户看到的是**静默**行为：教程要么从头再播一遍，要么整段不播。
 * 两种都不会报错，只有用户会发现。所以把「哪些算已看、哪些还要播、两次记录
 * 怎么合并」搬到 Node 里逐条钉住，引擎只负责调用。
 *
 * 这些函数直接 import 真实源码：`progress.ts` 不碰 Vue、不碰 i18n、不碰
 * localStorage，Node 能原样跑。
 */
const { mergeWatchedSteps, normalizeStepIds, unwatchedSteps } = await import('../src/tutorial/progress.ts')

test('normalizeStepIds 去掉空白、空串与重复，保留首次出现的顺序', () => {
  assert.deepEqual(normalizeStepIds([' b ', 'a', 'b', '', '  ', 'c', 'a']), ['b', 'a', 'c'])
})

test('normalizeStepIds 不修改传入数组', () => {
  const input = ['a', 'a']
  normalizeStepIds(input)
  assert.deepEqual(input, ['a', 'a'])
})

test('unwatchedSteps 只留下没看过的步骤', () => {
  const steps = [{ id: 'intro' }, { id: 'sidebar' }, { id: 'mods' }]
  assert.deepEqual(unwatchedSteps(steps, ['intro', 'mods']), [{ id: 'sidebar' }])
})

test('unwatchedSteps 保持原顺序并原样返回对象引用', () => {
  const first = { id: 'a' }
  const second = { id: 'b' }
  const result = unwatchedSteps([first, second], [])
  assert.deepEqual(result, [first, second])
  assert.equal(result[0], first)
})

test('旧的已看列表里出现未知 id 不影响判断', () => {
  const steps = [{ id: 'intro' }]
  assert.deepEqual(unwatchedSteps(steps, ['removed-step', 'intro']), [])
})

test('mergeWatchedSteps 取并集、去重、保留已有顺序并追加新 id', () => {
  assert.deepEqual(mergeWatchedSteps(['a', 'b'], ['b', 'c', 'a', 'd']), ['a', 'b', 'c', 'd'])
})

test('mergeWatchedSteps 对脏数据同样归一化', () => {
  assert.deepEqual(mergeWatchedSteps([' a '], ['', 'a', ' b ']), ['a', 'b'])
})

test('全部步骤都看过时没有待播步骤（启动不再打扰）', () => {
  const steps = [{ id: 'intro' }, { id: 'sidebar' }]
  const watched = mergeWatchedSteps([], steps.map((step) => step.id))
  assert.deepEqual(unwatchedSteps(steps, watched), [])
})
