import assert from 'node:assert/strict'
import test from 'node:test'
import { computed, ref } from 'vue'
import { useLocalGroupExpansion } from '../src/composables/useLocalGroupExpansion.ts'

const {
  collapsedGroupNames,
  expandedGroupNames,
  expandGroupName,
} = await import('../src/groupExpansion.ts')

test('本地分组默认展开，新分组不受旧折叠状态影响', () => {
  assert.deepEqual(expandedGroupNames(['a', 'b'], [], false), ['a', 'b'])
  assert.deepEqual(expandedGroupNames(['a', 'b', 'c'], ['b'], false), ['a', 'c'])
  assert.deepEqual(expandedGroupNames(['a', 'b'], ['a', 'b'], false), [])
})

test('筛选期间强制展开但不改用户折叠记录', () => {
  assert.deepEqual(expandedGroupNames(['a', 'b'], ['b'], true), ['a', 'b'])
  assert.deepEqual(collapsedGroupNames(['a', 'b', 'c'], ['a', 'c']), ['b'])
})

test('跨页定位只移除目标组的折叠标记', () => {
  assert.deepEqual(expandGroupName(['a', 'b'], 'b'), ['a'])
  assert.deepEqual(expandGroupName([], 'b'), [])
})

test('composable 保留用户折叠，筛选回写被忽略，新增组默认展开', () => {
  const keys = ref(['a', 'b'])
  const filtering = ref(false)
  const visibility = useLocalGroupExpansion(keys, computed(() => filtering.value), () => filtering.value)

  assert.deepEqual(visibility.expandedKeys.value, ['a', 'b'])
  visibility.onExpanded(['a'])
  assert.deepEqual(visibility.expandedKeys.value, ['a'])
  assert.equal(visibility.isCollapsed('b'), true)

  keys.value = ['a', 'b', 'c']
  assert.deepEqual(visibility.expandedKeys.value, ['a', 'c'])

  filtering.value = true
  visibility.onExpanded([])
  assert.deepEqual(visibility.expandedKeys.value, ['a', 'b', 'c'])
  filtering.value = false
  assert.deepEqual(visibility.expandedKeys.value, ['a', 'c'])

  visibility.expandGroup('b')
  assert.deepEqual(visibility.expandedKeys.value, ['a', 'b', 'c'])
})
