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

/* ===== 状态文案 ===== */

test('满足时只写「已满足」，不把已装版本写进状态列', () => {
  const rows = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.fw', version: '>=1.0' }],
    { mods: [mod({ guid: 'dev.fw', version: '1.4.2' })], translate: keyOf },
  )
  assert.equal(rows.length, 1)
  assert.equal(rows[0].status, 'tools.satisfied')
  assert.equal(rows[0].tone, 'mild')
  assert.equal(rows[0].requirement, '>=1.0')
})

test('装了但版本不满足 → 可更新；没装但线上有满足区间的版本 → 可安装', () => {
  const source = repo({ id: 'fw', plugin_guid: 'dev.fw', cached_releases: [release('1.5.0')] })
  const outdated = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.fw', version: '>=1.2' }],
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
    [{ kind: 'requires', guid: 'dev.fw', version: '>=1.2' }],
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
    [{ kind: 'compatible', guid: 'dev.x', version: '<=1.0' }],
    { mods: [mod({ guid: 'dev.x', version: '9.9' })], translate: keyOf },
  )
  assert.equal(compatible[0].status, 'tools.incompatible')
})

test('地图与 MOD 同一口径：可安装 / 可更新 / 缺依赖，不再自造「未安装」', () => {
  const satisfied = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', version: '>=1.0' }],
    { mods: [mod({ guid: 'dev.diy', version: '19.0' })], translate: keyOf },
  )
  assert.equal(satisfied[0].status, 'tools.satisfied')

  const absent = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', version: '>=1.0' }],
    { mods: [], translate: keyOf },
  )
  assert.equal(absent[0].status, 'modsOnline.missingDep')

  const installable = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', version: '>=1.0' }],
    {
      mods: [],
      repositories: [repo({
        id: 'diy',
        plugin_guid: 'dev.diy',
        cached_releases: [{ tag_name: 'v1.2.0', version: '1.2.0', draft: false, prerelease: false }],
      })],
      translate: keyOf,
    },
  )
  assert.equal(installable[0].status, 'tools.installable')
  assert.equal(installable[0].tone, 'moderate')

  // 装了但版本不够 →「可更新」，与 MOD 卡片上的结论同一套
  const outdated = dependencyRowsFor(
    [{ kind: 'requires', guid: 'dev.diy', version: '>=19.0' }],
    {
      mods: [mod({ guid: 'dev.diy', version: '1.0' })],
      repositories: [repo({
        id: 'diy',
        plugin_guid: 'dev.diy',
        cached_releases: [{ tag_name: 'v19.2', version: '19.2', draft: false, prerelease: false }],
      })],
      translate: keyOf,
    },
  )
  assert.equal(outdated[0].status, 'tools.updatable')
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

test('MOD 管理页的行：用已装版本筛 release_relations，名称走在线卡片', () => {
  const online = repo({
    id: 'cursor',
    plugin_guid: 'dev.cursor',
    name_i18n: { 'zh-CN': '光标 MOD' },
    release_relations: [
      { version: '>=2.0', relations: [{ kind: 'requires', guid: 'dev.fw', version: '>=2.0' }] },
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

test('包外壳的依赖表合并全部成员：高版本成员的覆盖规则不会被低版本成员盖掉', () => {
  const bundle = repo({
    id: 'bundle',
    plugin_guid: 'dev.bundle',
    name_i18n: { 'zh-CN': '前置包' },
    relations: [{ kind: 'requires', guid: 'dev.diy' }],
    release_relations: [
      { version: '==3.10.2', relations: [{ kind: 'requires', guid: 'dev.diy', version: '>=0.10.0' }] },
    ],
  })
  const diy = repo({
    id: 'diy',
    plugin_guid: 'dev.diy',
    name_i18n: { 'zh-CN': 'DIYLevel' },
    cached_releases: [release('0.10.0')],
  })
  const installedDiy = mod({
    name: 'DIYLevel.dll',
    dll_file: 'DIYLevel.dll',
    guid: 'dev.diy',
    version: '0.9.9',
  })
  // 同一个包里的两支 DLL：只有 3.10.2 命中覆盖规则，2.0.0 只有仓库级声明
  const low = mod({
    name: 'debugLog.dll',
    dll_file: 'debugLog.dll',
    repository_id: 'bundle',
    guid: 'dev.bundle.debug',
    version: '2.0.0',
  })
  const high = mod({
    name: 'Loader.dll',
    dll_file: 'Loader.dll',
    repository_id: 'bundle',
    guid: 'dev.bundle',
    version: '3.10.2',
  })
  const options = { mods: [installedDiy], repositories: [bundle, diy], translate: keyOf }

  // 只按低版本成员算：仓库级声明不设区间 → 「已满足」，与包标题的「缺依赖」自相矛盾
  assert.equal(installedModDependencyRows(low, options)[0].status, 'tools.satisfied')

  // 合并后取严：>=0.10.0 遇上本地 0.9.9 → 依赖表说可更新；包标题按新规则写「缺依赖」，两者指向同一件事（那个前置要升级）
  const merged = mergedModDependencyRows([low, high], options)
  assert.equal(merged.length, 1)
  assert.equal(merged[0].status, 'tools.updatable')
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

test('地图的行也带跳转：本地关卡集与地图源条目共用同一套名字解析与落点', () => {
  const map = mapInfo({ relations: [{ kind: 'requires', guid: 'dev.diy', version: '>=19.0' }] })
  const diy = repo({
    id: 'diy',
    plugin_guid: 'dev.diy',
    name_i18n: { 'zh-CN': 'DIYLevel' },
    cached_releases: [{ tag_name: 'v19.2', version: '19.2', draft: false, prerelease: false }],
  })
  const rows = mapDependencyRows(map, { mods: [], repositories: [diy], translate: keyOf })
  assert.equal(rows[0].name, 'DIYLevel')
  // 线上有满足区间的版本 → 「可安装」，并且落点是那张仓库卡（与 MOD 侧一致）
  assert.equal(rows[0].status, 'tools.installable')
  assert.equal(rows[0].jump?.id, 'diy')

  const levelRows = levelSetDependencyRows(levelSet(), {
    maps: [map], mods: [], repositories: [diy], translate: keyOf,
  })
  assert.deepEqual(levelRows, rows)
})
