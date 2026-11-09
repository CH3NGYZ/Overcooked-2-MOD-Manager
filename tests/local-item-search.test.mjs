import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 本地管理页搜索：真实源码直接执行（Node 24 类型擦除）。
 * 重点锁两件事：版本号（含卡片格式化后的写法）搜得到；内部数据搜不到。
 */
const { modSearchText, levelSetSearchText, matchesLocalSearch } =
  await import('../src/localItemSearch.ts')

function mod(overrides = {}) {
  return {
    name: 'ChefSight',
    title: '厨师视野',
    product_name: 'ChefSight Mod',
    plugin_name: 'ChefSightPlugin',
    original_filename: 'ChefSight.dll',
    description: '显示更多菜谱',
    company: 'Gua',
    copyright: 'Gua 2026',
    group: '辅助',
    group_i18n: { en: 'Helpers' },
    package_name: '厨师视野整合包',
    package_name_i18n: { en: 'ChefSight Bundle' },
    version: '1.8.7.0',
    note: '需要 DIYLevel 0.10',
    repository_binding: {
      provider: 'github',
      owner: 'Azhe',
      repository: 'ChefSight',
      tag: 'v1.8.7',
      channel: 'all',
    },
    dll_file: 'BepInEx/plugins/secret-folder/ChefSight.dll',
    guid: 'dev.gua.chefsight',
    version_source: 'assembly',
    status: 'ok',
    enabled: true,
    update_available: false,
    duplicate_guid: false,
    ...overrides,
  }
}

function levelSet(overrides = {}) {
  return {
    dir_name: 'JonnyLevelSet_a1b2',
    path: 'E:/Games/Overcooked/levels/JonnyLevelSet',
    enabled: true,
    name: 'JonnyLevelSet',
    name_zh: '职业鼠鼠的测图',
    author: 'Jonny',
    version: '3.4.1',
    installed_mapset_version: '3.4.1-fix',
    uid: 'deadbeef1234',
    base_uid: 'base-uid',
    level_count: 12,
    map_group: 'Jonny',
    map_group_i18n: { 'zh-CN': 'Jonny', en: 'Jonny' },
    map_id: 'community-maps::jonny',
    plugins: [{
      file_name: 'DIYLevel.dll',
      name: 'DIYLevel',
      title: 'DIY 关卡框架',
      product_name: 'DIYLevel Framework',
      company: 'Gua',
      version: '0.10.0',
      guid: 'dev.gua.overcooked.diylevel',
      role: 'mod',
      enabled: true,
    }],
    ...overrides,
  }
}

const hits = (query, item, ...extras) =>
  matchesLocalSearch(query, modSearchText(item), ...extras)
const mapHits = (query, item, ...extras) =>
  matchesLocalSearch(query, levelSetSearchText(item), ...extras)

test('MOD：名称的各种写法、作者、分类、包名与备注都能搜到', () => {
  const item = mod()
  for (const word of ['厨师视野', 'chefsight', 'Helpers', '辅助', 'ChefSight Bundle', 'Gua', 'DIYLevel 0.10']) {
    assert.ok(hits(word, item), `应命中：${word}`)
  }
})

test('MOD：版本号原文与卡片上格式化后的写法都能搜到', () => {
  const item = mod()
  assert.ok(hits('1.8.7.0', item), 'Release tag 原文')
  assert.ok(hits('1.8.7', item), '卡片显示的 1.8.7')
  assert.ok(hits('v1.8.7', item), '绑定的 Release tag')
})

test('MOD：自由文本版本（带后缀）也按原文命中', () => {
  const item = mod({ version: '2.0.0-beta.2' })
  assert.ok(hits('beta.2', item))
})

test('MOD：路径、GUID、状态枚举、布尔值与技术字段不参与搜索', () => {
  const item = mod()
  for (const word of [
    'secret-folder', 'dev.gua.chefsight', 'assembly', 'ok', 'true', 'all', 'BepInEx',
  ]) {
    assert.equal(hits(word, item), false, `不应命中内部数据：${word}`)
  }
})

test('MOD：空词不过滤，大小写与首尾空格不影响结果', () => {
  const item = mod()
  assert.ok(hits('', item))
  assert.ok(hits('   ', item))
  assert.ok(hits('  CHEFSIGHT  ', item))
})

test('地图：中英名、作者、分组与随包插件都能搜到', () => {
  const item = levelSet()
  for (const word of ['职业鼠鼠的测图', 'jonnyLevelSet', 'Jonny', 'DIY 关卡框架', 'DIYLevel Framework', 'DIYLevel.dll', '0.10.0']) {
    assert.ok(mapHits(word, item), `应命中：${word}`)
  }
})

test('地图：info 版本与安装时记录的资产版本都能搜到', () => {
  const item = levelSet()
  assert.ok(mapHits('3.4.1', item))
  assert.ok(mapHits('3.4.1-fix', item), '资产版本快照')
  const normalized = levelSet({ version: '1.8.7.0' })
  assert.ok(mapHits('1.8.7', normalized), '卡片格式化后的版本')
})

test('地图：路径、目录名、uid、内部 id、关卡数与插件 GUID 不参与搜索', () => {
  const item = levelSet()
  for (const word of [
    'E:/Games', 'deadbeef1234', 'a1b2', 'community-maps', '12', 'dev.gua.overcooked.diylevel',
  ]) {
    assert.equal(mapHits(word, item), false, `不应命中内部数据：${word}`)
  }
})

test('页面传入的额外文字（卡片显示标题、绑定地图名）参与匹配', () => {
  const item = mod()
  assert.equal(hits('线上显示名', item), false)
  assert.ok(hits('线上显示名', item, '线上显示名'))
  assert.ok(mapHits('清雨青雪的地图', levelSet(), '清雨青雪的地图', 'v3.4.1-fix'))
})


test('地图：info 内逐关中英文名称可搜索，兼容旧扫描结果及空名称', () => {
  const item = levelSet({ levels: [
    { name: 'BigBurger', name_zh: '宇宙超级无敌巨无霸汉堡登场', scene_name: 'internal_scene' },
    { name: 'Soup', name_zh: null },
  ] })
  assert.ok(mapHits('bigburger', item))
  assert.ok(mapHits('巨无霸汉堡', item))
  assert.ok(mapHits('SOUP', item))
  assert.equal(mapHits('internal_scene', item), false)
  assert.equal(mapHits('BigBurger', levelSet()), false)
})
