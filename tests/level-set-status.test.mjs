import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 关卡集状态判定（`src/levelSetStatus.ts`）。
 *
 * 这一层是从「MOD 状态」里照搬结构、但**换了身份来源**的部分：
 * MOD 靠 GUID，关卡集靠 `levelSetName + author`。所以它必须独立测 ——
 * 直接复用 modStatus 的测试会漏掉「uid 不能当身份」这类专属陷阱。
 *
 * 后半部分测的是地图管理页真正需要的那几件事：绑定哪个地图源、
 * 框架依赖缺不缺、卡片上该出现哪些标签。这些判定以前分散在页面里
 * （依赖还要后端再扫一遍 `BepInEx/plugins`），搬成纯函数后必须锁住行为。
 */
const {
  isSameLevelSet,
  levelSetState,
  levelSetDisplayName,
  mapVersionForLevelSet,
  mapVersionRefs,
  mapForLevelSet,
  levelSetForMap,
  mapRelationRepository,
  frameworkStates,
  mapDependencyMissing,
  levelSetCardTags,
  levelSetGroup,
  mapCardState,
  mapCardStatus,
  mapCardAction,
  LEVEL_SET_STATE_TAGS,
  LEVEL_SET_UNGROUPED_KEY,
  MAP_CARD_STATE_TAGS,
  MAP_CARD_STATE_TONES,
} = await import('../src/levelSetStatus.ts')

function set(overrides = {}) {
  return {
    dir_name: 'RW_LevelSet',
    enabled: true,
    name: 'RW_LevelSet',
    name_zh: 'RW_关卡合集',
    author: 'RW',
    version: '0.73',
    uid: 'e34b5584',
    level_count: 10,
    ...overrides,
  }
}

/** 地图源条目：只填判定会读的字段，和 `map_to_info` 的产物同形。 */
function map(overrides = {}) {
  return {
    id: 'map-rw',
    name: 'RW Level Set',
    provider: 'github',
    username: 'rw',
    repository: 'level-set',
    level_set_name: 'RW_LevelSet',
    author: 'RW',
    enabled: true,
    builtin: false,
    latest_tag: 'v0.73',
    cached_releases: [],
    ...overrides,
  }
}

/** 本地 MOD：关系判定只读 guid / version / prerelease。 */
function mod(overrides = {}) {
  return {
    guid: 'com.rw.diylevel',
    version: '1.1.0',
    enabled: true,
    ...overrides,
  }
}

test('身份比较用 author + levelSetName，忽略大小写与空白', () => {
  assert.ok(isSameLevelSet({ name: 'RW_LevelSet', author: 'RW' }, { name: 'rw_levelset ', author: ' rw' }))
  assert.ok(!isSameLevelSet({ name: 'RW_LevelSet', author: 'RW' }, { name: 'RW_LevelSet', author: 'Someone' }))
  assert.ok(!isSameLevelSet({ name: 'A', author: 'RW' }, { name: 'B', author: 'RW' }))
})

test('uid 随版本变化，不能当身份', () => {
  // 同一地图的两个版本：uid 不同，但必须认成同一个
  const v1 = { name: 'RW_LevelSet', author: 'RW', uid: 'aaa' }
  const v2 = { name: 'RW_LevelSet', author: 'RW', uid: 'bbb' }
  assert.ok(isSameLevelSet(v1, v2), 'uid 不同也必须认成同一地图')
})

test('停用优先于一切 —— 关掉的地图不该催用户更新', () => {
  assert.equal(levelSetState(set({ enabled: false, version: '0.1' }), {
    tag: 'v9.9.9', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'disabled')
})

test('解析失败优先于版本比较', () => {
  assert.equal(levelSetState(set({ error: 'levelSet.infoMissing' }), null), 'unreadable')
})

test('没有绑定仓库时是未绑定，不猜版本', () => {
  assert.equal(levelSetState(set(), undefined), 'unbound')
  assert.equal(levelSetState(set(), null), 'unbound')
})

test('本地版本低于 Release tag → 可更新', () => {
  assert.equal(levelSetState(set({ version: '0.73' }), {
    tag: 'v0.74', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'update_available')
})

test('版本一致 → 最新版（tag 的 v 前缀要能剥掉）', () => {
  assert.equal(levelSetState(set({ version: '0.73' }), {
    tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'latest')
  assert.equal(levelSetState(set({ version: 'v0.73' }), {
    tag: '0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'latest')
})

test('本地版本反而更高 → ahead，不显示成待更新', () => {
  // 作者的预发布版本，或仓库 tag 回退
  assert.equal(levelSetState(set({ version: '0.80' }), {
    tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'ahead')
})

test('任一侧版本缺失 → unknown，不误判为最新', () => {
  assert.equal(levelSetState(set({ version: '' }), {
    tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'unknown')
})

test('按身份找仓库版本，找不到就是未绑定', () => {
  const refs = [
    { tag: 'v1.0', levelSetName: 'Other', author: 'RW' },
    { tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW' },
  ]
  assert.equal(mapVersionForLevelSet(set(), refs)?.tag, 'v0.73')
  assert.equal(mapVersionForLevelSet(set({ name: 'Missing' }), refs), undefined)
  // 集名为空（解析失败）时不做身份匹配，避免把所有空名条目串在一起
  assert.equal(mapVersionForLevelSet(set({ name: '' }), refs), undefined)
})

test('展示名按语言三级回退', () => {
  assert.equal(levelSetDisplayName(set(), 'zh-CN'), 'RW_关卡合集')
  assert.equal(levelSetDisplayName(set(), 'en'), 'RW_LevelSet')
  // 没有中文名时英文优先
  assert.equal(levelSetDisplayName(set({ name_zh: null }), 'zh-CN'), 'RW_LevelSet')
  // 两者都空时退回目录名
  assert.equal(levelSetDisplayName(set({ name: '', name_zh: null }), 'zh-CN'), 'RW_LevelSet')
})

test('状态标签的 i18n key：解析失败与更高版本各有专属文案', () => {
  assert.equal(LEVEL_SET_STATE_TAGS.unreadable, 'levelSet.parseFailed')
  assert.equal(LEVEL_SET_STATE_TAGS.ahead, 'levelSet.higherVersion')
  assert.equal(LEVEL_SET_STATE_TAGS.update_available, 'tools.update')
  assert.equal(LEVEL_SET_STATE_TAGS.unbound, 'modsLocal.unbound')
})

/*
 * 界面层要守住的一条：**只有刷新到 Release 的地图才产生版本声明**。
 * 没刷过 Release 的地图没有 tag，此时本地关卡集必须显示「未绑定」，
 * 而不是被当成「最新版」—— 把「还没查」说成「已是最新」是错的结论，
 * 用户会因此错过真实更新。
 */
test('地图没刷新到 Release 时不产生版本声明', () => {
  const maps = [
    // 刷新成功
    map({ level_set_name: 'RW_LevelSet', author: 'RW', latest_tag: 'v0.73' }),
    // 没刷新过：latest_tag 缺失
    map({ id: 'map-other', level_set_name: 'Other_Set', author: 'RW', latest_tag: undefined }),
    // 刷新失败：latest_tag 为 null
    map({ id: 'map-third', level_set_name: 'Third_Set', author: 'RW', latest_tag: null }),
  ]
  const refs = mapVersionRefs(maps)

  assert.deepEqual(refs, [{ tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW' }])
  assert.equal(mapVersionForLevelSet({ name: 'RW_LevelSet', author: 'RW' }, refs)?.tag, 'v0.73')
  // 没刷新 → 没有声明 → 状态必须是未绑定
  assert.equal(mapVersionForLevelSet({ name: 'Other_Set', author: 'RW' }, refs), undefined)
  assert.equal(levelSetState(set({ name: 'Other_Set', version: '1.0' }),
    mapVersionForLevelSet({ name: 'Other_Set', author: 'RW' }, refs)), 'unbound')
  assert.equal(levelSetState(set({ name: 'Third_Set', version: '1.0' }),
    mapVersionForLevelSet({ name: 'Third_Set', author: 'RW' }, refs)), 'unbound')
})

test('找绑定地图：忽略大小写与空白，集名为空时不匹配', () => {
  const maps = [map(), map({ id: 'map-other', level_set_name: 'Other_Set' })]
  assert.equal(mapForLevelSet({ name: ' rw_levelset ', author: 'rw' }, maps)?.id, 'map-rw')
  assert.equal(mapForLevelSet({ name: 'Missing', author: 'RW' }, maps), undefined)
  // 解析失败的集（空名）不能和所有空名条目串在一起
  assert.equal(mapForLevelSet({ name: '', author: '' }, [map({ level_set_name: '', author: '' })]), undefined)
})

// ---------------------------------------------------------------------------
// 管理页：分组（作者）与折叠键
// ---------------------------------------------------------------------------

test('分组：绑定了地图时用线上声明的分组，键与展示名分开', () => {
  const bound = map({ group_i18n: { 'zh-CN': 'RW 的地图', en: 'RW Maps' } })
  const group = levelSetGroup(set(), [bound])
  // 规范名取 zh-CN：折叠键必须与语言无关，切语言不能重建键
  assert.equal(group.canonical, 'RW 的地图')
  assert.equal(group.key, 'group:rw 的地图')
  // 展示名交给页面的 pickI18n，所以必须把整张多语言表交出去
  assert.deepEqual(group.labelI18n, { 'zh-CN': 'RW 的地图', en: 'RW Maps' })
})

test('分组：线上没有分组时回退本地 info 的 author', () => {
  // map() 默认没有 group / group_i18n（源文档里没声明作者分组）
  const group = levelSetGroup(set(), [map()])
  assert.equal(group.canonical, 'RW')
  assert.equal(group.key, 'group:rw')
  // 作者名不是翻译：labelI18n 为空，页面直接用 canonical 展示
  assert.deepEqual(group.labelI18n, {})
})

test('分组：未绑定地图（本地 ZIP 导入）也用 author', () => {
  const group = levelSetGroup(set({ author: '  Chef ' }), [])
  assert.equal(group.canonical, 'Chef')
  assert.equal(group.key, 'group:chef')
  assert.deepEqual(group.labelI18n, {})
})

test('分组：没有线上分组、author 也空 → 未分组', () => {
  assert.equal(LEVEL_SET_UNGROUPED_KEY, '__ungrouped__')
  // info 解析失败（集名为空）绑定不上任何地图，作者也是空：只能是未分组
  const group = levelSetGroup(
    set({ name: '', author: '   ' }),
    [map({ level_set_name: '', author: '' })],
  )
  assert.equal(group.key, LEVEL_SET_UNGROUPED_KEY)
  assert.equal(group.canonical, '')
  assert.deepEqual(group.labelI18n, {})
})

test('分组：大小写不同归一成同一个折叠键', () => {
  // 作者在不同工具里导出时大小写常不一致；键必须相同，
  // 否则同一个作者会被拆成两个折叠项
  const upper = levelSetGroup(set({ author: 'RW' }), [])
  const lower = levelSetGroup(set({ author: 'rw' }), [])
  assert.equal(upper.key, lower.key)
  // 展示名保留原始大小写（组标题取该组第一个成员的 canonical）
  assert.equal(upper.canonical, 'RW')
})

test('分组：group_i18n 缺失但有单语 group 镜像时用 group', () => {
  // 手写 / 旧源文档可能只写单语 group；后端读取时会折进 zh-CN，
  // 但判定本身也要认这条，不能因为缺多语言表就退到 author
  const group = levelSetGroup(set(), [map({ group: '街机 MOD' })])
  assert.equal(group.canonical, '街机 MOD')
  assert.equal(group.key, 'group:街机 mod')
  assert.deepEqual(group.labelI18n, {})
})

test('地图投影成 Repository 时把 required_frameworks 变成 requires 依赖', () => {
  const repo = mapRelationRepository(map({
    required_frameworks: [
      { guid: 'com.rw.diylevel', min_version: '1.0.0' },
      { guid: 'com.rw.diylevel.web', min_version: '' },
    ],
  }))
  assert.equal(repo.id, 'map-rw')
  assert.deepEqual(repo.relations, [
    { kind: 'requires', guid: 'com.rw.diylevel', min_version: '1.0.0', max_version: '' },
    { kind: 'requires', guid: 'com.rw.diylevel.web', min_version: '', max_version: '' },
  ])
})

test('没有 cached_releases 时用 latest_release 兜底，保持仓库同形', () => {
  const latest = { tag_name: 'v0.73', name: 'v0.73', body: '', prerelease: false, draft: false, assets: [] }
  const repo = mapRelationRepository(map({ cached_releases: undefined, latest_release: latest }))
  assert.deepEqual(repo.cached_releases, [latest])
  assert.equal(repo.download_mode, 'release')
  assert.equal(repo.install_target, '')
})

test('框架依赖：装了且在区间内才算满足', () => {
  const target = map({ required_frameworks: [{ guid: 'com.rw.diylevel', min_version: '1.0.0' }] })
  const ok = frameworkStates(target, [mod({ version: '1.1.0' })])
  assert.deepEqual(ok, [{
    guid: 'com.rw.diylevel',
    minVersion: '1.0.0',
    installedVersion: '1.1.0',
    satisfied: true,
  }])

  // 版本低于依赖要求：装了也不算满足，但要保留已装版本（界面要说明）
  const old = frameworkStates(target, [mod({ version: '0.9.0' })])
  assert.equal(old[0].satisfied, false)
  assert.equal(old[0].installedVersion, '0.9.0')

  // 完全没装
  const none = frameworkStates(target, [])
  assert.equal(none[0].satisfied, false)
  assert.equal(none[0].installedVersion, null)
})

test('框架依赖：GUID 比较忽略大小写', () => {
  const target = map({ required_frameworks: [{ guid: 'Com.RW.DIYLevel' }] })
  const states = frameworkStates(target, [mod({ guid: 'com.rw.diylevel' })])
  assert.equal(states[0].satisfied, true)
})

test('依赖缺失：不知道游戏状态时不判缺失', () => {
  const target = map({ required_frameworks: [{ guid: 'com.rw.diylevel' }] })
  // 游戏未探测（game.path 为空）：本地 MOD 列表必然为空，不能说「缺」
  assert.equal(mapDependencyMissing(target, [], false), null)
  // 游戏已探测但一张地图都没有：不算缺依赖
  assert.equal(mapDependencyMissing(undefined, [], true), false)
  // 已探测且确实缺
  assert.equal(mapDependencyMissing(target, [], true), true)
  assert.equal(mapDependencyMissing(target, [mod()], true), false)
})

test('DIYLevel 与 DIYLevel Web Runtime 任一缺失都算依赖缺失', () => {
  const target = map({
    required_frameworks: [
      { guid: 'com.rw.diylevel' },
      { guid: 'com.rw.diylevel.web' },
    ],
  })
  assert.equal(mapDependencyMissing(target, [mod()], true), true)
  assert.equal(mapDependencyMissing(target, [mod(), mod({ guid: 'com.rw.diylevel.web' })], true), false)
})

test('卡片标签：先依赖提醒，再版本结论', () => {
  const target = map({
    required_frameworks: [{ guid: 'com.rw.diylevel' }],
    latest_tag: 'v0.74',
  })
  const tags = levelSetCardTags(set({ version: '0.73' }), [target], [], true)
  assert.deepEqual(tags.map((tag) => tag.text), ['levelSet.dependencyMissing', 'tools.update'])
  assert.equal(tags[0].kind, 'dependency')
  assert.equal(tags[0].tone, 'critical')
  // 「有更新」带着绑定地图 id：点下去要落到下载页那张卡
  assert.equal(tags[1].kind, 'state')
  assert.equal(tags[1].mapId, 'map-rw')
})

test('卡片标签：最新版也带跳转目标（与 MOD 管理页一致），未绑定没有目标', () => {
  const latest = levelSetCardTags(set(), [map({ latest_tag: 'v0.73' })], [], true)
  assert.deepEqual(latest.map((tag) => tag.text), ['modsLocal.latest'])
  // 「最新版」也可以点：跳到下载页看那张卡，与 MOD 管理页的「最新版」一致
  assert.equal(latest[0].mapId, 'map-rw')
  assert.equal(latest[0].tone, 'accent')

  const unbound = levelSetCardTags(set({ name: 'Local_Only' }), [map()], [], true)
  assert.deepEqual(unbound.map((tag) => tag.text), ['modsLocal.unbound'])
  assert.equal(unbound[0].mapId, undefined)
})

test('卡片标签：游戏未探测时不显示依赖缺失，但版本结论照旧', () => {
  const target = map({
    required_frameworks: [{ guid: 'com.rw.diylevel' }],
    latest_tag: 'v0.74',
  })
  const tags = levelSetCardTags(set({ version: '0.73' }), [target], [], false)
  assert.deepEqual(tags.map((tag) => tag.text), ['tools.update'])
})

test('卡片标签：停用且缺依赖时两条都显示（重新启用前就该知道）', () => {
  const target = map({ required_frameworks: [{ guid: 'com.rw.diylevel' }] })
  const tags = levelSetCardTags(set({ enabled: false }), [target], [], true)
  assert.deepEqual(tags.map((tag) => tag.text), ['levelSet.dependencyMissing', 'modsLocal.disabled'])
})

// ---------------------------------------------------------------------------
// 下载页：地图 → 本地关卡集的反向绑定
// ---------------------------------------------------------------------------

test('反向绑定：从地图找到本地关卡集，身份规则与正向一致', () => {
  assert.equal(levelSetForMap(map(), [set()])?.dir_name, 'RW_LevelSet')
  // 大小写与首尾空白不敏感（作者在不同工具里导出不一致）
  assert.equal(
    levelSetForMap(map({ author: ' rw ' }), [set({ author: 'RW' })])?.dir_name,
    'RW_LevelSet',
  )
  // 集名不同 = 不是同一张图
  assert.equal(levelSetForMap(map({ level_set_name: 'Other_Set' }), [set()]), undefined)
  // 作者不同 = 不是同一张图（同名不同作者的地图确实存在）
  assert.equal(levelSetForMap(map({ author: 'someone' }), [set()]), undefined)
  // 地图声明的地图名为空（解析失败）不能认领任意空名集
  assert.equal(levelSetForMap(map({ level_set_name: '  ' }), [set({ name: '' })]), undefined)
})

// ---------------------------------------------------------------------------
// 下载页：卡片状态与动作（对齐 MOD 安装页）
// ---------------------------------------------------------------------------

test('下载页状态：未安装 / 已安装 / 有更新 / 最新版', () => {
  // 线上有 Release，本地没有 → 未安装
  assert.equal(mapCardState(map(), []), 'not_installed')
  // 本地有，线上还没刷到 Release → 只说「已安装」，不说「最新版」
  assert.equal(mapCardState(map({ latest_tag: undefined }), [set()]), 'installed')
  // 本地比线上低 → 有更新
  assert.equal(mapCardState(map({ latest_tag: 'v0.74' }), [set({ version: '0.73' })]), 'update_available')
  // 一致 → 最新版
  assert.equal(mapCardState(map({ latest_tag: 'v0.73' }), [set({ version: '0.73' })]), 'latest')
  // 本地更高（作者预发布）也归到最新版：下载页不催他重装一个更新的版本
  // 注意版本段按数值比较：0.80 > 0.73（而 0.9 其实是 0.09 那一档，不是更大）
  assert.equal(mapCardState(map({ latest_tag: 'v0.73' }), [set({ version: '0.80' })]), 'latest')
})

test('下载页状态：无法检测更新优先于任何版本结论', () => {
  // 刷新失败：缓存不可信，哪怕缓存里有 tag 也不能说「最新版」
  assert.equal(
    mapCardState(map({ last_error: 'boom', latest_tag: 'v0.73' }), [set({ version: '0.73' })]),
    'cannot_check',
  )
  // 版本字符串不可比（info 解析失败后 version 为空）
  assert.equal(
    mapCardState(map({ latest_tag: 'v0.74' }), [set({ version: '' })]),
    'cannot_check',
  )
  // 本地没装、线上也没刷到 → 还没查到，不是「未安装」
  assert.equal(mapCardState(map({ latest_tag: undefined }), []), 'not_refreshed')
})

test('下载页状态：文案与色调复用 MOD 侧 key', () => {
  const status = mapCardStatus(map({ latest_tag: 'v0.74' }), [set({ version: '0.73' })])
  assert.equal(status.key, 'update_available')
  assert.equal(status.text, 'tools.update')
  assert.equal(status.tone, 'moderate')

  const notInstalled = mapCardStatus(map(), [])
  assert.equal(notInstalled.text, 'overview.notInstalled')
  assert.equal(notInstalled.tone, 'muted')

  // 每个状态都有文案与色调，避免新增状态时漏配
  const states = ['not_installed', 'installed', 'update_available', 'latest', 'cannot_check', 'not_refreshed']
  for (const key of states) {
    assert.equal(typeof MAP_CARD_STATE_TAGS[key], 'string', `${key} 缺少文案`)
    assert.match(MAP_CARD_STATE_TAGS[key], /^(overview|modsLocal|modsOnline|tools|levelSet)\./)
    assert.equal(typeof MAP_CARD_STATE_TONES[key], 'string', `${key} 缺色调`)
  }
})

test('下载页动作：无 Release 只能刷新，已装同版本是重装', () => {
  // 没有 Release 无从安装
  assert.equal(mapCardAction(map({ latest_tag: undefined }), []), 'refresh')
  // 刷新失败也只能刷新
  assert.equal(mapCardAction(map({ last_error: 'boom' }), [set()]), 'refresh')
  // 未安装 → 安装
  assert.equal(mapCardAction(map(), []), 'install')
  // 已装同版本 → 重装（点「安装」会让用户以为会多装一份）
  assert.equal(mapCardAction(map(), [set()]), 'reinstall')
  // 本地低 → 更新
  assert.equal(mapCardAction(map({ latest_tag: 'v0.74' }), [set({ version: '0.73' })]), 'update')
  // 本地更高 / 版本不可比 → 重装
  assert.equal(mapCardAction(map(), [set({ version: '0.80' })]), 'reinstall')
  assert.equal(mapCardAction(map({ latest_tag: 'v0.74' }), [set({ version: '' })]), 'reinstall')
})
