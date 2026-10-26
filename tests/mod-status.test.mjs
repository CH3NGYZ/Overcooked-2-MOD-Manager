import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * MOD 状态判定回归：MOD 安装页和 MOD 管理页共用 src/modStatus.ts。
 * Node 24 直接执行 TS（类型擦除），这里测的是真实源码，不是副本。
 */
const { compareVersions, repositoryForMod, modsForRepository } =
  await import('../src/modStatus.ts')

function repo(overrides = {}) {
  return {
    id: 'repo-1', name: 'Repo', provider: 'github', username: 'Owner', repository: 'Repo',
    download_mode: 'release', branch: 'main', file_path: '', channel: 'all', asset_rules: [],
    install_target: '', note: '', plugin_guid: '', local_product_name: '', local_original_filename: '',
    version_regex: '', max_release_count: 10, download_threads: 4, enabled: true, builtin: false,
    cached_releases: [], ...overrides,
  }
}

function mod(overrides = {}) {
  return {
    name: 'Mod', dll_file: 'Mod.dll', enabled: true, version_source: 'plugin', update_available: false,
    duplicate_guid: false, status: 'latest', ...overrides,
  }
}

test('版本比较覆盖 v 前缀、段数差异、预发布', () => {
  assert.equal(compareVersions('v1.2.3', '1.2.3'), 0)
  assert.equal(compareVersions('1.2', '1.2.0'), 0)
  assert.equal(compareVersions('1.10.0', '1.9.9'), 1)
  assert.equal(compareVersions('1.2.3-beta.2', '1.2.3-beta.1'), 1)
  assert.equal(compareVersions('1.2.3', '1.2.3-rc.1'), 1)
  assert.equal(compareVersions('2.0.0.1', '2.0.0'), 1)
  assert.equal(compareVersions('unknown', '1.0.0'), null)
  assert.equal(compareVersions(undefined, '1.0.0'), null)
})

test('安装清单优先用 repository_id 匹配仓库，身份字段兜底', () => {
  const target = repo()
  assert.equal(repositoryForMod([target], mod({ repository_id: 'repo-1' }))?.id, 'repo-1')
  assert.equal(repositoryForMod([target], mod({
    repository_binding: { provider: 'github', owner: 'owner', repository: 'repo', tag: 'v1.0.0', channel: 'all' },
  }))?.id, 'repo-1')
  assert.equal(repositoryForMod([target], mod()), undefined)
})

test('同一仓库的多个本地副本全部返回，便于取最高版本比较', () => {
  const target = repo()
  const list = [
    mod({ dll_file: 'A.dll', repository_id: 'repo-1', version: '1.0.0' }),
    mod({ dll_file: 'B.dll', repository_id: 'repo-1', version: '2.0.0' }),
    mod({ dll_file: 'C.dll', repository_id: 'other' }),
  ]
  assert.deepEqual(modsForRepository(list, target).map((item) => item.dll_file), ['A.dll', 'B.dll'])
})
