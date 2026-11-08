import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 关卡集状态判定（`src/levelSetStatus.ts`）。
 *
 * 这一层是从「MOD 状态」里照搬结构、但**换了身份来源**的部分：
 * MOD 靠 GUID，关卡集靠 `levelSetName + author`。所以它必须独立测 ——
 * 直接复用 modStatus 的测试会漏掉「uid 不能当身份」这类专属陷阱。
 *
 * 这里覆盖地图管理页的绑定、依赖和标签判定；在线地图下载卡片的状态、展示
 * 与动作投影由 `src/mapCardStatus.ts` 单独持有，但复用本模块的身份和版本规则。
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
  levelSetAuthorNames,
  mapUpdateState,
  LEVEL_SET_STATE_TAGS,
  LEVEL_SET_UNGROUPED_KEY,
} = await import('../src/levelSetStatus.ts')
const {
  mapCardState,
  mapCardStatus,
  mapCardAction,
  MAP_CARD_STATE_TAGS,
  MAP_CARD_STATE_TONES,
} = await import('../src/mapCardStatus.ts')
const {
  levelSetStatusKeys,
  summarizeLevelSetTags,
  summarizeLevelSetToggleStatus,
} = await import('../src/levelSetStatusSummary.ts')

function set(overrides = {}) {
  return {
    dir_name: 'RW_LevelSet',
    path: 'C:/OC2/BepInEx/plugins/OC2DIYLevel/levels/RW_LevelSet',
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

/**
 * 可安装来源：声明同一个 GUID、Release 模式、且有一个满足区间的 Release。
 *
 * 三个条件缺一，`installSourceFor` 都会判成「没有来源」，所以夹具里都要给。
 */
function repo(overrides = {}) {
  return {
    id: 'diy',
    name: 'DIYLevel',
    provider: 'github',
    username: 'gua',
    repository: 'diylevel',
    plugin_guid: 'com.rw.diylevel',
    download_mode: 'release',
    channel: 'all',
    enabled: true,
    builtin: false,
    cached_releases: [{
      tag_name: 'v1.2.0',
      version: '1.2.0',
      draft: false,
      prerelease: false,
      assets: [],
    }],
    ...overrides,
  }
}

test('页头状态 key 区分标签类型并附带开关状态', () => {
  assert.deepEqual(levelSetStatusKeys(
    set({ enabled: false }),
    [{ kind: 'state', text: 'tools.update', tone: 'moderate' }],
  ), ['tag:state:tools.update', 'disabled'])
})

test('页头结论摘要依赖优先、按数量排序且不重复统计禁用标签', () => {
  const first = set()
  const second = set({ dir_name: 'Second', enabled: false })
  const summaries = summarizeLevelSetTags([first, second], (item) => item === first
    ? [
        { kind: 'dependency', text: 'modsOnline.missingDep', tone: 'critical' },
        { kind: 'state', text: 'modsLocal.latest', tone: 'accent' },
        { kind: 'state', text: 'modsLocal.disabled', tone: 'muted' },
      ]
    : [
        { kind: 'dependency', text: 'modsOnline.missingDep', tone: 'critical' },
        { kind: 'state', text: 'tools.update', tone: 'moderate' },
      ])

  assert.deepEqual(summaries, [
    { key: 'tag:dependency:modsOnline.missingDep', text: 'modsOnline.missingDep', tone: 'critical', count: 2 },
    { key: 'tag:state:modsLocal.latest', text: 'modsLocal.latest', tone: 'accent', count: 1 },
    { key: 'tag:state:tools.update', text: 'tools.update', tone: 'moderate', count: 1 },
  ])
})

test('页头开关摘要固定启用在前并省略数量为零的状态', () => {
  assert.deepEqual(summarizeLevelSetToggleStatus([
    set({ enabled: false }),
    set({ dir_name: 'Second', enabled: true }),
    set({ dir_name: 'Third', enabled: true }),
  ]), [
    { key: 'enabled', text: 'modsLocal.enabled2', tone: 'accent', count: 2 },
    { key: 'disabled', text: 'modsLocal.disabled', tone: 'muted', count: 1 },
  ])
  assert.deepEqual(summarizeLevelSetToggleStatus([set({ enabled: true })]), [
    { key: 'enabled', text: 'modsLocal.enabled2', tone: 'accent', count: 1 },
  ])
})

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

test('分组：地图源被删后只剩快照分组时仍按快照分组', () => {
  // `map_group_i18n` 由后端从地图依赖快照（manifests/map-relations.json）补进来：
  // 线上条目已经没了，本地装着的这张图不该掉回裸作者名
  const group = levelSetGroup(
    set({ map_group_i18n: { 'zh-CN': 'RW 的地图', en: 'RW Maps' } }),
    [map()],
  )
  assert.equal(group.canonical, 'RW 的地图')
  assert.equal(group.key, 'group:rw 的地图')
  assert.deepEqual(group.labelI18n, { 'zh-CN': 'RW 的地图', en: 'RW Maps' })
})

test('分组：只声明英文分组时用英文，不退成作者名', () => {
  // 英文覆盖率最高、中文只是第二保底：作者只补了英文名时，分组标题
  // 该显示英文名，而不是因为「没有 zh-CN」就掉回 info 的 author
  const group = levelSetGroup(set(), [map({ group_i18n: { en: 'RW Maps' } })])
  assert.equal(group.canonical, 'RW Maps')
  assert.equal(group.key, 'group:rw maps')
  assert.deepEqual(group.labelI18n, { en: 'RW Maps' })
})

// ---------------------------------------------------------------------------
// 管理页详情：作者的中英展示名
// ---------------------------------------------------------------------------

test('作者名：绑定地图声明了双语时中英不同', () => {
  const names = levelSetAuthorNames(set(), [
    map({ group_i18n: { 'zh-CN': 'RW 的地图', en: 'RW Maps' } }),
  ])
  assert.deepEqual(names, { zh: 'RW 的地图', en: 'RW Maps' })
})

test('作者名：只有单语 group 时两行同一串', () => {
  const names = levelSetAuthorNames(set(), [map({ group: '街机 MOD' })])
  assert.deepEqual(names, { zh: '街机 MOD', en: '街机 MOD' })
})

test('作者名：只声明一种语言时两行同一串，不留空', () => {
  // 源文档常只写 zh-CN（手工维护，很少有人补英文）；留空会让详情里
  // 出现「—」，看着像作者数据丢了
  const names = levelSetAuthorNames(set(), [
    map({ group_i18n: { 'zh-CN': 'RW 的地图' } }),
  ])
  assert.deepEqual(names, { zh: 'RW 的地图', en: 'RW 的地图' })
})

test('作者名：快照分组参与中英两名（源被删后仍显示作者双语）', () => {
  const names = levelSetAuthorNames(
    set({ map_group_i18n: { 'zh-CN': 'RW 的地图', en: 'RW Maps' } }),
    [map()],
  )
  assert.deepEqual(names, { zh: 'RW 的地图', en: 'RW Maps' })
})

test('作者名：只声明英文时中文行也用英文串，不留空', () => {
  const names = levelSetAuthorNames(set(), [
    map({ group_i18n: { en: 'RW Maps' } }),
  ])
  assert.deepEqual(names, { zh: 'RW Maps', en: 'RW Maps' })
})

test('作者名：未绑定地图（本地 ZIP 导入）回退 info 的 author', () => {
  const names = levelSetAuthorNames(set({ author: '  Chef ' }), [])
  assert.deepEqual(names, { zh: 'Chef', en: 'Chef' })
})

test('作者名：线上与本地都为空时两行都空（视图显示 —）', () => {
  // info 解析失败（集名为空）绑定不上任何地图，作者也为空
  const names = levelSetAuthorNames(
    set({ name: '', author: '   ' }),
    [map({ level_set_name: '', author: '' })],
  )
  assert.deepEqual(names, { zh: '', en: '' })
})

test('地图投影成 Repository 时把 required_frameworks 变成 requires 依赖', () => {
  const repo = mapRelationRepository(map({
    required_frameworks: [
      { guid: 'com.rw.diylevel', version: '>=1.0.0' },
      { guid: 'com.rw.diylevel.web', version: '' },
    ],
  }))
  assert.equal(repo.id, 'map-rw')
  assert.deepEqual(repo.relations, [
    { kind: 'requires', guid: 'com.rw.diylevel', version: '>=1.0.0' },
    { kind: 'requires', guid: 'com.rw.diylevel.web', version: '' },
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
  const target = map({ required_frameworks: [{ guid: 'com.rw.diylevel', version: '>=1.0.0' }] })
  const ok = frameworkStates(target, [mod({ version: '1.1.0' })])
  assert.deepEqual(ok, [{
    guid: 'com.rw.diylevel',
    version: '>=1.0.0',
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
  // 文案与 MOD 卡片是同一枚 key（「缺依赖」三个字），不再自造「依赖缺失」
  assert.deepEqual(tags.map((tag) => tag.text), ['modsOnline.missingDep', 'tools.update'])
  assert.equal(tags[0].kind, 'dependency')
  assert.equal(tags[0].tone, 'critical')
  // 「有更新」带着绑定地图 id：点下去要落到下载页那张卡
  assert.equal(tags[1].kind, 'state')
  assert.equal(tags[1].mapId, 'map-rw')
})

test('卡片标签：必须依赖缺了就写「缺依赖」，线上有候选仓库时可点', () => {
  const target = map({ required_frameworks: [{ guid: 'com.rw.diylevel' }] })
  const diy = repo({ id: 'diy', plugin_guid: 'com.rw.diylevel' })
  const tags = levelSetCardTags(set(), [target], [], true, [diy])
  assert.equal(tags[0].kind, 'dependency')
  // `requires` 是必须依赖：缺了就是缺了，线上能装只是让标签可点、可跳过去补装
  assert.equal(tags[0].text, 'modsOnline.missingDep')
  assert.equal(tags[0].tone, 'critical')
  // 有候选仓库 → 标签可点，落点是提供依赖的仓库卡，而不是这张图自己
  assert.equal(tags[0].repositoryId, 'diy')
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
  assert.deepEqual(tags.map((tag) => tag.text), ['modsOnline.missingDep', 'modsLocal.disabled'])
})

test('卡片标签：绑定地图（或它所在的地图源）在刷新时显示「刷新中」，不再显示旧结论', () => {
  const target = map({ latest_tag: 'v0.74', source_id: 'rw' })

  // 这张图自己正在重抓 Release
  const mapRefreshing = levelSetCardTags(
    set({ version: '0.73' }), [target], [], true, [],
    { maps: new Set(['map-rw']), sources: new Set() },
  )
  assert.deepEqual(mapRefreshing.map((tag) => tag.text), ['tools.refreshing'])
  assert.equal(mapRefreshing[0].kind, 'state')
  assert.equal(mapRefreshing[0].tone, 'muted')
  // 与版本结论互斥：不能一边说「刷新中」一边还挂着过期的「有更新」
  assert.equal(mapRefreshing[0].mapId, 'map-rw')

  // 源文档在重拉：此刻一个 Release 都还没回来，也不能显示成「未绑定」
  const sourceRefreshing = levelSetCardTags(
    set({ version: '0.73' }), [map({ source_id: 'rw', latest_tag: undefined })], [], true, [],
    { maps: new Set(), sources: new Set(['rw']) },
  )
  assert.deepEqual(sourceRefreshing.map((tag) => tag.text), ['tools.refreshing'])

  // 整源刷新：共享列表被清空重建，绑定暂时查不到 —— 也不能显示「未绑定」
  const allRefreshing = levelSetCardTags(
    set({ version: '0.73' }), [], [], true, [],
    { entryStateUnavailable: true, maps: new Set(), sources: new Set() },
  )
  assert.deepEqual(allRefreshing.map((tag) => tag.text), ['tools.refreshing'])

  // 没在刷新时仍是正常结论（别把「刷新中」当成默认态）
  const idle = levelSetCardTags(
    set({ version: '0.73' }), [target], [], true, [],
    { maps: new Set(), sources: new Set() },
  )
  assert.deepEqual(idle.map((tag) => tag.text), ['tools.update'])
})

test('卡片标签：不传刷新提示时行为与旧版一致（向后兼容）', () => {
  const tags = levelSetCardTags(set({ version: '0.73' }), [map({ latest_tag: 'v0.73' })], [], true)
  assert.deepEqual(tags.map((tag) => tag.text), ['modsLocal.latest'])
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

// ---------------------------------------------------------------------------
// 源文档钉死版本（`[[maps]].latest_mapset_version`）：只比等值，不比大小
// ---------------------------------------------------------------------------

/** 一张钉死了最新版本的地图条目：tag 只是资产所在位置，版本由 latest_mapset_version 决定。 */
const pinnedMap = () => map({ latest_mapset_version: '0.70' })

/** 与 `pinnedMap` 对应的版本声明。 */
const pinnedRef = () => ({
  tag: 'v1.0',
  latestMapsetVersion: '0.70',
  levelSetName: 'RW_LevelSet',
  author: 'RW',
})

test('钉死版本：本地更高也提示更新 —— 作者可能就是要用户回退', () => {
  // 普通规则下 0.80 > 0.70 会判 ahead（不催更新）；钉死版本时结论必须反过来
  assert.equal(levelSetState(set({ version: '0.80' }), pinnedRef()), 'update_available')
  // 本地更低同理
  assert.equal(levelSetState(set({ version: '0.69' }), pinnedRef()), 'update_available')
})

test('钉死版本：只有同一版才算最新（v 前缀与多余尾段算同一版）', () => {
  assert.equal(levelSetState(set({ version: '0.70' }), pinnedRef()), 'latest')
  // info 与资产名的写法常不一致，等值仍走版本解析而不是字符串直比
  assert.equal(levelSetState(set({ version: 'v0.70' }), pinnedRef()), 'latest')
  assert.equal(levelSetState(set({ version: '0.70.0' }), pinnedRef()), 'latest')
})

test('钉死版本：本地版本读不出来时算有更新', () => {
  // 没装上指定版本就是没到位；这里不能说「无法检测更新」把问题糊过去
  assert.equal(levelSetState(set({ version: '' }), pinnedRef()), 'update_available')
  assert.equal(mapCardState(pinnedMap(), [set({ version: '' })]), 'update_available')
  assert.equal(mapCardAction(pinnedMap(), [set({ version: '' })]), 'update')
})

test('钉死版本为空时回到按大小比较（回归保护）', () => {
  const unforced = { tag: 'v0.73', latestMapsetVersion: '', levelSetName: 'RW_LevelSet', author: 'RW' }
  assert.equal(levelSetState(set({ version: '0.80' }), unforced), 'ahead')
  assert.equal(levelSetState(set({ version: '0.73' }), unforced), 'latest')
  // null 与「老地图源没有这个字段」同义
  assert.equal(levelSetState(set({ version: '0.80' }), { ...unforced, latestMapsetVersion: null }), 'ahead')
  assert.equal(
    levelSetState(set({ version: '0.80' }), {
      tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
    }),
    'ahead',
  )
})

test('版本声明带上钉死的版本，管理页与下载页共用同一结论', () => {
  const bound = pinnedMap()
  const refs = mapVersionRefs([bound])
  assert.equal(refs[0].latestMapsetVersion, '0.70')
  // 管理页（关卡集状态）与下载页（卡片状态 / 按钮）都走 mapUpdateState
  assert.equal(
    levelSetState(set({ version: '0.80' }), mapVersionForLevelSet(set(), refs)),
    'update_available',
  )
  assert.equal(mapCardState(bound, [set({ version: '0.80' })]), 'update_available')
  assert.equal(mapCardAction(bound, [set({ version: '0.80' })]), 'update')
})

test('钉死版本不能替代「已刷新到 Release」这个前提', () => {
  // 没有 Release 就没有可安装的资产：声明仍不产生，避免把「还没查」说成结论
  assert.deepEqual(mapVersionRefs([map({ latest_tag: undefined, latest_mapset_version: '0.70' })]), [])
})

test('mapUpdateState：没有声明时比不出结论', () => {
  assert.equal(mapUpdateState('0.70', undefined), 'unknown')
  assert.equal(mapUpdateState('0.70', null), 'unknown')
  // 普通模式下版本不可解析仍是 unknown（不把「不知道」说成「有更新」）
  assert.equal(mapUpdateState('', { tag: 'v0.70' }), 'unknown')
})

// ---------------------------------------------------------------------------
// info 版本优先（`latest_info_version`）
//
// `latest_mapset_version` 与 Release tag 描述的是**地图包**版本，本地扫出来的
// 却是 info 里写的版本号。作者升了包版本（tag 0.7）却忘了改 info 版本（还是
// 0.69）时，按包版本比会永远催用户更新一个已经是最新的地图；有了最新 Release
// 的 info 版本，同一套版本空间直接比就能认出「装的已是最新」。
// ---------------------------------------------------------------------------

/** 声明了「最新 Release 的 info 版本」的地图条目。 */
const infoVersionMap = (infoVersion, overrides = {}) =>
  map({ latest_tag: 'v0.7', latest_info_version: infoVersion, ...overrides })

/** 与 `infoVersionMap` 对应的版本声明。 */
const infoVersionRef = (infoVersion, overrides = {}) => ({
  tag: 'v0.7',
  latestInfoVersion: infoVersion,
  levelSetName: 'RW_LevelSet',
  author: 'RW',
  ...overrides,
})

test('info 版本相等：本地 info 版本更低也判最新版，不再催更新', () => {
  // 作者发了 tag 0.7，却忘了改 info 里的版本号（还是 0.69）
  const local = set({ version: '0.69' })
  assert.equal(mapUpdateState('0.69', infoVersionRef('0.69')), 'latest')
  // 没有 info 版本时，按数字段比 0.69 > 0.7 会判成 ahead（手写版本号的坑）
  assert.equal(mapUpdateState('0.69', { tag: 'v0.7' }), 'ahead')
  // 管理页与下载页必须同一个结论
  assert.equal(levelSetState(local, infoVersionRef('0.69')), 'latest')
  assert.equal(mapCardState(infoVersionMap('0.69'), [local]), 'latest')
  // 按钮是幂等的「重装」，不是「更新」
  assert.equal(mapCardAction(infoVersionMap('0.69'), [local]), 'reinstall')
})

test('info 版本更高：本地 info 版本确实落后才提示更新', () => {
  const local = set({ version: '0.68' })
  assert.equal(mapUpdateState('0.68', infoVersionRef('0.69')), 'update_available')
  assert.equal(levelSetState(local, infoVersionRef('0.69')), 'update_available')
  assert.equal(mapCardState(infoVersionMap('0.69'), [local]), 'update_available')
  assert.equal(mapCardAction(infoVersionMap('0.69'), [local]), 'update')
})

test('本地 info 版本比线上 info 版本更高 → ahead，不催更新', () => {
  assert.equal(mapUpdateState('0.70', infoVersionRef('0.69')), 'ahead')
  assert.equal(levelSetState(set({ version: '0.70' }), infoVersionRef('0.69')), 'ahead')
})

test('没有 info 版本时回到包版本逻辑，且 info 版本优先于包版本', () => {
  // 钉死的地图包版本：只比等值
  assert.equal(mapUpdateState('0.70', { tag: 'v1.0', latestMapsetVersion: '0.70' }), 'latest')
  assert.equal(mapUpdateState('0.69', { tag: 'v1.0', latestMapsetVersion: '0.70' }), 'update_available')
  // 没有钉死版本：按 tag 大小比
  assert.equal(mapUpdateState('0.73', { tag: 'v0.73' }), 'latest')
  assert.equal(mapUpdateState('0.72', { tag: 'v0.73' }), 'update_available')
  assert.equal(mapUpdateState('0.74', { tag: 'v0.73' }), 'ahead')
  // 两个都声明时 info 版本优先：按包版本 0.70 会催更新，按 info 版本 0.69 已是最新
  const both = {
    tag: 'v1.0',
    latestMapsetVersion: '0.70',
    latestInfoVersion: '0.69',
    levelSetName: 'RW_LevelSet',
    author: 'RW',
  }
  assert.equal(mapUpdateState('0.69', both), 'latest')
  assert.equal(levelSetState(set({ version: '0.69' }), both), 'latest')
})

test('mapVersionRefs 带出最新 Release 的 info 版本，空值不塞', () => {
  const refs = mapVersionRefs([
    map({ latest_tag: 'v0.7', latest_mapset_version: '0.7', latest_info_version: ' 0.69 ' }),
  ])
  assert.equal(refs[0].latestMapsetVersion, '0.7')
  assert.equal(refs[0].latestInfoVersion, '0.69')
  // 端到端：管理页从声明里拿 info 版本，判「最新版」
  assert.equal(
    levelSetState(set({ version: '0.69' }), mapVersionForLevelSet(set(), refs)),
    'latest',
  )
  // 空白 / 缺失都不产生字段（与 nameZh、latestMapsetVersion 同一写法）
  assert.equal('latestInfoVersion' in mapVersionRefs([map({ latest_info_version: '   ' })])[0], false)
  assert.deepEqual(
    mapVersionRefs([map({ latest_tag: 'v0.7' })]),
    [{ tag: 'v0.7', levelSetName: 'RW_LevelSet', author: 'RW' }],
  )
})

// ---------------------------------------------------------------------------
// 资产版本（`installed_mapset_version`）
//
// 作者发修复版时常忘记改 info 版本号：`3.4.1-fix` 与 `3.4.1` 包里的 info 版本
// 都是 `3.4.1`，但 `-fix` 确实是新版。只看 info 版本会把装了 `3.4.1` 的用户
// 判成「最新」，永远收不到修复版。安装/更新时记下当时的资产版本，判定先比它。
// ---------------------------------------------------------------------------

/** 线上声明：最新资产版本是 `3.4.1-fix`，info 版本仍是 `3.4.1`。 */
const fixRef = {
  tag: 'v3.4.1-fix',
  latestMapsetVersion: '3.4.1-fix',
  latestInfoVersion: '3.4.1',
  levelSetName: 'RW_LevelSet',
  author: 'RW',
}

test('装了 3.4.1 的用户能收到 3.4.1-fix 的更新', () => {
  // info 版本相同（都是 3.4.1），只比 info 会判 latest —— 这正是要修的问题
  assert.equal(mapUpdateState('3.4.1', fixRef), 'latest')
  // 有资产版本记录后按资产版本判：3.4.1 ≠ 3.4.1-fix
  assert.equal(mapUpdateState('3.4.1', fixRef, '3.4.1'), 'update_available')
  assert.equal(mapUpdateState('3.4.1', fixRef, ' 3.4.1-FIX '), 'latest')
})

test('装了修复版就不再提示更新', () => {
  assert.equal(mapUpdateState('3.4.1', fixRef, '3.4.1-fix'), 'latest')
  // 不比大小：`3.4.1-fix` 在 semver 里是 3.4.1 的预发布，比大小会判成 ahead
  assert.equal(mapUpdateState('3.4.1', fixRef, '3.4.1'), 'update_available')
})

test('资产版本比较归一化 v 前缀和补零，避免 1.5.1 误报更新', () => {
  const ref = { ...fixRef, latestMapsetVersion: '1.5.1' }
  assert.equal(mapUpdateState('1.5.1', ref, 'v1.5.1'), 'latest')
  assert.equal(mapUpdateState('1.5.1', ref, '1.5.1.0'), 'latest')
  assert.equal(mapUpdateState('1.5.1', ref, '1.5.2'), 'update_available')
})

test('没有安装记录时退回 info 版本比较，不凭空报更新', () => {
  // 老版本装的地图没有记录（空串 / undefined / null 三种写法都要走老路径）
  assert.equal(mapUpdateState('3.4.1', fixRef, ''), 'latest')
  assert.equal(mapUpdateState('3.4.1', fixRef, undefined), 'latest')
  assert.equal(mapUpdateState('3.4.1', fixRef, null), 'latest')
  // 线上没有钉死版本时也走老路径（本地记录帮不上忙，只能比 tag）
  assert.equal(mapUpdateState('3.4.1', { tag: 'v3.4.2' }, '3.4.1'), 'update_available')
})

test('管理页、下载页卡片与按钮都用资产版本判定', () => {
  const bound = map({
    latest_tag: 'v3.4.1-fix',
    latest_mapset_version: '3.4.1-fix',
    latest_info_version: '3.4.1',
  })
  const installed = set({ version: '3.4.1', installed_mapset_version: '3.4.1' })
  assert.equal(levelSetState(installed, mapVersionForLevelSet(installed, mapVersionRefs([bound]))), 'update_available')
  assert.equal(mapCardState(bound, [installed]), 'update_available')
  assert.equal(mapCardAction(bound, [installed]), 'update')
  // 装了修复版：三处都判最新 / 重装
  const fixed = set({ version: '3.4.1', installed_mapset_version: '3.4.1-fix' })
  assert.equal(levelSetState(fixed, mapVersionForLevelSet(fixed, mapVersionRefs([bound]))), 'latest')
  assert.equal(mapCardState(bound, [fixed]), 'latest')
  assert.equal(mapCardAction(bound, [fixed]), 'reinstall')
})
