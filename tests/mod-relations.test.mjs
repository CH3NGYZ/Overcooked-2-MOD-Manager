import assert from 'node:assert/strict'
import test from 'node:test'

/** 关系判定：真实源码直接执行（纯函数，不依赖 Vue / Naive）。 */
const {
  relationsForRelease, resolveRelation, resolveRelations, unmetRelations, relationJumpTarget,
  relationTargetRepository, dependencyTagFor, relationsForMapRelease, dependencyRepairStatus,
} = await import('../src/modRelations.ts')

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

test('unmetRelations 只返回有问题的条目', () => {
  const statuses = [
    { relation: DEP, state: 'ok', installedVersion: '1.0.0' },
    { relation: { kind: 'requires', guid: 'other' }, state: 'missing', installedVersion: null },
  ]
  assert.equal(unmetRelations(statuses).length, 1)
  assert.equal(unmetRelations(statuses)[0].relation.guid, 'other')
})

test('卡头优先给能处理整链的条目，只有提示时不显示可点样式', () => {
  const missing = { relation: { guid: 'a' }, state: 'missing', installedVersion: null }
  const repairable = { ...missing, relation: { guid: 'b' }, repair: { node_id: 'mod:owner' } }
  const conflict = { ...repairable, relation: { kind: 'conflicts', guid: 'c' }, state: 'conflict', installedVersion: '1.0' }
  assert.equal(dependencyTagFor([missing, repairable, conflict], true)?.dependency, conflict)
  assert.equal(dependencyTagFor([missing, repairable], true)?.dependency, repairable)
  assert.equal(dependencyTagFor([missing], true)?.clickable, false)
  assert.equal(dependencyTagFor([], true), undefined)
})

test('卡头不把尚未评估或无法确认的依赖当成已确认的问题', () => {
  const unassessed = resolveRelation(DEP, [])
  const unknown = { ...unassessed, state: 'unknown', repair: { node_id: 'mod:owner' } }
  const missing = { ...unassessed, state: 'missing' }
  assert.equal(unassessed.state, 'unassessed')
  assert.equal(dependencyTagFor([unassessed, { ...unassessed, state: 'ok' }], true), undefined)
  assert.equal(dependencyTagFor([unknown, missing], false), undefined)
  const unresolved = dependencyTagFor([unassessed, unknown], true)
  assert.equal(unresolved?.text, 'tools.relationUnconfirmed')
  assert.equal(unresolved?.clickable, false)
  assert.equal(relationJumpTarget(unknown), undefined)
  assert.equal(dependencyRepairStatus([unassessed, unknown]), undefined)
  assert.deepEqual(unmetRelations([unassessed, unknown, missing]), [missing])
  assert.equal(dependencyTagFor([unassessed, unknown, missing], true)?.text, 'tools.relationNeedsAction')
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
