import assert from 'node:assert/strict'
import test from 'node:test'

/** 依赖判定：真实源码直接执行（纯函数，不依赖 Vue / Naive）。 */
const { resolveDependency, resolveDependencies, firstUnmetDependency } = await import(
  '../src/modDependencies.ts'
)

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

const DEP = { guid: 'com.example.dep' }

test('已装且无版本要求 → satisfied', () => {
  const status = resolveDependency(DEP, [mod({ guid: 'com.example.dep', version: '1.0.0' })], [repo()])
  assert.equal(status.state, 'satisfied')
  assert.equal(status.installedVersion, '1.0.0')
})

test('未安装但仓库里有 → installable', () => {
  const status = resolveDependency(DEP, [], [repo()])
  assert.equal(status.state, 'installable')
  assert.equal(status.repository?.id, 'dep-repo')
})

test('未安装且没有来源 → missing（不给按钮）', () => {
  const status = resolveDependency(DEP, [], [])
  assert.equal(status.state, 'missing')
  assert.equal(status.repository, undefined)
})

test('GUID 比较忽略大小写与空白', () => {
  const installed = [mod({ guid: '  COM.Example.DEP  ', version: '1.0.0' })]
  assert.equal(resolveDependency(DEP, installed, []).state, 'satisfied')
})

/*
 * 版本约束：已装但太旧仍然算不满足，并且要给「可安装」入口 ——
 * 用户需要的是升级到够新的版本，不是再装一遍。
 */
test('已装但低于 min_version → 仍要提示，并给可安装来源', () => {
  const dep = { guid: 'com.example.dep', min_version: '2.0.0' }
  const status = resolveDependency(dep, [mod({ guid: 'com.example.dep', version: '1.5.0' })], [repo()])
  assert.equal(status.state, 'installable')
  assert.equal(status.installedVersion, '1.5.0')
})

test('版本刚好达标 → satisfied', () => {
  const dep = { guid: 'com.example.dep', min_version: '2.0.0' }
  const status = resolveDependency(dep, [mod({ guid: 'com.example.dep', version: '2.0.0' })], [])
  assert.equal(status.state, 'satisfied')
})

test('本地没有版本号时，有版本要求就按不满足处理', () => {
  const dep = { guid: 'com.example.dep', min_version: '1.0.0' }
  const status = resolveDependency(dep, [mod({ guid: 'com.example.dep' })], [])
  assert.equal(status.state, 'missing')
  assert.equal(status.installedVersion, null)
})

/* 四段 = beta 序号：1.8.7.2 等价于 1.8.7-beta.2，两者都不满足 1.8.7。 */
test('版本比较沿用项目的四段 beta 语义', () => {
  const dep = { guid: 'com.example.dep', min_version: '1.8.7' }
  const beta = resolveDependency(dep, [mod({ guid: 'com.example.dep', version: '1.8.7.2' })], [])
  assert.equal(beta.state, 'missing', 'beta 低于同号正式版')
  const stable = resolveDependency(dep, [mod({ guid: 'com.example.dep', version: '1.8.7' })], [])
  assert.equal(stable.state, 'satisfied')
})

/* RAW 仓库不能用来满足依赖：它没有版本概念，装上也证明不了版本够。 */
test('RAW 模式仓库不作为可安装来源', () => {
  const raw = repo({ download_mode: 'repository_file' })
  assert.equal(resolveDependency(DEP, [], [raw]).state, 'missing')
})

/* 缓存里没有 Release 的仓库也不能装 —— 按钮点了会失败。 */
test('没有 Release 缓存的仓库不作为可安装来源', () => {
  const empty = repo({ cached_releases: [] })
  assert.equal(resolveDependency(DEP, [], [empty]).state, 'missing')
})

test('可选依赖不参与判定', () => {
  const owner = repo({ id: 'owner', requires: [{ guid: 'com.example.dep', optional: true }] })
  assert.deepEqual(resolveDependencies(owner, [], [repo()]), [])
})

test('必选依赖才参与判定，且排除仓库自身作为来源', () => {
  const owner = repo({ id: 'owner', plugin_guid: 'com.example.owner', requires: [DEP] })
  // 自身 GUID 与依赖不同、且列表里只有自己 → 没有可安装来源
  const statuses = resolveDependencies(owner, [], [owner])
  assert.equal(statuses.length, 1)
  assert.equal(statuses[0].state, 'missing')
})

test('自我依赖不会把自己当作来源（配置错误也不产生可点按钮）', () => {
  // 仓库声明依赖自己的 GUID
  const selfDependent = repo({ id: 'self', plugin_guid: 'com.example.self', requires: [{ guid: 'com.example.self' }] })
  const statuses = resolveDependencies(selfDependent, [], [selfDependent])
  assert.equal(statuses[0].state, 'missing')
})

test('firstUnmetDependency 取第一条未满足的', () => {
  const statuses = [
    { dependency: DEP, state: 'satisfied', installedVersion: '1.0.0' },
    { dependency: { guid: 'other' }, state: 'missing', installedVersion: null },
  ]
  assert.equal(firstUnmetDependency(statuses)?.dependency.guid, 'other')
  assert.equal(firstUnmetDependency([statuses[0]]), undefined)
})
