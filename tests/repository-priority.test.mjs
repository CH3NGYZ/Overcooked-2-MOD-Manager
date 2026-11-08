import assert from 'node:assert/strict'
import test from 'node:test'

/** 仓库刷新排队：已安装优先 + 去重。真实源码直接执行。 */
const { prioritizeInstalledModRepositories } = await import('../src/modRepositoryPriority.ts')

const repo = (id, extra = {}) => ({ id, name: id, enabled: true, download_mode: 'release', ...extra })
const mod = (extra = {}) => ({
  name: 'Mod', dll_file: 'BepInEx/plugins/Mod.dll', enabled: true,
  version_source: 'plugin', update_available: false, duplicate_guid: false,
  status: 'latest', repositories: [], ...extra,
})

test('已安装的仓库排到最前，其余保持原顺序', () => {
  const repos = [repo('a'), repo('b'), repo('c'), repo('d')]
  const mods = [mod({ repository_id: 'c' })]
  const ordered = prioritizeInstalledModRepositories(repos, mods)
  assert.deepEqual(ordered.map((r) => r.id), ['c', 'a', 'b', 'd'])
})

test('repository_ids 里任意命中都算已安装', () => {
  // 一条 MOD 可能登记多个归属仓库（多副本、多候选）
  const repos = [repo('a'), repo('b'), repo('c')]
  const mods = [mod({ repository_ids: ['b', 'c'] })]
  const ordered = prioritizeInstalledModRepositories(repos, mods)
  assert.deepEqual(ordered.map((r) => r.id), ['b', 'c', 'a'])
})

test('多个已安装 MOD 绑定同一仓库时只刷新一次', () => {
  // 同一仓库的多份副本 / 多个包共享一个仓库，不能重复排队
  const repos = [repo('a'), repo('b')]
  const mods = [
    mod({ repository_id: 'b', dll_file: 'BepInEx/plugins/One.dll' }),
    mod({ repository_id: 'b', dll_file: 'BepInEx/plugins/Two.dll' }),
    mod({ repository_ids: ['b'], repository_id: 'b', dll_file: 'BepInEx/plugins/Three.dll' }),
  ]
  const ordered = prioritizeInstalledModRepositories(repos, mods)
  assert.deepEqual(ordered.map((r) => r.id), ['b', 'a'])
  assert.equal(ordered.filter((r) => r.id === 'b').length, 1)
})

test('没有已安装 MOD 时保持原有顺序', () => {
  const repos = [repo('a'), repo('b'), repo('c')]
  const ordered = prioritizeInstalledModRepositories(repos, [])
  assert.deepEqual(ordered.map((r) => r.id), ['a', 'b', 'c'])
})

test('已安装但不在当前仓库列表里的 id 不影响结果', () => {
  // 仓库源被删掉后，MOD 清单里仍留着旧 id；不能因此丢仓库或乱序
  const repos = [repo('a'), repo('b')]
  const mods = [mod({ repository_id: 'gone' })]
  const ordered = prioritizeInstalledModRepositories(repos, mods)
  assert.deepEqual(ordered.map((r) => r.id), ['a', 'b'])
})

test('禁用仓库若被调用方传入，依然按已安装优先', () => {
  // 启用过滤由调用方负责；这个函数只管排序，不应偷偷改语义
  const repos = [repo('a'), repo('b', { enabled: false })]
  const mods = [mod({ repository_id: 'b' })]
  const ordered = prioritizeInstalledModRepositories(repos, mods)
  assert.deepEqual(ordered.map((r) => r.id), ['b', 'a'])
})
