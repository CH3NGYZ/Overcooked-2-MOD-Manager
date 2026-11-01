import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 两级卡片的展开状态（`src/composables/useCardExpansion.ts`）。
 *
 * MOD 管理页（包外壳 → 包内子 MOD / 随包依赖）与地图管理页（集卡片 → 随包插件）
 * 共用这一份实现，所以这里锁的不是某一页的行为，而是两页必须一致的规则：
 * **外层收起时，它名下的内层展开状态一起清掉** —— 下次展开是一张全新的卡片，
 * 而不是上次留下的半开状态（那种状态下内层卡片用户得再点一次才收得回）。
 *
 * 另外两条：多开域（MOD 包）里清一个包不许动别的包；单展开域（地图集）里
 * 展开 B 时被动收起的 A 也必须清干净内层。
 */
const { useCardExpansion } = await import('../src/composables/useCardExpansion.ts')

test('外层收起时，它名下的内层展开状态一起清掉', () => {
  const cards = useCardExpansion()
  cards.openOuter('pkg-a')
  cards.openInner('pkg-a', 'main.dll')
  cards.openInner('pkg-a', 'dep.dll')

  cards.closeOuter('pkg-a')

  assert.equal(cards.isOuterOpen('pkg-a'), false)
  assert.equal(cards.isInnerOpen('pkg-a', 'main.dll'), false)
  assert.equal(cards.isInnerOpen('pkg-a', 'dep.dll'), false)
  assert.deepEqual(cards.innerKeys.value, [])
})

test('重新展开外层不会把上次的内层状态带回来', () => {
  const cards = useCardExpansion()
  cards.openOuter('pkg-a')
  cards.openInner('pkg-a', 'main.dll')
  cards.closeOuter('pkg-a')

  cards.openOuter('pkg-a')

  assert.equal(cards.isOuterOpen('pkg-a'), true)
  assert.equal(cards.isInnerOpen('pkg-a', 'main.dll'), false)
})

test('多开域：清一个包的内层不影响别的包', () => {
  const cards = useCardExpansion()
  cards.openOuter('pkg-a')
  cards.openOuter('pkg-b')
  cards.openInner('pkg-a', 'main.dll')
  cards.openInner('pkg-b', 'main.dll')

  cards.closeOuter('pkg-a')

  assert.equal(cards.isOuterOpen('pkg-b'), true)
  assert.equal(cards.isInnerOpen('pkg-b', 'main.dll'), true)
})

test('多开域：展开新包不会收起已展开的包', () => {
  const cards = useCardExpansion()
  cards.openOuter('pkg-a')
  cards.openOuter('pkg-b')

  assert.deepEqual(cards.outerKeys.value, ['pkg-a', 'pkg-b'])
})

test('单展开域：展开 B 时被动收起的 A 连同内层一起清掉', () => {
  const cards = useCardExpansion({ singleOuter: true })
  cards.openOuter('set-a')
  cards.openInner('set-a', 'plugin.dll')

  cards.openOuter('set-b')

  assert.deepEqual(cards.outerKeys.value, ['set-b'])
  assert.equal(cards.isInnerOpen('set-a', 'plugin.dll'), false)
})

test('单展开域：重复展开同一张不会丢内层状态', () => {
  const cards = useCardExpansion({ singleOuter: true })
  cards.openOuter('set-a')
  cards.openInner('set-a', 'plugin.dll')

  cards.openOuter('set-a')

  assert.equal(cards.isInnerOpen('set-a', 'plugin.dll'), true)
})

test('toggleOuter 返回切换后的展开状态', () => {
  const cards = useCardExpansion()

  assert.equal(cards.toggleOuter('pkg-a'), true)
  assert.equal(cards.isOuterOpen('pkg-a'), true)

  assert.equal(cards.toggleOuter('pkg-a'), false)
  assert.equal(cards.isOuterOpen('pkg-a'), false)
})

test('toggleInner 与 closeInner 只动目标那一项', () => {
  const cards = useCardExpansion()
  cards.openOuter('pkg-a')
  cards.openInner('pkg-a', 'keep.dll')

  assert.equal(cards.toggleInner('pkg-a', 'main.dll'), true)
  assert.equal(cards.toggleInner('pkg-a', 'main.dll'), false)
  assert.equal(cards.isInnerOpen('pkg-a', 'keep.dll'), true)

  cards.closeInner('pkg-a', 'keep.dll')
  assert.equal(cards.isInnerOpen('pkg-a', 'keep.dll'), false)
})

test('closeAllInner 只收内层，外层保持（MOD 页点空白不收起包外壳）', () => {
  const cards = useCardExpansion()
  cards.openOuter('pkg-a')
  cards.openInner('pkg-a', 'main.dll')

  cards.closeAllInner()

  assert.equal(cards.isOuterOpen('pkg-a'), true)
  assert.equal(cards.isInnerOpen('pkg-a', 'main.dll'), false)
})

test('closeAll 内外层一起收（地图页点空白）', () => {
  const cards = useCardExpansion({ singleOuter: true })
  cards.openOuter('set-a')
  cards.openInner('set-a', 'plugin.dll')

  cards.closeAll()

  assert.deepEqual(cards.outerKeys.value, [])
  assert.deepEqual(cards.innerKeys.value, [])
})

test('childOf 还原内层键里的子键，父键与子键同名也不会串', () => {
  const cards = useCardExpansion()
  cards.openOuter('DIYLevel')
  cards.openInner('DIYLevel', 'DIYLevel.dll')

  const [key] = cards.innerKeys.value
  assert.equal(cards.childOf(key), 'DIYLevel.dll')
})
