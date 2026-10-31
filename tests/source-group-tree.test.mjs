import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 「源 → 分类 → 卡片」三层折叠树的纯函数部分（`src/composables/useSourceGroupTree.ts`）。
 *
 * MOD 安装页与地图安装页共用这一份实现，`sourceGroupTree`/折叠状态都不依赖
 * Vue 实例，所以规则在这里直接锁死。三条不变量各自对应一类真实事故：
 *
 * 1. **分桶键用原文**。见过一次的反面实现是按翻译后的分组名排序：用户切到
 *    英文，同一个分组的键换了、顺序跟着跳，看起来像「分组全乱了」。
 * 2. **顺序按声明顺序**。源按源文档顺序，分类按条目首次出现顺序；排序会
 *    引入与语言/数据无关的抖动。
 * 3. **默认全展开、搜索强制全展开**。搜索时把命中卡片折在别的分类里，等于
 *    搜索没生效；而「用户把最后一个源收起」必须真的收起（地图页往往只有
 *    一个源，用空数组当「没动过」的哨兵会让第一层折叠彻底失效）。
 */
const {
  UNGROUPED_GROUP_KEY,
  groupKeyOf,
  buildSourceGroupTree,
  sourceKeysOf,
  groupKeysOf,
  expandedNames,
} = await import('../src/composables/useSourceGroupTree.ts')

/** 条目工厂：默认给一个分类与一条数据。 */
function entry(overrides = {}) {
  return {
    sourceId: 'src-a',
    sourceLabel: '',
    group: 'Group A',
    groupLabel: '分组 A',
    item: 'item',
    ...overrides,
  }
}

test('分类键只由原文决定：大小写与首尾空白归一到同一个桶', () => {
  assert.equal(groupKeyOf(' Map '), groupKeyOf('map'))
  assert.equal(groupKeyOf('Map'), 'group:map')
  // 中文与英文是两个不同的分组：规范化不等于翻译
  assert.notEqual(groupKeyOf('地图'), groupKeyOf('MAPS'))
})

test('空分类归入未分类桶，且不把翻译文案写进键里', () => {
  assert.equal(groupKeyOf(''), UNGROUPED_GROUP_KEY)
  assert.equal(groupKeyOf('   '), UNGROUPED_GROUP_KEY)
  assert.equal(groupKeyOf(null), UNGROUPED_GROUP_KEY)
  assert.equal(groupKeyOf(undefined), UNGROUPED_GROUP_KEY)
})

test('切语言只换 label：分桶键、分类顺序与源顺序都不变', () => {
  const entries = (translate) =>
    [
      entry({ group: 'b-group', groupLabel: translate === 'zh' ? '乙' : 'B', item: 'b1' }),
      entry({ group: 'a-group', groupLabel: translate === 'zh' ? '甲' : 'A', item: 'a1' }),
      entry({ group: 'a-group', groupLabel: translate === 'zh' ? '甲' : 'A', item: 'a2' }),
    ]
  const zh = buildSourceGroupTree(entries('zh'), { ungrouped: { id: '__none__', label: '未归类' } })
  const en = buildSourceGroupTree(entries('en'), { ungrouped: { id: '__none__', label: 'Uncategorized' } })
  assert.deepEqual(
    zh.map((node) => node.groups.map((group) => group.key)),
    en.map((node) => node.groups.map((group) => group.key)),
  )
  // 分类顺序 = 首次出现顺序（b 在前），不是按名字排序
  assert.deepEqual(zh[0].groups.map((group) => group.label), ['乙', '甲'])
  assert.deepEqual(en[0].groups.map((group) => group.label), ['B', 'A'])
  // 同组条目累加、计数与条目总数一致
  assert.deepEqual(zh[0].groups[1].items, ['a1', 'a2'])
  assert.equal(zh[0].count, 3)
})

test('源顺序：order 声明的按声明顺序在前，其余按首次出现补在后', () => {
  const entries = [
    entry({ sourceId: 'src-z', item: 'z1' }),
    entry({ sourceId: 'src-a', item: 'a1' }),
    entry({ sourceId: 'src-m', item: 'm1' }),
  ]
  const tree = buildSourceGroupTree(entries, {
    // 只声明 a：z 与 m 是未声明源，按条目首次出现顺序（z → m）跟在后面
    order: [{ id: 'src-a', label: '源 A' }],
    ungrouped: { id: '__none__', label: '未归类' },
  })
  assert.deepEqual(tree.map((node) => node.id), ['src-a', 'src-z', 'src-m'])
  assert.equal(tree[0].label, '源 A')
  // 未声明源没有昵称：退化成源 ID（显示 ID 也好过混进「未归类」）
  assert.equal(tree[1].label, 'src-z')
})

test('order 里声明了但没有条目的源不出现（不产生空标题）', () => {
  const tree = buildSourceGroupTree([entry({ sourceId: 'src-a' })], {
    order: [{ id: 'src-ghost', label: '空源' }, { id: 'src-a', label: '源 A' }],
    ungrouped: { id: '__none__', label: '未归类' },
  })
  assert.deepEqual(tree.map((node) => node.id), ['src-a'])
})

test('没有源归属的条目进未归类桶，并固定垫底', () => {
  const entries = [
    entry({ sourceId: '', group: '', sourceLabel: '', item: 'orphan' }),
    entry({ sourceId: 'src-a', item: 'a1' }),
  ]
  const tree = buildSourceGroupTree(entries, { ungrouped: { id: '__none__', label: '未归类' } })
  assert.deepEqual(tree.map((node) => node.id), ['src-a', '__none__'])
  assert.equal(tree[1].label, '未归类')
  // 分类标题由调用方给（页面传「其他」），键仍是与语言无关的未分类键
  assert.equal(tree[1].groups[0].key, '__none__::__ungrouped__')
  assert.deepEqual(tree[1].groups[0].items, ['orphan'])
})

test('flat 源只有一组、无分类标题，且分类键不进展开列表', () => {
  const entries = [
    entry({ sourceId: 'community', group: 'x', groupLabel: 'X', item: 'c1' }),
    entry({ sourceId: 'community', group: 'y', groupLabel: 'Y', item: 'c2' }),
  ]
  const tree = buildSourceGroupTree(entries, {
    ungrouped: { id: '__none__', label: '未归类' },
    flat: (sourceId) => sourceId === 'community',
  })
  assert.equal(tree[0].flat, true)
  assert.equal(tree[0].groups.length, 1)
  assert.equal(tree[0].groups[0].key, 'community::flat')
  assert.equal(tree[0].groups[0].label, '')
  assert.deepEqual(tree[0].groups[0].items, ['c1', 'c2'])
  // 不分类的源没有分类折叠项（否则会多出一个点不开的空标题）
  assert.deepEqual(groupKeysOf(tree), [])
  assert.deepEqual(sourceKeysOf(tree), ['community'])
})

test('源名为空时的兜底顺序：条目上的源名优先于源 ID', () => {
  const tree = buildSourceGroupTree(
    [entry({ sourceId: 'src-a', sourceLabel: '作者源' })],
    { ungrouped: { id: '__none__', label: '未归类' } },
  )
  assert.equal(tree[0].label, '作者源')
})

test('展开规则：默认全展开、搜索时全展开、用户收起后保持收起', () => {
  const all = ['a', 'b']
  // 还没交互过（null）→ 全展开
  assert.deepEqual(expandedNames(all, null, false), all)
  // 用户把最后一个展开项也收起 → 真的收起（空数组不能被当成「没动过」）
  assert.deepEqual(expandedNames(all, [], false), [])
  assert.deepEqual(expandedNames(all, ['a'], false), ['a'])
  // 搜索时强制全展开：命中的卡片不能折在别的分类里
  assert.deepEqual(expandedNames(all, [], true), all)
  assert.deepEqual(expandedNames(all, ['a'], true), all)
})
