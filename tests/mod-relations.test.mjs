import assert from 'node:assert/strict'
import test from 'node:test'

/** 关系判定：真实源码直接执行（纯函数，不依赖 Vue / Naive）。 */
const {
  relationsForRelease, resolveRelation, resolveRelations, unmetRelations, primaryRelation, relationCardTag,
  relationTargetRepository, dependencyTagFor, relationsForMapRelease,
} = await import('../src/modRelations.ts')

function mod(overrides = {}) {
  return {
    name: 'Mod', dll_file: 'Mod.dll', enabled: true, version_source: 'plugin',
    update_available: false, duplicate_guid: false, status: 'installed', labels: [], ...overrides,
  }
}

function repo(overrides = {}) {
  return {
    id: 'dep-repo', name: 'Dep Repo', provider: 'github', username: 'u', repository: 'dep',
    download_mode: 'release', branch: 'main', file_path: '', region: '', channel: 'all',
    asset_rules: [], install_target: '', note: '', source_id: '',
    plugin_guid: 'com.example.dep',
    local_product_name: '', local_original_filename: '', version_regex: '',
    max_release_count: 10, download_threads: 1, enabled: true,
    cached_releases: [{ tag_name: 'v2.0.0', version: '2.0.0', name: '', body: '', prerelease: false, draft: false, assets: [] }],
    ...overrides,
  }
}

const DEP = { kind: 'requires', guid: 'com.example.dep' }

/* ===== requires：依赖 ===== */

test('已装且无版本要求 → ok', () => {
  const status = resolveRelation(DEP, [mod({ guid: 'com.example.dep', version: '1.0.0' })], [repo()])
  assert.equal(status.state, 'ok')
  assert.equal(status.installedVersion, '1.0.0')
})

test('省略 kind 时按依赖处理', () => {
  const status = resolveRelation({ guid: 'com.example.dep' }, [mod({ guid: 'com.example.dep' })], [])
  assert.equal(status.state, 'ok')
})

test('未安装但仓库里有 → installable', () => {
  const status = resolveRelation(DEP, [], [repo()])
  assert.equal(status.state, 'installable')
  assert.equal(status.repository?.id, 'dep-repo')
})

test('未安装且没有来源 → missing（不给按钮）', () => {
  const status = resolveRelation(DEP, [], [])
  assert.equal(status.state, 'missing')
  assert.equal(status.repository, undefined)
})

test('GUID 比较忽略大小写与空白', () => {
  const installed = [mod({ guid: '  COM.Example.DEP  ', version: '1.0.0' })]
  assert.equal(resolveRelation(DEP, installed, []).state, 'ok')
})

/*
 * 版本约束：已装但太旧仍然算不满足，并且要给「可安装」入口 ——
 * 用户需要的是升级到够新的版本，不是再装一遍。
 */
test('已装但低于下限 → 仍要提示，并给可安装来源', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=2.0.0' }
  const status = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '1.5.0' })], [repo()])
  assert.equal(status.state, 'installable')
  assert.equal(status.installedVersion, '1.5.0')
})

test('版本刚好达标 → ok', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=2.0.0' }
  const status = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '2.0.0' })], [])
  assert.equal(status.state, 'ok')
})

/* 上限：依赖库在 3.0 改了接口，这个 MOD 只能配 ≤2.x 的版本。 */
test('已装但高于上限 → 也算不满足', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '<=2.0.0' }
  const above = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '3.0.0' })], [])
  assert.equal(above.state, 'missing', '高于上限不该算满足')
  const equal = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '2.0.0' })], [])
  assert.equal(equal.state, 'ok', '上限本身是含的')
  const below = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '1.9.0' })], [])
  assert.equal(below.state, 'ok')
})

test('双端区间：只有落在 [min, max] 内才算满足', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=2.0,<=2.9' }
  for (const version of ['2.0', '2.5', '2.9']) {
    assert.equal(
      resolveRelation(dep, [mod({ guid: 'com.example.dep', version })], []).state, 'ok', version)
  }
  for (const version of ['1.9', '3.0']) {
    assert.notEqual(
      resolveRelation(dep, [mod({ guid: 'com.example.dep', version })], []).state, 'ok', version)
  }
})

test('本地没有版本号时，有版本要求就按不满足处理', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=1.0.0' }
  const status = resolveRelation(dep, [mod({ guid: 'com.example.dep' })], [])
  assert.equal(status.state, 'missing')
  assert.equal(status.installedVersion, null)
})

/* 四段 = beta 序号：1.8.7.2 等价于 1.8.7-beta.2，两者都不满足 1.8.7。 */
test('版本比较沿用项目的四段 beta 语义', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=1.8.7' }
  const beta = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '1.8.7.2' })], [])
  assert.equal(beta.state, 'missing', 'beta 低于同号正式版')
  const stable = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '1.8.7' })], [])
  assert.equal(stable.state, 'ok')
})

/* RAW 仓库不能用来满足依赖：它没有版本概念，装上也证明不了版本够。 */
test('RAW 模式仓库不作为可安装来源', () => {
  const raw = repo({ download_mode: 'repository_file' })
  assert.equal(resolveRelation(DEP, [], [raw]).state, 'missing')
})

/* 缓存里没有 Release 的仓库也不能装 —— 按钮点了会失败。 */
test('没有 Release 缓存的仓库不作为可安装来源', () => {
  const empty = repo({ cached_releases: [] })
  assert.equal(resolveRelation(DEP, [], [empty]).state, 'missing')
})

/* ===== conflicts：排斥 ===== */

test('冲突关系：对方没装 → ok', () => {
  const dep = { kind: 'conflicts', guid: 'com.example.dep' }
  assert.equal(resolveRelation(dep, [], []).state, 'ok')
})

test('冲突关系：对方装了且落在区间内 → conflict，并指出要卸哪个', () => {
  const dep = { kind: 'conflicts', guid: 'com.example.dep', version: '<=2.0' }
  const installed = [mod({ name: 'Old Dep', guid: 'com.example.dep', version: '1.5' })]
  const status = resolveRelation(dep, installed, [])
  assert.equal(status.state, 'conflict')
  assert.equal(status.installedMod?.name, 'Old Dep')
})

test('冲突关系：对方版本不在区间内 → ok（只排斥特定版本）', () => {
  const dep = { kind: 'conflicts', guid: 'com.example.dep', version: '>=1.0,<=2.0' }
  const installed = [mod({ guid: 'com.example.dep', version: '3.0' })]
  assert.equal(resolveRelation(dep, installed, []).state, 'ok')
})

test('冲突关系：装了被禁用的对方也算冲突（禁用不等于没装）', () => {
  const dep = { kind: 'conflicts', guid: 'com.example.dep' }
  const installed = [mod({ guid: 'com.example.dep', version: '1.0', enabled: false })]
  assert.equal(resolveRelation(dep, installed, []).state, 'conflict')
})

/* ===== compatible：兼容列表 ===== */

test('兼容关系：对方没装 → ok（不兼容只在对方存在时成立）', () => {
  const dep = { kind: 'compatible', guid: 'com.example.dep', version: '>=2.0' }
  assert.equal(resolveRelation(dep, [], []).state, 'ok')
})

test('兼容关系：对方版本在区间内 → ok', () => {
  const dep = { kind: 'compatible', guid: 'com.example.dep', version: '>=2.0,<=3.0' }
  const installed = [mod({ guid: 'com.example.dep', version: '2.5' })]
  assert.equal(resolveRelation(dep, installed, []).state, 'ok')
})

test('兼容关系：对方版本超出区间 → conflict', () => {
  const dep = { kind: 'compatible', guid: 'com.example.dep', version: '>=2.0,<=3.0' }
  const installed = [mod({ name: 'Future Dep', guid: 'com.example.dep', version: '4.0' })]
  const status = resolveRelation(dep, installed, [])
  assert.equal(status.state, 'conflict')
  assert.equal(status.installedMod?.name, 'Future Dep')
})

/* ===== 汇总与优先级 ===== */

test('可选依赖不参与判定', () => {
  const owner = repo({
    id: 'owner',
    relations: [{ kind: 'requires', guid: 'com.example.dep', optional: true }],
  })
  assert.deepEqual(resolveRelations(owner, [], [repo()]), [])
})

test('必选依赖才参与判定，且排除仓库自身作为来源', () => {
  const owner = repo({ id: 'owner', plugin_guid: 'com.example.owner', relations: [DEP] })
  // 自身 GUID 与依赖不同、且列表里只有自己 → 没有可安装来源
  const statuses = resolveRelations(owner, [], [owner])
  assert.equal(statuses.length, 1)
  assert.equal(statuses[0].state, 'missing')
})

test('自我依赖不会把自己当作来源（配置错误也不产生可点按钮）', () => {
  // 仓库声明依赖自己的 GUID
  const selfDependent = repo({
    id: 'self',
    plugin_guid: 'com.example.self',
    relations: [{ kind: 'requires', guid: 'com.example.self' }],
  })
  const statuses = resolveRelations(selfDependent, [], [selfDependent])
  assert.equal(statuses[0].state, 'missing')
})

test('unmetRelations 只返回有问题的条目', () => {
  const statuses = [
    { relation: DEP, state: 'ok', installedVersion: '1.0.0' },
    { relation: { kind: 'requires', guid: 'other' }, state: 'missing', installedVersion: null },
  ]
  assert.equal(unmetRelations(statuses).length, 1)
  assert.equal(unmetRelations(statuses)[0].relation.guid, 'other')
})

/* 冲突优先于缺依赖：冲突可能已经让游戏崩了，缺依赖只是不工作。 */
test('primaryRelation 优先返回冲突，其次可安装，最后只提示的', () => {
  const missing = { relation: { guid: 'a' }, state: 'missing', installedVersion: null }
  const installable = { relation: { guid: 'b' }, state: 'installable', installedVersion: null }
  const conflict = { relation: { guid: 'c' }, state: 'conflict', installedVersion: '1.0' }
  assert.equal(primaryRelation([missing, installable, conflict])?.state, 'conflict')
  assert.equal(primaryRelation([missing, installable])?.state, 'installable')
  assert.equal(primaryRelation([missing])?.state, 'missing')
  assert.equal(primaryRelation([]), undefined)
})

/* ===== 按 Release 覆盖 ===== */

/*
 * 同一 MOD 的不同大版本可以依赖不同版本的基础库。
 * A 1.0 要 B ≥2.0，A 1.1 要 B ≥3.0 —— 仓库级 relations 表达不了这种差异。
 */
test('relationsForRelease 按版本选最高的匹配覆盖', () => {
  const owner = repo({
    relations: [{ kind: 'requires', guid: 'b', version: '>=2.0' }],
    release_relations: [
      { version: '>=1.1', relations: [{ kind: 'requires', guid: 'b', version: '>=3.0' }] },
      { version: '>=1.5', relations: [{ kind: 'requires', guid: 'b', version: '>=4.0' }] },
    ],
  })
  // 1.0 低于所有覆盖门槛 → 用默认
  assert.equal(relationsForRelease(owner, '1.0')[0].version, '>=2.0')
  // 1.1 命中第一条
  assert.equal(relationsForRelease(owner, '1.1')[0].version, '>=3.0')
  // 1.6 命中更高那条（取最大匹配，不是第一条匹配）
  assert.equal(relationsForRelease(owner, '1.6')[0].version, '>=4.0')
})

/* 上限也是区间的一部分：命中区间之外要回到默认值。 */
test('relationsForRelease 尊重区间上限', () => {
  const owner = repo({
    relations: [{ kind: 'requires', guid: 'b', version: '>=2.0' }],
    release_relations: [{
      version: '>=1.1,<=1.9',
      relations: [{ kind: 'requires', guid: 'b', version: '>=3.0' }],
    }],
  })
  assert.equal(relationsForRelease(owner, '1.5')[0].version, '>=3.0', '区间内应命中')
  assert.equal(relationsForRelease(owner, '2.0')[0].version, '>=2.0', '超过上限应回默认')
  assert.equal(relationsForRelease(owner, '1.0')[0].version, '>=2.0', '低于下限应回默认')
})

/* 只写上限也应当是合法区间（例如「2.0 之前用它」）。 */
test('relationsForRelease 允许只写上限', () => {
  const owner = repo({
    relations: [{ kind: 'requires', guid: 'b', version: '>=2.0' }],
    release_relations: [{
      version: '<=2.0',
      relations: [{ kind: 'requires', guid: 'b', version: '>=9.0' }],
    }],
  })
  assert.equal(relationsForRelease(owner, '1.0')[0].version, '>=9.0')
  assert.equal(relationsForRelease(owner, '2.1')[0].version, '>=2.0')
})

test('没有版本号时只用仓库级默认声明', () => {
  const owner = repo({
    relations: [{ kind: 'requires', guid: 'b', version: '>=2.0' }],
    release_relations: [
      { version: '>=1.0', relations: [{ kind: 'requires', guid: 'b', version: '>=9.0' }] },
    ],
  })
  assert.equal(relationsForRelease(owner, null)[0].version, '>=2.0')
  assert.equal(relationsForRelease(owner, '')[0].version, '>=2.0')
})

test('覆盖是整体替换而不是合并', () => {
  const owner = repo({
    relations: [{ kind: 'requires', guid: 'old' }],
    release_relations: [
      { version: '>=2.0', relations: [{ kind: 'requires', guid: 'new' }] },
    ],
  })
  const result = relationsForRelease(owner, '2.0')
  assert.equal(result.length, 1)
  assert.equal(result[0].guid, 'new', '旧关系应被替换掉，不是并存')
})

test('release_relations 可以同时覆盖依赖与冲突', () => {
  const owner = repo({
    id: 'owner', plugin_guid: 'owner',
    relations: [{ kind: 'requires', guid: 'b', version: '>=2.0' }],
    release_relations: [{
      version: '>=2.0',
      relations: [
        { kind: 'requires', guid: 'b', version: '>=3.0' },
        { kind: 'conflicts', guid: 'legacy', version: '<=1.0' },
      ],
    }],
  })
  const statuses = resolveRelations(owner, [], [], '2.5')
  assert.equal(statuses.length, 2)
  assert.equal(statuses[0].relation.kind, 'requires')
  assert.equal(statuses[0].relation.version, '>=3.0')
  assert.equal(statuses[1].relation.kind, 'conflicts')
})

test('resolveRelations 接受 releaseVersion 并按覆盖后的声明判定', () => {
  const b = repo({ id: 'b-repo', plugin_guid: 'b', name: 'B' })
  const owner = repo({
    id: 'owner', plugin_guid: 'owner', name: 'Owner',
    relations: [{ kind: 'requires', guid: 'b', version: '>=2.0' }],
    release_relations: [
      { version: '>=1.1', relations: [{ kind: 'requires', guid: 'b', version: '>=3.0' }] },
    ],
  })
  // 本地 B 是 2.5：默认要求（≥2.0）满足，但 1.1 的要求（≥3.0）不满足
  const installed = [mod({ guid: 'b', version: '2.5' })]
  assert.equal(resolveRelations(owner, installed, [b], '1.0')[0].state, 'ok')
  /*
   * 覆盖后的要求是 ≥3.0，而仓库 B 最高只有 2.0.0 —— 装了也不满足，
   * 所以必须是 missing 而不是 installable：给按钮等于给一个点了没用的入口。
   */
  assert.equal(resolveRelations(owner, installed, [b], '1.1')[0].state, 'missing')

  // 仓库里有满足要求的版本时才是 installable
  const bWithV3 = repo({
    id: 'b-repo', plugin_guid: 'b', name: 'B',
    cached_releases: [{ tag_name: 'v3.1.0', version: '3.1.0', name: '', body: '', prerelease: false, draft: false, assets: [] }],
  })
  const installable = resolveRelations(owner, installed, [bWithV3], '1.1')[0]
  assert.equal(installable.state, 'installable')
  assert.equal(installable.release?.tag_name, 'v3.1.0')
})

/* 来源仓库里的 Release 必须落在要求区间内，否则不算可安装来源。 */
test('来源仓库没有满足区间的版本时不提供安装入口', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=9.0' }
  // rep() 的缓存里只有 v2.0.0
  assert.equal(resolveRelation(dep, [], [repo()]).state, 'missing')

  const higher = repo({
    cached_releases: [{ tag_name: 'v9.5.0', version: '9.5.0', name: '', body: '', prerelease: false, draft: false, assets: [] }],
  })
  const status = resolveRelation(dep, [], [higher])
  assert.equal(status.state, 'installable')
  assert.equal(status.release?.tag_name, 'v9.5.0')
})

/* 自动挑版本时跳过预发布与草稿：不能把用户悄悄带到测试版上。 */
test('自动选源跳过预发布与草稿 Release', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep' }
  const onlyBeta = repo({
    cached_releases: [
      { tag_name: 'v3.0.0-beta.1', version: '3.0.0-beta.1', name: '', body: '', prerelease: true, draft: false, assets: [] },
      { tag_name: 'v2.5.0', version: '2.5.0', name: '', body: '', prerelease: false, draft: true, assets: [] },
    ],
  })
  assert.equal(resolveRelation(dep, [], [onlyBeta]).state, 'missing')

  const withStable = repo({
    cached_releases: [
      { tag_name: 'v3.0.0-beta.1', version: '3.0.0-beta.1', name: '', body: '', prerelease: true, draft: false, assets: [] },
      { tag_name: 'v2.0.0', version: '2.0.0', name: '', body: '', prerelease: false, draft: false, assets: [] },
    ],
  })
  assert.equal(resolveRelation(dep, [], [withStable]).release?.tag_name, 'v2.0.0')
})

/* ===== 安装前计划 ===== */

const { buildInstallPlan, planHasActions } = await import('../src/modRelations.ts')

const OWNER = { id: 'owner', plugin_guid: 'com.example.owner', name: 'Owner' }

test('没有关系声明时计划为空', () => {
  const plan = buildInstallPlan(repo(OWNER), [], [])
  assert.equal(planHasActions(plan), false)
})

test('缺依赖且有可安装版本 → 进 install', () => {
  const owner = repo({ ...OWNER, relations: [{ kind: 'requires', guid: 'com.example.dep' }] })
  const plan = buildInstallPlan(owner, [], [repo()])
  assert.equal(plan.install.length, 1)
  assert.equal(plan.uninstall.length, 0)
  assert.equal(plan.blocked.length, 0)
})

test('缺依赖且没有可安装版本 → 进 blocked（只提醒）', () => {
  const owner = repo({ ...OWNER, relations: [{ kind: 'requires', guid: 'com.example.dep' }] })
  const plan = buildInstallPlan(owner, [], [])
  assert.equal(plan.install.length, 0)
  assert.equal(plan.blocked.length, 1)
  assert.equal(plan.blocked[0].state, 'missing')
})

/*
 * 冲突的两种解法优先级：
 *   线上有「不上冲突区间」的版本 → 换版本（不卸载）
 *   没有任何可用版本           → 卸载
 * 换版本优先，因为用户本来想继续用那个 MOD。
 */
test('冲突且有可用版本 → 进 install（换版本而不是卸载）', () => {
  const owner = repo({
    ...OWNER,
    relations: [{ kind: 'conflicts', guid: 'com.example.dep', version: '<=1.0' }],
  })
  const installed = [mod({ guid: 'com.example.dep', version: '0.9' })]
  // repo() 的缓存是 v2.0.0，2.0 不在 [.., 1.0] 冲突区间内 → 可用
  const plan = buildInstallPlan(owner, installed, [repo()])
  assert.equal(plan.install.length, 1)
  assert.equal(plan.install[0].release?.tag_name, 'v2.0.0')
  assert.equal(plan.uninstall.length, 0)
})

test('冲突且没有可用版本 → 进 uninstall', () => {
  const owner = repo({
    ...OWNER,
    relations: [{ kind: 'conflicts', guid: 'com.example.dep' }],
  })
  const installed = [mod({ guid: 'com.example.dep', version: '1.0' })]
  // 不带任何来源仓库：没有「换上就没事」的版本，只能卸载
  const plan = buildInstallPlan(owner, installed, [])
  assert.equal(plan.uninstall.length, 1)
  assert.equal(plan.uninstall[0].installedMod?.guid, 'com.example.dep')
  assert.equal(plan.install.length, 0)
})

test('兼容关系不满足且线上有区间内版本 → 进 install（升级/降级）', () => {
  const owner = repo({
    ...OWNER,
    relations: [{
      kind: 'compatible', guid: 'com.example.dep', version: '>=2.0,<=3.0',
    }],
  })
  const installed = [mod({ guid: 'com.example.dep', version: '1.0' })]
  // 缓存里 v2.0.0 落在 [2.0, 3.0] 内 → 可升级到它
  const plan = buildInstallPlan(owner, installed, [repo()])
  assert.equal(plan.install.length, 1)
  assert.equal(plan.install[0].release?.tag_name, 'v2.0.0')
})

test('兼容关系不满足且没有区间内版本 → 进 blocked（卸载解决不了）', () => {
  const owner = repo({
    ...OWNER,
    relations: [{
      kind: 'compatible', guid: 'com.example.dep', version: '>=5.0,<=6.0',
    }],
  })
  const installed = [mod({ guid: 'com.example.dep', version: '1.0' })]
  const plan = buildInstallPlan(owner, installed, [repo()])
  assert.equal(plan.blocked.length, 1)
  assert.equal(plan.install.length, 0)
  assert.equal(plan.uninstall.length, 0, '卸载对方解决不了版本不兼容')
})

/* 一次安装要把三件事都排进去，且顺序是 卸载 → 换版本/装依赖。 */
test('计划能同时包含卸载、安装与提醒', () => {
  const owner = repo({
    ...OWNER,
    relations: [
      { kind: 'conflicts', guid: 'legacy' },
      { kind: 'requires', guid: 'com.example.dep' },
      { kind: 'requires', guid: 'missing.forever' },
    ],
  })
  const installed = [mod({ guid: 'legacy', version: '1.0' })]
  const plan = buildInstallPlan(owner, installed, [repo()])
  assert.equal(plan.uninstall.length, 1)
  assert.equal(plan.install.length, 1)
  assert.equal(plan.blocked.length, 1)
  assert.equal(planHasActions(plan), true)
})

test('安装前计划按目标 Release 版本选关系表', () => {
  const owner = repo({
    ...OWNER,
    relations: [{ kind: 'requires', guid: 'com.example.dep', version: '>=2.0' }],
    release_relations: [
      { version: '>=2.0', relations: [] },
    ],
  })
  // 装 2.0：覆盖后关系表为空 → 无需任何动作
  const plan = buildInstallPlan(owner, [], [repo()], '2.0')
  assert.equal(planHasActions(plan), false)
  // 装 1.0：用默认要求，本地没装 → 要装依赖
  const older = buildInstallPlan(owner, [], [repo()], '1.0')
  assert.equal(older.install.length, 1)
})

/* ===== 发布线（prerelease）===== */

/*
 * 正式版与测试版常是同一 GUID 的两条发布线（stable / beta 两个仓库条目）。
 * 版本区间无法把它们分干净：`1.9.8.1`（= 1.9.8-beta.1）排在 `1.9.8` 之下，
 * 所以 `[1.9.8, 1.9.9)` 命中正式版却漏掉测试版。`prerelease` 直接按发布线筛。
 */
const SAME_GUID = 'com.example.self'

test('prerelease=true 只匹配测试版', () => {
  const rel = { kind: 'conflicts', guid: SAME_GUID, prerelease: true }
  const stableCopy = [mod({ guid: SAME_GUID, version: '1.9.8' })]
  const betaCopy = [mod({ guid: SAME_GUID, version: '1.9.8.1', prerelease: true })]
  assert.equal(resolveRelation(rel, stableCopy, []).state, 'ok', '正式版不该命中')
  assert.equal(resolveRelation(rel, betaCopy, []).state, 'conflict', '测试版应命中')
})

test('prerelease=false 只匹配正式版', () => {
  const rel = { kind: 'conflicts', guid: SAME_GUID, prerelease: false }
  const stableCopy = [mod({ guid: SAME_GUID, version: '1.9.8' })]
  const betaCopy = [mod({ guid: SAME_GUID, version: '1.9.9.3', prerelease: true })]
  assert.equal(resolveRelation(rel, stableCopy, []).state, 'conflict')
  assert.equal(resolveRelation(rel, betaCopy, []).state, 'ok')
})

test('不写 prerelease 时两条发布线都算', () => {
  const rel = { kind: 'conflicts', guid: SAME_GUID }
  assert.equal(resolveRelation(rel, [mod({ guid: SAME_GUID, version: '1.9.8' })], []).state, 'conflict')
  assert.equal(
    resolveRelation(rel, [mod({ guid: SAME_GUID, version: '1.9.8.1', prerelease: true })], []).state,
    'conflict')
})

/* 四段写法本身就是预发布，不依赖清单标记也能判出来。 */
test('四段版本号按预发布处理（即使清单没标记）', () => {
  const rel = { kind: 'conflicts', guid: SAME_GUID, prerelease: true }
  assert.equal(resolveRelation(rel, [mod({ guid: SAME_GUID, version: '1.9.8.7' })], []).state, 'conflict')
  const stableOnly = { kind: 'conflicts', guid: SAME_GUID, prerelease: false }
  assert.equal(resolveRelation(stableOnly, [mod({ guid: SAME_GUID, version: '1.9.8.7' })], []).state, 'ok')
})

/* 正式版与测试版互斥：两边都装时各自都报冲突，且指向的是对方。 */
test('同 GUID 两条发布线互斥：两边都装时互相报冲突', () => {
  const stable = repo({
    id: 'hu-stable', plugin_guid: SAME_GUID,
    relations: [{ kind: 'conflicts', guid: SAME_GUID, prerelease: true }],
  })
  const beta = repo({
    id: 'hu-beta', plugin_guid: SAME_GUID,
    relations: [{ kind: 'conflicts', guid: SAME_GUID, prerelease: false }],
  })
  const installed = [
    mod({ name: 'Stable', guid: SAME_GUID, version: '1.9.8' }),
    mod({ name: 'Beta', guid: SAME_GUID, version: '1.9.9.1', prerelease: true }),
  ]
  const fromStable = resolveRelations(stable, installed, [stable, beta])
  const fromBeta = resolveRelations(beta, installed, [stable, beta])
  assert.equal(fromStable[0].state, 'conflict')
  assert.equal(fromStable[0].installedMod?.name, 'Beta', '正式版应指向测试版')
  assert.equal(fromBeta[0].state, 'conflict')
  assert.equal(fromBeta[0].installedMod?.name, 'Stable', '测试版应指向正式版')
})

/* 只装一边时谁也不冲突 —— 否则装完自己的正式版就会立刻提示冲突。 */
test('同 GUID 发布线互斥：只装一边不报冲突', () => {
  const stable = repo({
    id: 'hu-stable', plugin_guid: SAME_GUID,
    relations: [{ kind: 'conflicts', guid: SAME_GUID, prerelease: true }],
  })
  const installed = [mod({ name: 'Stable', guid: SAME_GUID, version: '1.9.8' })]
  // 关系仍然返回一条（state=ok），但没有未满足项
  assert.deepEqual(unmetRelations(resolveRelations(stable, installed, [stable])), [])
})

/* 发布线互斥只给卸载，不给「换版本」——换发布线不是装同一 GUID 的另一个包。 */
test('发布线互斥的冲突不进 install 计划，只进 uninstall', () => {
  const stable = repo({
    id: 'hu-stable', plugin_guid: SAME_GUID,
    relations: [{ kind: 'conflicts', guid: SAME_GUID, prerelease: true }],
    cached_releases: [{
      tag_name: 'v2.0.0', version: '2.0.0', name: '', body: '',
      prerelease: false, draft: false, assets: [],
    }],
  })
  const installed = [mod({ guid: SAME_GUID, version: '1.9.9.1', prerelease: true })]
  const plan = buildInstallPlan(stable, installed, [stable])
  assert.equal(plan.uninstall.length, 1)
  assert.equal(plan.install.length, 0, '换同一 GUID 的版本解决不了发布线冲突')
})

/* 自动选源要尊重发布线：关系限定了正式版，就不能自动装预发布。 */
test('依赖自动选源跳过预发布版本', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep' }
  const betaOnly = repo({
    cached_releases: [{
      tag_name: 'v3.0.0-beta.1', version: '3.0.0-beta.1', name: '', body: '',
      prerelease: true, draft: false, assets: [],
    }],
  })
  assert.equal(resolveRelation(dep, [], [betaOnly]).state, 'missing')
})

/* ===== 标签跳转目标 ===== */

const { relationJumpRepositoryId } = await import('../src/modRelations.ts')

/*
 * 缺依赖 / 冲突标签必须跳到**依赖的**仓库。
 *
 * 之前的实现跳到当前 MOD 自己绑定的仓库，于是用户在安装页看到的是一张
 * 毫无问题的卡片，找不到要装的东西。这里固定「目标取自关系」这条规则。
 */
test('关系跳转目标取依赖仓库，而不是当前 MOD 的仓库', () => {
  const dep = repo({ id: 'cma-repo', name: 'Configuration Manager' })
  const status = resolveRelation({ kind: 'requires', guid: 'com.example.dep' }, [], [dep])
  assert.equal(status.repository?.id, 'cma-repo')
  assert.equal(relationJumpRepositoryId(status), 'cma-repo')
})

test('没有可安装来源时不给跳转目标（标签保持不可点）', () => {
  const status = resolveRelation({ kind: 'requires', guid: 'com.example.dep' }, [], [])
  assert.equal(status.state, 'missing')
  assert.equal(relationJumpRepositoryId(status), undefined)
})

test('没有关系问题时也没有跳转目标', () => {
  assert.equal(relationJumpRepositoryId(undefined), undefined)
})

/* 冲突同样指向关系里那个仓库（线上有可修复版本时才有目标）。 */
test('冲突关系的跳转目标同样取关系仓库', () => {
  const installed = [mod({ guid: 'com.example.dep', version: '0.9' })]
  const status = resolveRelation(
    { kind: 'conflicts', guid: 'com.example.dep', version: '<=1.0' },
    installed,
    [repo()],
  )
  assert.equal(status.state, 'conflict')
  assert.equal(relationJumpRepositoryId(status), 'dep-repo')
})

/* ===== 卡头关系标签：未安装的卡也要能看到缺依赖 ===== */

/*
 * 真实反馈：关卡包（DIYLevel 地图包）卡片上看不到「缺依赖」，也没有提示
 * 要先装前置，装上才发现跑不起来。
 *
 * 「包」和「单个 MOD」的差别只是 DLL 数量，依赖信息不该因未安装而消失。
 * 唯一例外是冲突：未安装的卡片挂「冲突」会和底部的「安装」按钮自相矛盾。
 */

test('未安装且缺依赖 → 仍显示缺依赖标签', () => {
  const status = resolveRelation(DEP, [], [repo()])
  assert.equal(status.state, 'installable')
  assert.deepEqual(relationCardTag(status, false), { kind: 'missingDependency' })
})

test('未安装且没有可安装来源的依赖 → 仍显示缺依赖标签', () => {
  const status = resolveRelation(DEP, [], [])
  assert.equal(status.state, 'missing')
  assert.deepEqual(relationCardTag(status, false), { kind: 'missingDependency' })
})

test('已安装且缺依赖 → 显示缺依赖标签', () => {
  const status = resolveRelation(DEP, [], [repo()])
  assert.deepEqual(relationCardTag(status, true), { kind: 'missingDependency' })
})

test('已安装且冲突 → 显示冲突标签', () => {
  const installed = [mod({ guid: 'com.example.dep', version: '0.9' })]
  const status = resolveRelation(
    { kind: 'conflicts', guid: 'com.example.dep', version: '<=1.0' },
    installed,
    [],
  )
  assert.equal(status.state, 'conflict')
  assert.deepEqual(relationCardTag(status, true), { kind: 'conflict' })
})

test('未安装且冲突 → 不显示（否则与「安装」按钮矛盾）', () => {
  // 测试版卡片对稳定版声明 conflicts{prerelease:false}：用户没装这张卡，
  // 卡头却挂「冲突」，而主按钮是「安装」—— 用户不知道该卸载什么
  const status = {
    state: 'conflict',
    relation: { kind: 'conflicts', guid: 'com.example.dep' },
  }
  assert.equal(relationCardTag(status, false), undefined)
})

test('没有未满足的关系 → 不显示', () => {
  assert.equal(relationCardTag(undefined, false), undefined)
  assert.equal(relationCardTag(undefined, true), undefined)
})

/* ===== 展示：关系目标解析（名字取仓库配置里的，不摊 GUID） ===== */

const HU_GUID = 'com.ch3ngyz.plugin.HostUtilities'
const STABLE_RELEASE = {
  tag_name: '1.9.8', version: '1.9.8', name: '', body: '', prerelease: false, draft: false, assets: [],
}
const BETA_RELEASE = {
  tag_name: '1.9.8.1', version: '1.9.8.1', name: '', body: '', prerelease: false, draft: false, assets: [],
}

test('展示：同一 GUID 按发布线定位到对应仓库', () => {
  const stable = repo({
    id: 'hu-stable', name: 'HostUtilities Stable', plugin_guid: HU_GUID,
    cached_releases: [STABLE_RELEASE],
  })
  const beta = repo({
    id: 'hu-beta', name: 'HostUtilities Beta', plugin_guid: HU_GUID,
    cached_releases: [BETA_RELEASE],
  })
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: true }, [stable, beta], 'hu-stable')?.id,
    'hu-beta',
    '正式版声明的「与测试版互斥」应指向测试版仓库',
  )
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: false }, [stable, beta], 'hu-beta')?.id,
    'hu-stable',
    '测试版声明的「与正式版互斥」应指向正式版仓库',
  )
})

test('展示：不限发布线时取第一个声明了该 GUID 的仓库', () => {
  const first = repo({ id: 'a', name: 'A', plugin_guid: HU_GUID, cached_releases: [STABLE_RELEASE] })
  const second = repo({ id: 'b', name: 'B', plugin_guid: HU_GUID, cached_releases: [BETA_RELEASE] })
  assert.equal(relationTargetRepository({ kind: 'requires', guid: HU_GUID }, [first, second])?.id, 'a')
})

test('展示：排除声明方自己，避免自冲突显示成「指向自己」', () => {
  const self = repo({ id: 'self', name: 'Self', plugin_guid: HU_GUID, cached_releases: [STABLE_RELEASE] })
  assert.equal(relationTargetRepository({ kind: 'requires', guid: HU_GUID }, [self], 'self'), undefined)
})

test('展示：Release 未拉到时用另一条线兜底（不退回 GUID）', () => {
  // 频道 all + 空缓存 → 发布线未知；声明方已排除，剩下的就是另一条线
  const stable = repo({ id: 'hu-stable', name: 'Stable', plugin_guid: HU_GUID, cached_releases: [] })
  const beta = repo({ id: 'hu-beta', name: 'Beta', plugin_guid: HU_GUID, cached_releases: [] })
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: true }, [stable, beta], 'hu-stable')?.id,
    'hu-beta',
  )
})

test('展示：没有仓库声明该 GUID → undefined（调用方退回 GUID）', () => {
  assert.equal(relationTargetRepository({ guid: 'com.example.nobody' }, [repo()]), undefined)
})

test('展示：没有 Release 时按 channel 判发布线', () => {
  const betaOnly = repo({ id: 'beta', plugin_guid: HU_GUID, channel: 'beta', cached_releases: [] })
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: true }, [betaOnly])?.id,
    'beta',
  )
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: false }, [betaOnly]),
    undefined,
    '限定正式版的关系不该匹配只发测试版的仓库',
  )
})

test('展示：只有另一条线的仓库时返回 undefined', () => {
  const stableOnly = repo({ id: 's', plugin_guid: HU_GUID, cached_releases: [STABLE_RELEASE] })
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: true }, [stableOnly]),
    undefined,
  )
})

test('展示：同仓同时发两条线时两种关系都能命中', () => {
  const both = repo({
    id: 'both', plugin_guid: HU_GUID, cached_releases: [STABLE_RELEASE, BETA_RELEASE],
  })
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: true }, [both])?.id,
    'both',
  )
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: false }, [both])?.id,
    'both',
  )
})

test('展示：多个未知发布线候选时不猜兄弟仓库', () => {
  const first = repo({ id: 'a', plugin_guid: HU_GUID, cached_releases: [] })
  const second = repo({ id: 'b', plugin_guid: HU_GUID, cached_releases: [] })
  assert.equal(
    relationTargetRepository({ kind: 'conflicts', guid: HU_GUID, prerelease: true }, [first, second]),
    undefined,
    '两个候选的发布线都未知时，猜错会显示另一条线的仓库名，不如退回本地 MOD 名',
  )
})

/* ===== 卡片关系标签的三种说法：冲突 / 可更新·可安装 / 缺依赖 ===== */

/*
 * 真实反馈：地图包依赖的 DIYLevel 线上发了新版，卡片却写「缺依赖」，
 * 用户以为这张图没救 —— 其实点「去安装」跳到那个仓库就修好了。
 *
 * 现在三种说法与依赖表（`dependencyRows.statusFor`）同一套词：有候选版本
 * 就写「可更新 / 可安装」，线上连候选都没有才是真的「缺依赖」。
 */

test('关系标签：依赖没装但线上能装 → 可安装（中档，带仓库）', () => {
  const status = resolveRelation(DEP, [], [repo()])
  assert.equal(status.state, 'installable')
  assert.deepEqual(dependencyTagFor([status], true), {
    text: 'tools.installable',
    tone: 'moderate',
    kind: 'installable',
    repositoryId: 'dep-repo',
  })
})

test('关系标签：装着旧版且线上有满足区间的版本 → 可更新（中档，带仓库）', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=2.0.0' }
  const status = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '1.5.0' })], [repo()])
  assert.equal(status.state, 'installable')
  assert.notEqual(status.installedMod, undefined, '本地那一份是「升级」而不是「补装」的依据')
  assert.deepEqual(dependencyTagFor([status], true), {
    text: 'tools.updatable',
    tone: 'moderate',
    kind: 'installable',
    repositoryId: 'dep-repo',
  })
})

test('关系标签：卡片自己没装时不显示「可安装 / 可更新」', () => {
  const status = resolveRelation(DEP, [], [repo()])
  assert.equal(status.state, 'installable')
  assert.equal(
    dependencyTagFor([status], false),
    undefined,
    '未安装的卡片头上不该出现描述依赖状态的「可安装」标签',
  )
  const upgrade = resolveRelation(
    { kind: 'requires', guid: 'com.example.dep', version: '>=2.0.0' },
    [mod({ guid: 'com.example.dep', version: '1.5.0' })],
    [repo()],
  )
  assert.equal(upgrade.state, 'installable')
  assert.equal(dependencyTagFor([upgrade], false), undefined)
})

test('关系标签：卡片自己没装、依赖线上也没有来源 → 仍报缺依赖', () => {
  const status = resolveRelation(DEP, [], [])
  assert.equal(status.state, 'missing')
  assert.deepEqual(dependencyTagFor([status], false), {
    text: 'modsOnline.missingDep',
    tone: 'critical',
    kind: 'missingDependency',
  })
})

/* ===== 地图按版本覆盖表：与仓库共用同一套「命中 + 下限最高」规则 ===== */

test('地图覆盖表：命中取区间下限最高的一张整体替换，未命中回落默认表', () => {
  const map = {
    relations: [{ kind: 'requires', guid: 'base.guid' }],
    release_relations: [
      { version: '>=1.1', relations: [{ kind: 'requires', guid: 'mid.guid' }] },
      { version: '>=1.5,<2.0', relations: [{ kind: 'requires', guid: 'new.guid' }] },
    ],
  }
  assert.deepEqual(
    relationsForMapRelease(map, '1.2').map((relation) => relation.guid),
    ['mid.guid'],
  )
  assert.deepEqual(
    relationsForMapRelease(map, '1.6').map((relation) => relation.guid),
    ['new.guid'],
  )
  // 被上限排除后回落到仍命中的那张（>=1.1 没有上限），而不是默认表
  assert.deepEqual(
    relationsForMapRelease(map, '2.5').map((relation) => relation.guid),
    ['mid.guid'],
  )
  // 没有目标版本时无从判断，按默认表
  assert.deepEqual(
    relationsForMapRelease(map, null).map((relation) => relation.guid),
    ['base.guid'],
  )
})

test('地图覆盖表：没有 relations 时先用 required_frameworks 兜底，再套覆盖表', () => {
  const map = {
    required_frameworks: [{ guid: 'dev.diy', version: '>=19.0' }],
    release_relations: [
      { version: '>=2.0', relations: [{ kind: 'requires', guid: 'dev.diy', version: '>=20.0' }] },
    ],
  }
  assert.deepEqual(relationsForMapRelease(map, '1.0'), [
    { kind: 'requires', guid: 'dev.diy', version: '>=19.0' },
  ])
  assert.deepEqual(relationsForMapRelease(map, '2.5'), [
    { kind: 'requires', guid: 'dev.diy', version: '>=20.0' },
  ])
})

test('关系标签：线上没有满足区间的 Release → 才是缺依赖（严重，不可点）', () => {
  const status = resolveRelation(DEP, [], [])
  assert.equal(status.state, 'missing')
  assert.deepEqual(dependencyTagFor([status], true), {
    text: 'modsOnline.missingDep',
    tone: 'critical',
    kind: 'missingDependency',
  })
})

test('关系标签：区间把线上版本全排除时不显示「可更新」', () => {
  const dep = { kind: 'requires', guid: 'com.example.dep', version: '>=9.0.0' }
  const status = resolveRelation(dep, [mod({ guid: 'com.example.dep', version: '1.5.0' })], [repo()])
  assert.equal(status.state, 'missing')
  assert.deepEqual(dependencyTagFor([status], true), {
    text: 'modsOnline.missingDep',
    tone: 'critical',
    kind: 'missingDependency',
  })
})
