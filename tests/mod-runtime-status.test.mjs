import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 「卡片现在该显示什么状态」的判定（`src/modRuntimeStatus.ts`）。
 *
 * 这份判定原来写在 MOD 管理页里，只有页面自己能用；卡片标签下沉成组件后
 * 两边共用它。**必须锁住的第一条是优先级**：仓库通常已经带着上一轮的
 * Release 缓存，如果先按缓存出结论、刷新状态排在后面，整批刷新期间每张卡
 * 都还挂着「最新版」—— 用户点完「检查更新」看不到任何反应。
 */
const { runtimeModStatus, repositoryRefreshing } = await import('../src/modRuntimeStatus.ts')

const release = (version) => ({
  tag_name: `v${version}`, version, name: '', body: '',
  prerelease: false, draft: false, assets: [],
})

function repo(overrides = {}) {
  return {
    id: 'r1', name: 'r1', provider: 'github', username: 'u', repository: 'r1',
    download_mode: 'release', source_id: 'src-a', enabled: true,
    cached_releases: [release('1.0.0')], ...overrides,
  }
}

function mod(overrides = {}) {
  return {
    dll_file: 'BepInEx/plugins/A/A.dll', name: 'A', version: '1.0.0',
    repository_id: 'r1', enabled: true, labels: ['latest'],
    update_available: false, duplicate_guid: false, status: 'latest',
    is_dependency: false, ...overrides,
  }
}

const idle = { refreshing: {}, queued: {}, sourceRefreshing: {}, refreshingAll: false }

test('没有刷新时按缓存 Release 出结论', () => {
  const cold = runtimeModStatus(mod(), [mod()], [repo()], idle)
  assert.deepEqual(cold.mod.labels, ['latest'])
  assert.equal(cold.mod.update_available, false)
  assert.equal(cold.refreshing, false)

  const warm = runtimeModStatus(mod(), [mod()], [repo({ cached_releases: [release('2.0.0')] })], idle)
  assert.deepEqual(warm.mod.labels, ['update_available'])
  assert.equal(warm.mod.update_available, true)
})

test('刷新中优先于缓存结论（否则点检查更新界面毫无反应）', () => {
  const state = { refreshing: { r1: true }, queued: {}, sourceRefreshing: {}, refreshingAll: false }
  const result = runtimeModStatus(mod(), [mod()], [repo()], state)
  assert.deepEqual(result.mod.labels, ['refreshing'])
  assert.equal(result.refreshing, true)
  // 旧结论必须被替换掉，不能一边「刷新中」一边「最新版」
  assert.ok(!result.mod.labels.includes('latest'))
})

test('排队中（还没轮到）也算刷新中', () => {
  /*
   * 并发只有 4：20 个仓库里 16 个在排队。只把「正在飞」当刷新中的话，
   * 用户点一次「检查更新」只会看到 4 张卡变灰，剩下 16 张还挂着旧结论，
   * 看起来就像「只查了四个」。
   */
  const state = { refreshing: {}, queued: { r1: true }, sourceRefreshing: {}, refreshingAll: false }
  const result = runtimeModStatus(mod(), [mod()], [repo()], state)
  assert.deepEqual(result.mod.labels, ['refreshing'])
  assert.equal(result.refreshing, true)
})

test('源文档阶段（整批/整源）也算刷新中', () => {
  const all = { refreshing: {}, queued: {}, sourceRefreshing: {}, refreshingAll: true }
  assert.deepEqual(runtimeModStatus(mod(), [mod()], [repo()], all).mod.labels, ['refreshing'])

  // 源文档重拉时还没有按仓库发的请求，只能靠源级状态反映进度
  const source = { refreshing: {}, queued: {}, sourceRefreshing: { 'src-a': true }, refreshingAll: false }
  assert.deepEqual(runtimeModStatus(mod(), [mod()], [repo()], source).mod.labels, ['refreshing'])
})

test('无关的仓库在刷不影响这个 MOD', () => {
  const state = { refreshing: { 'r9': true }, queued: {}, sourceRefreshing: { 'src-z': true }, refreshingAll: false }
  assert.deepEqual(runtimeModStatus(mod(), [mod()], [repo()], state).mod.labels, ['latest'])
})

test('仓库文件（RAW）模式刷新时也不显示刷新中', () => {
  const raw = repo({ download_mode: 'repository_file', cached_releases: [] })
  const state = { refreshing: { r1: true }, queued: {}, sourceRefreshing: {}, refreshingAll: false }
  const result = runtimeModStatus(mod({ labels: ['installed'] }), [mod()], [raw], state)
  assert.deepEqual(result.mod.labels, ['installed'])
  assert.equal(repositoryRefreshing(raw, state), false)
})

test('关系标签与「刷新中」并存，且关系始终在最前', () => {
  // 仓库声明：A 依赖缺了 B —— 缺依赖与版本结论是两件事，刷新期间也要保留
  const withRelation = repo({
    plugin_guid: 'com.a',
    relations: [{ kind: 'requires', guid: 'com.b' }],
  })
  const target = mod({ guid: 'com.a' })
  const state = { refreshing: { r1: true }, queued: {}, sourceRefreshing: {}, refreshingAll: false }
  const result = runtimeModStatus(target, [target], [withRelation], state)
  assert.equal(result.mod.labels[0], 'missing_dependency')
  assert.ok(result.mod.labels.includes('refreshing'))
  assert.equal(result.issue?.state, 'missing')
})

test('未绑定仓库的 MOD 保持后端标签', () => {
  const result = runtimeModStatus(
    mod({ repository_id: undefined, labels: ['unbound'] }), [mod()], [], idle)
  assert.deepEqual(result.mod.labels, ['unbound'])
  assert.equal(result.refreshing, false)
})
