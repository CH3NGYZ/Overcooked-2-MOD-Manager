import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * MOD 状态判定回归：MOD 安装页和 MOD 管理页共用 src/modStatus.ts。
 * Node 24 直接执行 TS（类型擦除），这里测的是真实源码，不是副本。
 */
const { compareVersions, repositoryForMod, modsForRepository, formatVersion, isPrereleaseVersion } =
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
  // 第 4 段是 beta 序号：2.0.0.1 ≡ 2.0.0-beta.1，低于正式版
  assert.equal(compareVersions('2.0.0.1', '2.0.0'), -1)
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

test('第 4 段是 beta 序号：序号间比较，且低于同号正式版', () => {
  // 真实场景：Release tag 是 0.4.0，DLL 里是 0.4.0.0。
  // 旧实现把第 4 段拼成预发布，导致已是最新的 MOD 永远显示「可更新」。
  assert.equal(compareVersions('0.4.0', '0.4.0.0'), 0)
  assert.equal(compareVersions('0.4', '0.4.0'), 0)
  assert.equal(compareVersions('v0.4', '0.4.0'), 0)
  // 第 4 段是 beta 序号：23 > 21（有更新），但都低于正式版
  assert.equal(compareVersions('1.8.7.23', '1.8.7.21'), 1)
  assert.equal(compareVersions('1.8.7.23', '1.8.7'), -1)
  assert.equal(compareVersions('0.4.0.5', '0.4.0'), -1)
  assert.equal(compareVersions('0.4.0', '0.4.0.0'), 0)
  // 只有显式 - 后缀才是预发布
  assert.equal(compareVersions('0.4.0-beta', '0.4.0'), -1)
})

test('formatVersion 只去尾部全 0 段，有数字的段一律保留', () => {
  assert.equal(formatVersion('0.4'), '0.4')
  assert.equal(formatVersion('0.4.0'), '0.4')
  assert.equal(formatVersion('0.4.0.0'), '0.4')
  // 第 4 段有实际数字：保留，必须能和 Release tag 对上
  assert.equal(formatVersion('1.8.7.23'), '1.8.7.23')
  assert.equal(formatVersion('0.4.0.5'), '0.4.0.5')
  assert.equal(formatVersion('v1.2.3'), 'v1.2.3')
  assert.equal(formatVersion('1.2.3-beta.1'), '1.2.3-beta.1')
  // 预发布保留原始数字段，能和 Release tag 逐字对上
  assert.equal(formatVersion('0.4.0.1-beta'), '0.4.0.1-beta')
  assert.equal(formatVersion('0.4.0.1-pre.01'), '0.4.0.1-pre.01')
  assert.equal(formatVersion('v0.4.0-beta.1'), 'v0.4-beta.1')
  // 至少保留两段，不把 1.0 压成 1
  assert.equal(formatVersion('1.0.0'), '1.0')
  assert.equal(formatVersion(''), '')
})

test('预发布判定：元数据标记与版本号后缀是或关系', () => {
  // 作者勾了 prerelease → 是
  assert.equal(isPrereleaseVersion('0.4.0', true), true)
  // 作者没勾，但版本号带 - 后缀 → 也是
  assert.equal(isPrereleaseVersion('0.4.0-beta', false), true)
  assert.equal(isPrereleaseVersion('0.4.0.5-beta', false), true)
  assert.equal(isPrereleaseVersion('0.4.0-rc.1', undefined), true)
  // 正式版（含尾部补 0 段）
  assert.equal(isPrereleaseVersion('0.4.0', false), false)
  assert.equal(isPrereleaseVersion('0.4.0.0', false), false)
  // 第 4 段非 0 即 beta 序号，算预发布
  assert.equal(isPrereleaseVersion('0.4.0.5', false), true)
  assert.equal(isPrereleaseVersion('1.8.7.23', false), true)
  assert.equal(isPrereleaseVersion('0.4', undefined), false)
  // 无法解析且没有标记 → 不臆断
  assert.equal(isPrereleaseVersion('unknown', false), false)
})

/*
 * 后端下发的 prerelease 是权威值，前端不得再用版本号推翻它。
 *
 * 真实事故：同一个 Release，仓库卡片读 tag_name（`v1.8.7.23`，四段）
 * 判成预发布，安装确认弹窗读 version（后端归一后的 `1.8.7`，第 4 段已丢）
 * 判成正式版，用户看到的两个结论相反。
 */
test('后端标记为预发布时，任何版本号写法都按预发布处理', () => {
  for (const version of ['1.8.7', '1.8.7.0', '1.8.7.23', 'unknown', '', undefined]) {
    assert.equal(isPrereleaseVersion(version, true), true, String(version))
  }
})

test('四段版本号读 tag_name 与读归一后的 version 结论应一致', () => {
  // 后端归一化会把 1.8.7.23 截成 1.8.7，所以只靠 version 反推会漏判；
  // 正确做法是用后端算好的 prerelease + 完整 tag_name。
  const tagName = 'v1.8.7.23'
  const normalized = '1.8.7'
  const backendFlag = true // 后端 is_prerelease_version("v1.8.7.23")
  assert.equal(isPrereleaseVersion(tagName, backendFlag), true)
  assert.equal(isPrereleaseVersion(normalized, backendFlag), true)
  // 若丢掉后端标记，两者就会分叉——这正是修复前的问题
  assert.equal(isPrereleaseVersion(tagName, false), true)
  assert.equal(isPrereleaseVersion(normalized, false), false)
})
