import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 依赖表行构造（`src/dependencyRows.ts`）。
 *
 * 这个模块是四个页面（MOD 安装页 / MOD 管理页 / 地图下载页 / 地图管理页）
 * 展示依赖的唯一口径，所以测试要锁住三件产品定过的事：
 *
 * 1. 目标名按「在线仓库名 → 本地安装包标题 → GUID」解析，不能把 GUID 摊给用户；
 * 2. 满足时状态只写「已满足」，**不带版本号**（版本号在「要求」列）；
 * 3. 只有「点一下能解决」的状态才可跳转，地图页永远不可跳。
 */
const {
  dependencyTargetName,
  dependencyRequirement,
  dependencyRowsFor,
  dependencyRowFromRelation,
  installedModRelations,
  installedModDependencyRows,
  levelSetRelations,
  levelSetDependencyRows,
  mapRelations,
  mapDependencyRows,
} = await import('../src/dependencyRows.ts')

/** 一台离线可用的假仓库：只填关系判定/取名会读的字段。 */
function repo(overrides = {}) {
  return {
    id: 'repo-a',
    name: 'Repo A',
    name_i18n: { 'zh-CN': '仓库 A' },
    group: '',
    group_i18n: {},
    provider: 'github',
    username: 'owner',
    repository: 'a',
    download_mode: 'release',
    branch: '',
    file_path: '',
    channel: 'all',
    asset_rules: [],
    install_target: '',
    note: '',
    plugin_guid: '',
    local_product_name: '',
    local_original_filename: '',
    version_regex: '',
    max_release_count: 10,
    download_threads: 0,
    enabled: true,
    builtin: false,
    cached_releases: [],
    relations: [],
    ...overrides,
  }
}

function mod(overrides = {}) {
  return {
    name: 'Plugin.dll',
    dll_file: 'Plugin.dll',
    enabled: true,
    version_source: 'dll',
    update_available: false,
    duplicate_guid: false,
    status: 'ok',
    ...overrides,
  }
}

function release(tag) {
  return { tag_name: tag, version: tag, name: tag, body: '', prerelease: false, draft: false, assets: [] }
}

function mapInfo(overrides = {}) {
  return {
    id: 'map-a',
    name: 'Map A',
    name_i18n: { 'zh-CN': '地图 A' },
    provider: 'github',
    username: 'owner',
    repository: 'map-a',
    level_set_name: 'RW_LevelSet',
    author: 'guagua',
    enabled: true,
    builtin: false,
    cached_releases: [],
    ...overrides,
  }
}

function levelSet(overrides = {}) {
  return {
    dir_name: 'RW_LevelSet',
    enabled: true,
    name: 'RW_LevelSet',
    author: 'guagua',
    version: '1.0.0',
    uid: 'uid',
    level_count: 3,
    ...overrides,
  }
}

/** 恒等翻译：返回 key，带参数时附上参数，便于断言「状态里没有版本号」。 */
const keyOf = (key, params) => (params && Object.keys(params).length
  ? `${key}(${JSON.stringify(params)})`
  : key)

/* ===== 目标名解析 ===== */

test('在线仓库名优先，并按发布线定位到正确的兄弟仓库', () => {
  const stable = repo({
    id: 'host-stable',
    channel: 'stable',
    plugin_guid: 'dev.Host',
    name: 'HostUtilities',
    name_i18n: { 'zh-CN': '主机实用 MOD（街机 MOD）稳定版' },
  })
  const beta = repo({
    id: 'host-beta',
    channel: 'beta',
    plugin_guid: 'dev.Host',
    name: 'HostUtilitiesBeta',
    name_i18n: { 'zh-CN': '主机实用 MOD（街机 MOD）测试版' },
  })
  const relation = { kind: 'conflicts', guid: 'dev.Host', prerelease: true }
  assert.equal(
    dependencyTargetName(relation, { repositories: [stable, beta] }),
    '主机实用 MOD（街机 MOD）测试版',
  )
})

test('线上没有该 GUID 时退回本地安装包标题（安装时固化的线上名字）', () => {
  const installed = mod({
    guid: 'dev.Host',
    package_name: 'HostUtilities',
    package_name_i18n: { 'zh-CN': '主机实用 MOD（街机 MOD）测试版' },
    title: 'HostUtilities',
  })
  assert.equal(
    dependencyTargetName({ guid: 'dev.Host' }, { mods: [installed] }),
    '主机实用 MOD（街机 MOD）测试版',
  )
})

test('本地包标题也没有时才用 DLL 标题，最后才是 GUID', () => {
  const titled = mod({ guid: 'dev.Host', title: 'HostUtilities' })
  assert.equal(dependencyTargetName({ guid: 'dev.Host' }, { mods: [titled] }), 'HostUtilities')
  assert.equal(dependencyTargetName({ guid: 'dev.Unknown' }, { mods: [titled] }), 'dev.Unknown')
})

test('本地化回退顺序可由调用方接管（视图传 pickI18n）', () => {
  const target = repo({ plugin_guid: 'dev.Host', name: 'fallback', name_i18n: { en: 'Host Utilities' } })
  const row = dependencyRowFromRelation(
    { guid: 'dev.Host' },
    { repositories: [target], pickName: (map, fallback) => map?.en ?? fallback },
  )
  assert.equal(row.name, 'Host Utilities')
})

/* ===== 状态文案 ===== */

test('满足时只写「已满足」，不把已装版本写进状态列', () => {
  const rows = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.fw', min_version: '1.0' }],
    { mods: [mod({ guid: 'dev.fw', version: '1.4.2' })], translate: keyOf },
  )
  assert.equal(rows.length, 1)
  assert.equal(rows[0].status, 'tools.satisfied')
  assert.equal(rows[0].tone, 'mild')
  assert.equal(rows[0].requirement, '≥ 1.0')
})

test('装了但版本不满足 → 可更新；没装但线上有满足区间的版本 → 可安装', () => {
  const source = repo({ id: 'fw', plugin_guid: 'dev.fw', cached_releases: [release('1.5.0')] })
  const outdated = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.fw', min_version: '1.2' }],
    {
      mods: [mod({ guid: 'dev.fw', version: '1.0.0' })],
      repositories: [source],
      translate: keyOf,
      jumpToRepository: true,
    },
  )
  assert.equal(outdated[0].status, 'tools.updatable')
  assert.equal(outdated[0].tone, 'moderate')
  assert.equal(outdated[0].jump?.id, 'fw')

  const absent = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.fw', min_version: '1.2' }],
    { mods: [], repositories: [source], translate: keyOf, jumpToRepository: true },
  )
  assert.equal(absent[0].status, 'tools.installable')
  assert.equal(absent[0].jump?.id, 'fw')
})

test('线上没有可安装来源 → 缺依赖，且不可跳转', () => {
  const rows = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.gone' }],
    { mods: [], translate: keyOf, jumpToRepository: true },
  )
  assert.equal(rows[0].status, 'modsOnline.missingDep')
  assert.equal(rows[0].tone, 'critical')
  assert.equal(rows[0].jump, undefined)
})

test('冲突/兼容用自己那套措辞，不共用「已满足」', () => {
  const hit = dependencyRowsFor(
    [{ kind: 'conflicts', guid: 'dev.x' }],
    { mods: [mod({ guid: 'dev.x', version: '2.0' })], translate: keyOf },
  )
  assert.equal(hit[0].status, 'modsOnline.conflict')

  const clear = dependencyRowsFor(
    [{ kind: 'conflicts', guid: 'dev.x' }],
    { mods: [], translate: keyOf },
  )
  assert.equal(clear[0].status, 'tools.noConflict')

  const compatible = dependencyRowsFor(
    [{ kind: 'compatible', guid: 'dev.x', max_version: '1.0' }],
    { mods: [mod({ guid: 'dev.x', version: '9.9' })], translate: keyOf },
  )
  assert.equal(compatible[0].status, 'tools.incompatible')
})

test('地图口径：未满足只说事实（未安装 / 版本不符），不给可安装按钮语义', () => {
  const satisfied = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', min_version: '1.0' }],
    { mods: [mod({ guid: 'dev.diy', version: '19.0' })], translate: keyOf, mode: 'map' },
  )
  assert.equal(satisfied[0].status, 'tools.satisfied')

  const absent = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', min_version: '1.0' }],
    { mods: [], translate: keyOf, mode: 'map' },
  )
  assert.equal(absent[0].status, 'levelSet.notInstalled')

  const mismatch = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', min_version: '19.0' }],
    { mods: [mod({ guid: 'dev.diy', version: '1.0' })], translate: keyOf, mode: 'map' },
  )
  assert.equal(mismatch[0].status, 'levelSet.versionMismatch({"version":"1.0"})')
})

/* ===== 行结构与过滤 ===== */

test('可选依赖不进表：缺失是正常状态，列出来只是噪音', () => {
  const rows = dependencyRowsFor(
    [
      { kind: 'requires', guid: 'dev.optional', optional: true },
      { kind: 'requires', guid: 'dev.required' },
    ],
    { mods: [], translate: keyOf },
  )
  assert.deepEqual(rows.map((row) => row.name), ['dev.required'])
})

test('key 带发布线：同 GUID 的「与正式版互斥 / 与测试版互斥」是两行', () => {
  const rows = dependencyRowsFor([
    { kind: 'conflicts', guid: 'dev.host', prerelease: true },
    { kind: 'conflicts', guid: 'dev.host', prerelease: false },
  ], { mods: [], translate: keyOf })
  assert.equal(rows.length, 2)
  assert.notEqual(rows[0].key, rows[1].key)
  assert.equal(rows[0].line, 'prerelease')
  assert.equal(rows[1].line, 'stable')
})

test('版本要求文本：单边与区间两种写法', () => {
  assert.equal(dependencyRequirement({ guid: 'a', min_version: '1.0' }), '≥ 1.0')
  assert.equal(dependencyRequirement({ guid: 'a', max_version: '2.0' }), '≤ 2.0')
  assert.equal(dependencyRequirement({ guid: 'a', min_version: '1.0', max_version: '2.0' }), '1.0 ~ 2.0')
  assert.equal(dependencyRequirement({ guid: 'a' }), '')
})

test('jumpToRepository=false 时不产生任何跳转目标', () => {
  const source = repo({ id: 'fw', plugin_guid: 'dev.fw', cached_releases: [release('1.5.0')] })
  const rows = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.fw' }],
    { mods: [], repositories: [source], translate: keyOf, jumpToRepository: false },
  )
  assert.equal(rows[0].status, 'tools.installable')
  assert.equal(rows[0].jump, undefined)
})

/* ===== 「在线为准，本地兜底」========== */

test('已安装 MOD 的关系声明：绑定仓库在线时以在线为准（含版本区间覆盖）', () => {
  const online = repo({
    id: 'cursor',
    plugin_guid: 'dev.cursor',
    relations: [{ kind: 'requires', guid: 'dev.old' }],
    release_relations: [
      { min_version: '2.0', relations: [{ kind: 'requires', guid: 'dev.new', min_version: '3.0' }] },
    ],
  })
  const installed = mod({ repository_id: 'cursor', guid: 'dev.cursor', version: '2.4.0', relations: [{ guid: 'dev.snapshot' }] })
  assert.deepEqual(
    installedModRelations(installed, [online]).map((relation) => relation.guid),
    ['dev.new'],
  )
})

test('绑定仓库不在配置里时退回安装清单快照', () => {
  const installed = mod({ repository_id: 'gone', relations: [{ kind: 'requires', guid: 'dev.snapshot' }] })
  assert.deepEqual(installedModRelations(installed, []).map((relation) => relation.guid), ['dev.snapshot'])
})

test('MOD 管理页的行：用已装版本筛 release_relations，名称走在线卡片', () => {
  const online = repo({
    id: 'cursor',
    plugin_guid: 'dev.cursor',
    name_i18n: { 'zh-CN': '光标 MOD' },
    release_relations: [
      { min_version: '2.0', relations: [{ kind: 'requires', guid: 'dev.fw', min_version: '2.0' }] },
    ],
  })
  const fw = repo({ id: 'fw', plugin_guid: 'dev.fw', name_i18n: { 'zh-CN': '框架' }, cached_releases: [release('2.1.0')] })
  const rows = installedModDependencyRows(
    mod({ repository_id: 'cursor', guid: 'dev.cursor', version: '2.4.0' }),
    { mods: [], repositories: [online, fw], translate: keyOf },
  )
  assert.equal(rows.length, 1)
  assert.equal(rows[0].name, '框架')
  assert.equal(rows[0].status, 'tools.installable')
})

test('地图关系：在线条目在就按在线，不在才用本地快照', () => {
  const online = mapInfo({ relations: [{ kind: 'requires', guid: 'dev.diy', min_version: '19.0' }] })
  assert.deepEqual(
    levelSetRelations(levelSet(), [online]).map((relation) => relation.guid),
    ['dev.diy'],
  )
  assert.deepEqual(
    levelSetRelations(levelSet({ relations: [{ kind: 'requires', guid: 'dev.snapshot' }] }), []).map((relation) => relation.guid),
    ['dev.snapshot'],
  )
})

test('老快照没有 relations 时退回 required_frameworks 投影', () => {
  const legacy = mapInfo({ required_frameworks: [{ guid: 'dev.diy', min_version: '19.0' }] })
  const relations = mapRelations(legacy)
  assert.deepEqual(relations, [{ kind: 'requires', guid: 'dev.diy', min_version: '19.0', max_version: '' }])
})

test('地图页的行永不带跳转；本地关卡集与地图源条目共用同一套名字解析', () => {
  const map = mapInfo({ relations: [{ kind: 'requires', guid: 'dev.diy', min_version: '19.0' }] })
  const diy = repo({ id: 'diy', plugin_guid: 'dev.diy', name_i18n: { 'zh-CN': 'DIYLevel' } })
  const rows = mapDependencyRows(map, { mods: [], repositories: [diy], translate: keyOf })
  assert.equal(rows[0].name, 'DIYLevel')
  assert.equal(rows[0].status, 'levelSet.notInstalled')
  assert.equal(rows[0].jump, undefined)

  const levelRows = levelSetDependencyRows(levelSet(), {
    maps: [map], mods: [], repositories: [diy], translate: keyOf,
  })
  assert.deepEqual(levelRows, rows)
})
