import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * MOD 状态判定回归：MOD 安装页和 MOD 管理页共用 src/modStatus.ts。
 * Node 24 直接执行 TS（类型擦除），这里测的是真实源码，不是副本。
 */
const {
  compareVersions, repositoryForMod, modsForRepository, formatVersion, isPrereleaseVersion, versionDisplay,
} = await import('../src/modStatus.ts')

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

/*
 * 本项目 beta 写法不统一，下面四种语义完全一致，都必须识别成预发布。
 *
 * 曾经只有 `-x.y` 一种生效：`1.8.7.beta4` 因为正则只找 `-`，
 * 整个版本号解析失败，既不判预发布也比较不了版本。
 */
test('四种 beta 写法统一识别为预发布', () => {
  for (const version of ['1.8.5-pre.01', '1.9.7-beta.4', '1.8.7-beta1', '1.8.7.beta4']) {
    assert.equal(isPrereleaseVersion(version, false), true, version)
  }
})

test('beta 序号之间按序号比较', () => {
  for (const [older, newer] of [
    ['1.8.7-beta1', '1.8.7-beta2'],
    ['1.9.7-beta.1', '1.9.7-beta.2'],
    ['1.8.7.beta1', '1.8.7.beta2'],
    ['1.8.5-pre.1', '1.8.5-pre.2'],
    ['1.8.7.1', '1.8.7.2'],
  ]) {
    assert.equal(compareVersions(newer, older), 1, `${newer} 应新于 ${older}`)
  }
})

/*
 * 紧凑写法 `-betaN` 必须按**数值**比较，否则第 10 个 beta 会静默失效。
 *
 * 严格 semver 里 `-beta1` 是单个字母数字标识符，按字典序比较，
 * 于是 `"beta10" < "beta9"`（'1' < '9'）——发到 beta10 就再也检测不到
 * 更新。规范推荐 `-beta.1` 正是为了避免这个。
 *
 * 本项目实际用紧凑写法，所以在解析层把 `beta1` 拆成 `beta` + `1`，
 * 与 `beta.1` 等价。
 */
test('紧凑写法 betaN 按数值比较，beta10 大于 beta9', () => {
  for (const [older, newer] of [
    ['1.8.8-beta9', '1.8.8-beta10'],
    ['1.8.8-beta10', '1.8.8-beta11'],
    ['1.8.8-beta99', '1.8.8-beta100'],
    ['1.8.8-rc9', '1.8.8-rc10'],
  ]) {
    assert.equal(compareVersions(newer, older), 1, `${newer} 应新于 ${older}`)
  }
})

test('beta1 与 beta.1 是同一个预发布序号', () => {
  for (const [compact, dotted] of [
    ['1.8.8-beta1', '1.8.8-beta.1'],
    ['1.8.8-beta10', '1.8.8-beta.10'],
  ]) {
    assert.equal(compareVersions(compact, dotted), 0, `${compact} 应等于 ${dotted}`)
  }
})

/*
 * semver 2.0.0 §11 给出的经典优先级链，逐条必须成立。
 * 这是「全盘接受标准语义」的验收基准。
 */
test('符合 semver 规范给出的经典优先级链', () => {
  const chain = [
    '1.0.0-alpha', '1.0.0-alpha.1', '1.0.0-alpha.beta', '1.0.0-beta',
    '1.0.0-beta.2', '1.0.0-beta.11', '1.0.0-rc.1', '1.0.0',
  ]
  for (let index = 0; index < chain.length - 1; index += 1) {
    assert.equal(
      compareVersions(chain[index + 1], chain[index]),
      1,
      `${chain[index]} 应低于 ${chain[index + 1]}`,
    )
  }
})

/*
 * 现实约束：.NET 程序集版本最多四段纯数字，`1.9.8-beta.1` 无法写进 DLL。
 *
 * 所以 HostUtilities 的发布模型是「DLL 用四段数字，GitHub tag 用标准 semver」：
 *
 *     DLL version  1.9.8.1
 *     release tag  v1.9.8-beta.1
 *
 * 安装器读取本地版本时，必须把第 4 段当作 `-beta.<n>` 参与线上 tag 对比，
 * 否则「装了 beta.1，仓库有 beta.2」检测不出来，或者反过来把同一个 beta
 * 反复提示为更新。
 */
test('DLL 四段版本与仓库 beta tag 语义等价', () => {
  for (const serial of [1, 2, 9, 10, 21, 23]) {
    const dll = `1.9.8.${serial}`
    const tag = `v1.9.8-beta.${serial}`
    assert.equal(compareVersions(dll, tag), 0, `${dll} 应等于 ${tag}`)
  }
})

test('本地四段 DLL 能检测到线上 beta 的更新', () => {
  // 装了 beta.1，仓库有 beta.2 → 有更新
  assert.equal(compareVersions('v1.9.8-beta.2', '1.9.8.1'), 1)
  // 装了 beta.2，仓库最新仍是 beta.1 → 已是最新
  assert.equal(compareVersions('v1.9.8-beta.1', '1.9.8.2'), -1)
  // 序号跨十位仍正确（旧实现会把 .21 判得比 .1 小）
  assert.equal(compareVersions('v1.9.8-beta.21', '1.9.8.1'), 1)
  assert.equal(compareVersions('1.9.8.21', '1.9.8.1'), 1)
})

test('本地装 beta 时能检测到同基版本的正式版更新', () => {
  // HostUtilities 已开启「同基版本 beta 低于正式版」，且 beta 仓库也会带
  // stable 的 Release，因此装了 1.9.8.1 应能升级到 1.9.8
  assert.equal(compareVersions('v1.9.8', '1.9.8.1'), 1)
  // 下一条产品线的 beta 同样算更新
  assert.equal(compareVersions('v1.9.9-beta.1', '1.9.8.1'), 1)
  // 上一个正式版不算更新
  assert.equal(compareVersions('v1.9.7', '1.9.8.1'), -1)
})

/*
 * 同号 beta 永远低于正式版 —— 这是「稳定渠道不接受 beta 更新」的依据。
 * 本地 1.8.7，仓库有 1.8.7.23 时不能提示有更新。
 */
test('同号 beta 低于正式版，下一个次版本的 beta 才是更新', () => {
  for (const beta of ['1.8.7-beta.1', '1.8.7-beta.23', '1.8.7-beta1', '1.8.7.beta4', '1.8.7.23']) {
    assert.equal(compareVersions(beta, '1.8.7'), -1, `${beta} 应低于 1.8.7`)
    assert.equal(compareVersions('1.8.7', beta), 1, `1.8.7 应高于 ${beta}`)
  }
  // host utilities 的新发布约定：下一个测试版用下一个正式版号 + beta 标志
  assert.equal(compareVersions('1.8.8-beta1', '1.8.7'), 1)
})

/*
 * 版本号两段式展示：主干 + 一个测试标记。
 *
 * 卡片上的 `v1.9.9-beta.2` 占位太宽，用户实际只需要「哪个版本 + 第几个测试」。
 */
test('versionDisplay 把 -beta.N 拆成 主干 + 数字序号', () => {

  for (const tag of ['v1.9.9-beta.2', '1.9.9-beta.2', '1.9.9.2']) {
    const d = versionDisplay(tag, false, '测')
    assert.equal(d.base, '1.9.9', tag)
    assert.equal(d.marker, '2', tag)
    assert.equal(d.markerIsNumeric, true, tag)
  }
})

test('versionDisplay 对正式版不给标记', () => {

  for (const tag of ['v1.9.9', '1.9.9', '1.9.9.0']) {
    const d = versionDisplay(tag, false, '测')
    assert.equal(d.base, '1.9.9', tag)
    assert.equal(d.marker, null, tag)
  }
})

test('versionDisplay 无数字序号时用文字标记提示测试版', () => {

  // 只有文字标识
  const beta = versionDisplay('1.9.9-beta', false, '测')
  assert.equal(beta.base, '1.9.9')
  assert.equal(beta.markerIsNumeric, false)
  assert.ok(beta.marker)
  // 后端标记 prerelease 但 tag 是纯正式版号 → 同样要提示
  const flagged = versionDisplay('v1.9.9', true, '测')
  assert.equal(flagged.base, '1.9.9')
  assert.equal(flagged.marker, '测')
  assert.equal(flagged.markerIsNumeric, false)
})

test('versionDisplay 主干去掉 v 前缀与第 4 段', () => {

  assert.equal(versionDisplay('v1.9.9.23', false, '测').base, '1.9.9')
  assert.equal(versionDisplay('v1.9.9.23', false, '测').marker, '23')
})
