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
  mergedModDependencyRows,
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
  assert.equal(dependencyRequirement({ guid: 'a', version: '>=1.0' }), '>=1.0')
  assert.equal(dependencyRequirement({ guid: 'a', version: '<=2.0' }), '<=2.0')
  assert.equal(dependencyRequirement({ guid: 'a', version: '>=1.0,<=2.0' }), '>=1.0,<=2.0')
  assert.equal(dependencyRequirement({ guid: 'a' }), '')
})

/* ===== 「在线为准，本地兜底」========== */

test('已安装 MOD 的关系声明：绑定仓库在线时以在线为准（含版本区间覆盖）', () => {
  const online = repo({
    id: 'cursor',
    plugin_guid: 'dev.cursor',
    relations: [{ kind: 'requires', guid: 'dev.old' }],
    release_relations: [
      { version: '>=2.0', relations: [{ kind: 'requires', guid: 'dev.new', version: '>=3.0' }] },
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

test('用扫描确认的 Release / 磁盘版本选择关系，不把核心的声明加给附属 DLL', () => {
  const online = repo({
    id: 'cursor', plugin_guid: 'dev.cursor',
    relations: [{ kind: 'requires', guid: 'dev.default' }],
    release_relations: [{ version: '<3.0', relations: [{ kind: 'requires', guid: 'dev.release' }] }],
  })
  const core = mod({ repository_id: 'cursor', version: '9.0.0', update_version: 'v2.0.0' })
  const replaced = { ...core, version: '3.0.0', update_version: '3.0.0' }
  assert.deepEqual(installedModRelations(core, [online]).map(relation => relation.guid), ['dev.release'])
  assert.deepEqual(installedModRelations(replaced, [online]).map(relation => relation.guid), ['dev.default'])
  assert.deepEqual(installedModRelations({ ...core, release_companion: true }, [online]), [])
})

test('地图关系：在线条目在就按在线，不在才用本地快照', () => {
  const online = mapInfo({ relations: [{ kind: 'requires', guid: 'dev.diy', version: '>=19.0' }] })
  assert.deepEqual(
    levelSetRelations(levelSet(), [online]).map((relation) => relation.guid),
    ['dev.diy'],
  )
  assert.deepEqual(
    levelSetRelations(levelSet({ relations: [{ kind: 'requires', guid: 'dev.snapshot' }] }), []).map((relation) => relation.guid),
    ['dev.snapshot'],
  )
})

test('已安装关卡集按自己的版本取覆盖表，不跟最新版走', () => {
  const online = mapInfo({
    latest_tag: 'v2.0.0',
    relations: [{ kind: 'requires', guid: 'dev.old', version: '>=1.0' }],
    release_relations: [
      { version: '>=2.0', relations: [{ kind: 'requires', guid: 'dev.new', version: '>=2.0' }] },
    ],
  })
  assert.deepEqual(
    levelSetRelations(levelSet({ version: '1.4.0' }), [online]).map((relation) => relation.guid),
    ['dev.old'],
  )
  assert.deepEqual(
    levelSetRelations(levelSet({ version: '2.1.0' }), [online]).map((relation) => relation.guid),
    ['dev.new'],
  )
})

test('老快照没有 relations 时退回 required_frameworks 投影', () => {
  const legacy = mapInfo({ required_frameworks: [{ guid: 'dev.diy', version: '>=19.0' }] })
  const relations = mapRelations(legacy)
  assert.deepEqual(relations, [{ kind: 'requires', guid: 'dev.diy', version: '>=19.0' }])
})

test('地图覆盖表按版本命中，取区间下限最高的那张（整体替换）', () => {
  const map = mapInfo({
    relations: [{ kind: 'requires', guid: 'dev.base', version: '>=1.0' }],
    release_relations: [
      { version: '>=1.1', relations: [{ kind: 'requires', guid: 'dev.mid', version: '>=1.0' }] },
      { version: '>=1.5,<2.0', relations: [{ kind: 'requires', guid: 'dev.new', version: '>=1.0' }] },
    ],
  })
  assert.deepEqual(mapRelations(map, '1.0').map((relation) => relation.guid), ['dev.base'])
  assert.deepEqual(mapRelations(map, '1.2').map((relation) => relation.guid), ['dev.mid'])
  // 两条都命中时取下限更高的那条，而不是先出现的
  assert.deepEqual(mapRelations(map, '1.6').map((relation) => relation.guid), ['dev.new'])
  // 被上限排除后回落到仍命中的那条；低于所有下限才回默认声明
  assert.deepEqual(mapRelations(map, '2.1').map((relation) => relation.guid), ['dev.mid'])
  assert.deepEqual(mapRelations(map, '0.9').map((relation) => relation.guid), ['dev.base'])
})

test('地图覆盖表下限相同时先出现的生效（与后端同一条规则）', () => {
  const map = mapInfo({
    release_relations: [
      { version: '>=1.0', relations: [{ kind: 'requires', guid: 'dev.first' }] },
      { version: '>=1.0', relations: [{ kind: 'requires', guid: 'dev.second' }] },
    ],
  })
  assert.deepEqual(mapRelations(map, '1.5').map((relation) => relation.guid), ['dev.first'])
})

test('地图卡片依赖跟「将要安装的版本」走：默认 latest_tag，选中旧版用旧版声明', () => {
  const map = mapInfo({
    latest_tag: 'v2.0.0',
    relations: [{ kind: 'requires', guid: 'dev.old', version: '>=1.0' }],
    release_relations: [
      { version: '>=2.0', relations: [{ kind: 'requires', guid: 'dev.new', version: '>=2.0' }] },
    ],
  })
  assert.deepEqual(
    mapDependencyRows(map, { mods: [], translate: keyOf }).map((row) => row.name),
    ['dev.new'],
  )
  assert.deepEqual(
    mapDependencyRows(map, { mods: [], translate: keyOf, version: 'v1.4.0' }).map((row) => row.name),
    ['dev.old'],
  )
})
