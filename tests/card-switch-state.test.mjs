import assert from 'node:assert/strict'
import test from 'node:test'
import { effect, nextTick } from 'vue'
import {
  cardSwitchOn,
  cardSwitchValue,
  clearCardSwitches,
  levelSetPluginSwitchKey,
  levelSetSwitchKey,
  modSwitchKey,
  publishCardSwitches,
} from '../src/cardSwitchState.ts'

/**
 * 卡片开关快照的守门测试。
 *
 * ## 这条约定是什么
 *
 * 批量启停（「全部开启 / 全部关闭」）后，页面上每个开关都要像用户手点一样
 * 走一遍 Naive `n-switch` 自带的值变化过渡。开关值来自 `cardSwitchState`
 * 发布的快照：页面在折叠插槽里按稳定键读它，卡片自己也按同一个键读。
 *
 * ## 之前坏在哪
 *
 * 折叠项内容挂在 `n-collapse-item` 的稳定插槽里，父级换数据不会让插槽重新
 * 求值，卡片拿不到新的 `enabled`；早先靠「换整棵列表的 key」强制重建，
 * 开关被重新挂载后直接就是目标状态，过渡没有播的机会。
 *
 * ## 所以这里锁的是
 *
 *   1. 发布后能按稳定键读到值，并且读操作是**响应式的**（快照变了，读它的
 *      插槽/卡片才会重渲染，这是动画的前提）；
 *   2. 批量发布一轮后每个键都是新值 —— 少一个键就少一个会动的开关；
 *   3. 没发布过的键回退调用方传入的实时值（首帧、批量结果还没回来的窗口）；
 *   4. 清理后不留旧值，避免换页/重进时拿上一次的开关状态；
 *   5. 键的稳定性：MOD 的 `dll_file` 会随开关在 `.dll` / `.dll.disabled`
 *      之间变，两种形态必须落到同一个键；地图按目录名、随包插件按
 *      「集 + 文件名」，不能串。
 */

test('发布后能按稳定键读到值', () => {
  clearCardSwitches()
  publishCardSwitches([['mod:BepInEx/plugins/A.dll', true], ['mod:BepInEx/plugins/B.dll', false]])

  assert.equal(cardSwitchValue('mod:BepInEx/plugins/A.dll'), true)
  assert.equal(cardSwitchValue('mod:BepInEx/plugins/B.dll'), false)
  assert.equal(cardSwitchOn('mod:BepInEx/plugins/A.dll', false), true)
  assert.equal(cardSwitchOn('mod:BepInEx/plugins/B.dll', true), false)
})

test('批量发布后每个键都变成新值', () => {
  clearCardSwitches()
  const keys = Array.from({ length: 25 }, (_, index) => `mod:BepInEx/plugins/Mod${index}.dll`)
  publishCardSwitches(keys.map((key) => [key, true]))
  assert.deepEqual(keys.map((key) => cardSwitchValue(key)), keys.map(() => true))

  // 一次「全部关闭」：所有键一起翻成 false，中间不留旧值
  publishCardSwitches(keys.map((key) => [key, false]))
  assert.deepEqual(keys.map((key) => cardSwitchValue(key)), keys.map(() => false))
})

test('没发布过的键回退调用方手里的实时值', () => {
  clearCardSwitches()

  assert.equal(cardSwitchValue('mod:never-published.dll'), undefined)
  assert.equal(cardSwitchOn('mod:never-published.dll', true), true)
  assert.equal(cardSwitchOn('mod:never-published.dll', false), false)
  // 没有稳定键的卡片（例如还没接快照的调用方）完全走 props
  assert.equal(cardSwitchOn(undefined, true), true)
  assert.equal(cardSwitchOn('', false), false)
})

test('清理后不残留', () => {
  clearCardSwitches()
  publishCardSwitches([['level:A', false]])
  assert.equal(cardSwitchValue('level:A'), false)

  clearCardSwitches()
  assert.equal(cardSwitchValue('level:A'), undefined)
  assert.equal(cardSwitchOn('level:A', true), true)
})

test('发布是响应式的：读快照的一方会收到新值', async () => {
  clearCardSwitches()
  publishCardSwitches([['mod:BepInEx/plugins/A.dll', true]])

  const seen = []
  effect(() => {
    seen.push(cardSwitchOn('mod:BepInEx/plugins/A.dll', false))
  })
  assert.deepEqual(seen, [true])

  publishCardSwitches([['mod:BepInEx/plugins/A.dll', false]])
  await nextTick()
  assert.deepEqual(seen, [true, false])

  // 同值重复发布不该再触发渲染（每次 load 都会整体重发一遍，不能因此整页重绘）
  publishCardSwitches([['mod:BepInEx/plugins/A.dll', false]])
  await nextTick()
  assert.deepEqual(seen, [true, false])
})

test('MOD 键去掉 .disabled 后缀：开关前后是同一个键', () => {
  const enabled = 'BepInEx\\plugins\\Example.dll'
  const disabled = 'BepInEx\\plugins\\Example.dll.disabled'

  assert.equal(modSwitchKey(enabled), modSwitchKey(disabled))
  assert.equal(modSwitchKey('X.DLL.DISABLED'), modSwitchKey('X.DLL'))
  // 只去一层后缀，不把文件名中间出现的 .disabled 当后缀
  assert.notEqual(modSwitchKey('X.dll.disabled.disabled'), modSwitchKey('X.dll'))
})

test('地图键按目录名与「集 + 插件文件名」区分', () => {
  assert.equal(levelSetSwitchKey('宜'), 'level:宜')
  assert.notEqual(levelSetSwitchKey('宜'), levelSetSwitchKey('胡闹糕中'))

  // 不同集里允许有同名 DLL，键必须带上所属集
  assert.notEqual(
    levelSetPluginSwitchKey('宜', 'Runtime.dll'),
    levelSetPluginSwitchKey('胡闹糕中', 'Runtime.dll'),
  )
  assert.equal(
    levelSetPluginSwitchKey('宜', 'Runtime.dll'),
    levelSetPluginSwitchKey('宜', 'Runtime.dll'),
  )
})
