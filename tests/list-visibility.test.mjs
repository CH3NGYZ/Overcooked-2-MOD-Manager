import assert from 'node:assert/strict'
import test from 'node:test'

/** 两页共用的列表可见性规则：真实源码直接执行（Node 24 类型擦除）。 */
const {
  nextStatusKey,
  bulkSwitchValue,
  statusKeyMatches,
  visibilityCountLabel,
  visibilityEmptyText,
} = await import('../src/listVisibility.ts')

const texts = {
  noData: () => '没有数据',
  noQueryMatch: () => '没有匹配的搜索结果',
  noStatusMatch: () => '当前状态下没有可显示的项',
}

test('状态筛选是单选：换状态直接切过去，再点同一个取消', () => {
  assert.equal(nextStatusKey(null, 'enabled'), 'enabled')
  assert.equal(nextStatusKey('enabled', 'disabled'), 'disabled')
  assert.equal(nextStatusKey('enabled', 'enabled'), null)
})

test('批量开关状态由完整列表投影，空列表和混合态均保持未选中', () => {
  assert.equal(bulkSwitchValue(0, 0), '')
  assert.equal(bulkSwitchValue(0, 3), 'disable')
  assert.equal(bulkSwitchValue(3, 3), 'enable')
  assert.equal(bulkSwitchValue(1, 3), '')
  assert.equal(bulkSwitchValue(2, 3), '')
})

test('未选状态时所有单元可见，选中后只留含该状态的单元', () => {
  assert.equal(statusKeyMatches(null, []), true)
  assert.equal(statusKeyMatches(null, ['latest']), true)
  assert.equal(statusKeyMatches('enabled', ['enabled', 'latest']), true)
  assert.equal(statusKeyMatches('enabled', ['disabled', 'latest']), false)
  assert.equal(statusKeyMatches('enabled', []), false)
})

test('只有筛选时才显示「命中 / 全部」', () => {
  assert.equal(visibilityCountLabel(false, 9, 9), null)
  assert.equal(visibilityCountLabel(true, 1, 9), '1 / 9')
  assert.equal(visibilityCountLabel(true, 0, 9), '0 / 9')
})

test('空态区分：没数据 / 搜索没命中 / 状态筛选没命中', () => {
  const state = (overrides = {}) => ({
    hasItems: true,
    visible: 0,
    searching: false,
    statusFiltering: false,
    ...overrides,
  })
  assert.equal(visibilityEmptyText(state({ hasItems: false }), texts), '没有数据')
  assert.equal(visibilityEmptyText(state({ searching: true }), texts), '没有匹配的搜索结果')
  assert.equal(
    visibilityEmptyText(state({ statusFiltering: true }), texts),
    '当前状态下没有可显示的项',
  )
  // 两个维度同时生效时优先说搜索：用户最先看到的是搜索框
  assert.equal(
    visibilityEmptyText(state({ searching: true, statusFiltering: true }), texts),
    '没有匹配的搜索结果',
  )
  // 有可见项时不该给空态（调用方用 n-empty v-if 挡住，这里仍返回兜底文案）
  assert.equal(visibilityEmptyText(state({ visible: 3 }), texts), '没有数据')
})

test('selected status clears when its last item changes state', async () => {
  const { ref, computed, nextTick } = await import('vue')
  const { useListVisibility } = await import('../src/composables/useListVisibility.ts')
  const keys = ref(['update', 'latest'])
  const view = useListVisibility(ref(''), texts)
  view.reconcileStatuses(computed(() => keys.value))
  view.toggleStatus('update')
  keys.value = ['latest']
  await nextTick()
  assert.equal(view.activeStatus.value, null)
  assert.equal(view.statusMatches(['latest']), true)
})
