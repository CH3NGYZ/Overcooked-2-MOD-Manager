import assert from 'node:assert/strict'
import test from 'node:test'

/** 共享状态标签：真实源码直接执行（Node 24 类型擦除）。 */
const {
  STATUS_META, installedModTags, repositoryStatus, replaceVersionConclusion, isVersionConclusion,
  packageStatuses, summarizeUnits, modUpdateAvailable, modUpdateVerdict, repositoryInstallAction,
} = await import('../src/statusTags.ts')

function repo(overrides = {}) {
  return {
    id: 'repo-1', name: 'Repo', provider: 'github', username: 'Owner', repository: 'Repo',
    download_mode: 'release', branch: 'main', file_path: '', channel: 'all', asset_rules: [],
    install_target: '', note: '', plugin_guid: '', local_product_name: '', local_original_filename: '',
    version_regex: '', max_release_count: 10, download_threads: 4, enabled: true, builtin: false,
    cached_releases: [{ tag_name: 'v2.0.0', version: '2.0.0', name: '', body: '', prerelease: false, draft: false, assets: [] }],
    ...overrides,
  }
}

function mod(overrides = {}) {
  return {
    name: 'Mod', dll_file: 'Mod.dll', enabled: true, version_source: 'plugin',
    update_available: false, duplicate_guid: false, status: 'installed', labels: [], ...overrides,
  }
}

test('未安装仓库显示未安装，未刷新仓库显示未刷新', () => {
  assert.equal(repositoryStatus(repo(), []).text, 'overview.notInstalled')
  assert.equal(repositoryStatus(repo({ cached_releases: [] }), []).text, 'modsOnline.notRefreshed')
})

test('RAW 仓库只区分未安装 / 已安装，未安装为灰色', () => {
  const raw = repo({ download_mode: 'repository_file' })
  const notInstalled = repositoryStatus(raw, [])
  assert.equal(notInstalled.text, 'overview.notInstalled')
  assert.equal(notInstalled.tone, 'muted')

  const installed = mod({ repository_id: 'repo-1' })
  const done = repositoryStatus(raw, [installed])
  assert.equal(done.text, 'modsLocal.installed')
  // 已安装与最新版同档，使用主题色
  assert.equal(done.tone, 'accent')
})

test('RAW 仓库即使被禁用或刷新失败，也只按安装状态显示', () => {
  const installed = mod({ repository_id: 'repo-1' })
  assert.equal(repositoryStatus(repo({ download_mode: 'repository_file', enabled: false }), [installed]).text, 'modsLocal.installed')
  assert.equal(repositoryStatus(repo({ download_mode: 'repository_file', last_error: 'x' }), []).text, 'overview.notInstalled')
})

test('所有未安装类标签都是灰色', () => {
  /*
   * `text` 现在是 tag（`overview.notInstalled`），不再以「未」开头 ——
   * 断言中文前缀等于在断言某个具体语言的译文。这里改为断言两件真正重要
   * 的事：文案指向「未安装」这一族 tag，且色调是中性灰。
   */
  for (const key of ['not_installed', 'not_installed_raw', 'not_refreshed']) {
    assert.match(STATUS_META[key].text, /^[a-zA-Z]+\.[a-zA-Z]/, `${key} 的文案不是 tag`)
    assert.equal(STATUS_META[key].tone, 'muted', key)
  }
})

test('Release 仓库禁用或刷新失败时优先提示状态', () => {
  assert.equal(repositoryStatus(repo({ enabled: false }), []).text, 'modsOnline.disabled')
  assert.equal(repositoryStatus(repo({ last_error: 'x' }), []).text, 'modsOnline.repositoryRefreshFailed')
})

test('仓库刷新失败与已安装 MOD 的仓库刷新失败同文案同等级', () => {
  const fromRepository = repositoryStatus(repo({ last_error: 'x' }), [])
  const fromInstalledMod = STATUS_META.repository_unavailable
  assert.equal(fromRepository.text, fromInstalledMod.text)
  assert.equal(fromRepository.tone, fromInstalledMod.tone)
  // 中等偏上：需要用户注意并重试，但没到「同 GUID 冲突 / 加载失败」那一档
  assert.equal(fromRepository.tone, 'moderate')
})

test('本地比较版本与所选 Release 一致则显示最新版，落后则显示可更新', () => {
  const installed = mod({
    repository_id: 'repo-1', labels: ['latest'], version: '2.0.0',
    repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v2.0.0', channel: 'all' },
  })
  assert.equal(repositoryStatus(repo(), [installed]).text, 'modsLocal.latest')

  const outdated = { ...installed, version: '1.0.0' }
  assert.equal(repositoryStatus(repo(), [outdated]).text, 'tools.update')
})

test('后端未绑定该仓库时回退到版本比较', () => {
  const installed = mod({
    version: '1.0.0', repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v1.0.0', channel: 'all' },
  })
  const status = repositoryStatus(repo(), [installed])
  assert.equal(status.text, 'tools.update')
  // 可更新属于「中度」
  assert.equal(status.tone, 'moderate')
})

test('续接保留安装渠道，跳过草稿和预发布，并将精确标签用于新仓库更新', () => {
  const target = repo({ asset_rules: [{ type: 'glob', value: '*.zip' }] })
  const release = { ...target.cached_releases[0], assets: [{ name: 'Mod.ZIP', download_url: 'https://example.invalid/Mod.ZIP' }] }
  target.cached_releases = [{ ...release, tag_name: 'v4.0.0', draft: true },
    { ...release, tag_name: 'v3.0.0-beta.1', prerelease: true }, release]
  const installed = mod({ repository_id: 'old', replacement_repository_id: target.id, version: '1.0.0',
    repository_binding: { provider: 'gitee', owner: 'Owner', repository: 'Repo', tag: 'v1.0.0', channel: 'stable' } })
  assert.equal(repositoryStatus(target, [installed]).key, 'update_available')
  assert.equal(modUpdateVerdict(installed, [target]).updateAvailable, true)
  assert.equal(repositoryStatus(target, [installed]).replacement?.updateTag, 'v2.0.0')
  const allChannel = { ...installed, repository_binding: { ...installed.repository_binding, channel: 'all' } }
  assert.equal(repositoryStatus(target, [allChannel]).replacement?.updateTag, 'v2.0.0')
  assert.equal(repositoryStatus(target, [{ ...allChannel, prerelease: true }]).replacement?.updateTag, 'v3.0.0-beta.1')
  const betaTarget = { ...target, channel: 'beta' }
  assert.equal(repositoryStatus(betaTarget, [installed]).replacement?.updateTag, 'v3.0.0-beta.1')
  assert.equal(modUpdateVerdict(installed, [betaTarget]).tag, 'v3.0.0-beta.1')
  for (const pinned_version of ['missing', 'v3.0.0-beta.1']) {
    assert.deepEqual(repositoryStatus({ ...target, pinned_version }, [installed]).replacement, { updateTag: undefined })
  }
  for (const version of ['2.0.0', '5.0.0', 'nightly']) {
    const current = { ...installed, version, repository_binding: { ...installed.repository_binding, tag: version } }
    assert.equal(modUpdateAvailable(current, [target]), false)
    assert.deepEqual(repositoryStatus(target, [current]).replacement, { updateTag: undefined })
  }
  const unclaimed = { ...installed, replacement_repository_id: null }
  assert.equal(repositoryStatus(target, [unclaimed]).key, 'not_installed')
})

test('续接仅推荐首个规则命中的可下载 ZIP，不跳到第二个附件或其他版本', () => {
  const target = repo()
  const release = target.cached_releases[0]
  const zip = { name: 'Mod.zip', download_url: 'https://example.invalid/Mod.zip' }
  const dll = { name: 'Mod.dll', download_url: 'https://example.invalid/Mod.dll' }
  const allowed = { type: 'glob', value: 'Mod.*' }
  const installed = mod({ repository_id: 'old', replacement_repository_id: target.id, version: '1.0.0', update_available: true,
    repository_binding: { provider: 'gitee', owner: 'Owner', repository: 'Repo', tag: 'v1.0.0', channel: 'all' } })
  for (const { assets, asset_rules, pinned_version } of [
    { assets: [zip], asset_rules: [{ type: 'exact', value: 'Other.zip' }] },
    { assets: [dll, zip], asset_rules: [allowed] },
    { assets: [{ ...zip, download_url: '' }], asset_rules: [allowed] },
    { assets: [{ ...zip, download_url: ' ' }], asset_rules: [allowed] },
    { assets: [dll], asset_rules: [allowed], pinned_version: release.tag_name },
  ]) {
    const candidate = { ...target, asset_rules, pinned_version,
      cached_releases: [{ ...release, assets }, { ...release, tag_name: 'v1.5.0', assets: [zip] }] }
    assert.equal(modUpdateAvailable(installed, [candidate]), false)
    assert.equal(repositoryStatus(candidate, [installed]).key, 'unknown')
    assert.deepEqual(repositoryStatus(candidate, [installed]).replacement, { updateTag: undefined })
  }
  const skippedDll = { ...target, asset_rules: [{ type: 'exact', value: zip.name }], pinned_version: release.tag_name,
    cached_releases: [{ ...release, assets: [dll, zip] }] }
  assert.equal(repositoryStatus(skippedDll, [installed]).replacement?.updateTag, release.tag_name)
  const sameOrigin = mod({ repository_id: target.id, version: '1.0.0' })
  assert.equal(modUpdateAvailable(sameOrigin, [target]), true, '普通仓库维持现有版本判断')
})

test('每个状态标签都有文案与合法色调', () => {
  // 五档严重度 + 中性灰，外加为兼容旧快照保留的别名
  const tones = new Set([
    'accent', 'mild', 'moderate', 'severe', 'critical', 'muted',
    'ok', 'idle', 'warn', 'danger', 'violet',
  ])
  for (const [key, meta] of Object.entries(STATUS_META)) {
    assert.ok(meta.text, `${key} 缺少文案`)
    assert.ok(tones.has(meta.tone), `${key} 色调非法: ${meta.tone}`)
  }
})

test('installedModTags 保留后端顺序，labels 为空时退回 status', () => {
  const tags = installedModTags(mod({ labels: ['duplicate_lower_version', 'duplicate'] }))
  assert.deepEqual(tags.map((tag) => tag.text), ['tools.olderVersion', 'modsLocal.duplicate'])
  // 重复属重度，低版本属轻微
  assert.deepEqual(tags.map((tag) => tag.tone), ['mild', 'severe'])
  assert.equal(installedModTags(mod({ labels: [], status: 'unbound' }))[0].text, 'modsLocal.sourceUnconfirmed')
})

test('confirmed absence is a separate MOD label shared by the card, package and summary', () => {
  const core = mod({ labels: ['unlisted'], status: 'unbound' })
  const other = { ...core, dll_file: 'b.dll' }
  assert.deepEqual(installedModTags(core), [{ key: 'unlisted', text: 'modsLocal.unlisted', tone: 'muted' }])
  assert.deepEqual(replaceVersionConclusion(['unlisted', 'disabled'], 'latest'), ['latest', 'disabled'])
  assert.deepEqual(packageStatuses([core, other]).map(item => [item.key, item.count]), [['unlisted', undefined]])
  assert.deepEqual(packageStatuses([core, { ...other, labels: ['unbound'] }]).map(item => item.key), ['unbound'])
  assert.deepEqual(packageStatuses([core, { ...other, labels: ['latest'] }]).map(item => item.key), ['latest'])
  assert.deepEqual(packageStatuses([core, { ...other, labels: [] }]), [])
  assert.deepEqual(summarizeUnits([{ keys: ['unlisted'] }, { keys: ['unbound'] }]).map(x => [x.key, x.count]),
    [['unbound', 1], ['unlisted', 1]])
})

test('预发布版本在最前面插入「预发布」标签，与可更新同档', () => {
  const tags = installedModTags(mod({ labels: ['latest'], prerelease: true }))
  assert.deepEqual(tags.map((tag) => tag.text), ['modsOnline.preRelease', 'modsLocal.latest'])
  assert.equal(tags[0].tone, 'moderate')
  // 最新版用主题色
  assert.equal(tags[1].tone, 'accent')
})

test('非预发布版本不出现「预发布」标签', () => {
  const tags = installedModTags(mod({ labels: ['latest'], prerelease: false }))
  assert.deepEqual(tags.map((tag) => tag.text), ['modsLocal.latest'])
})

test('后端已带 prerelease 标签时不重复插入', () => {
  const tags = installedModTags(mod({ labels: ['prerelease', 'latest'], prerelease: true }))
  assert.equal(tags.filter((tag) => tag.key === 'prerelease').length, 1)
  assert.equal(tags[0].key, 'prerelease')
})

test('纯数字版本明确转正式清除单卡与包头的历史预发布标签', () => {
  const installed = mod({ version: '1.0.0.1', prerelease: false, labels: ['prerelease', 'latest'] })
  assert.equal(installedModTags(installed).some(tag => tag.key === 'prerelease'), false)
  assert.equal(packageStatuses([installed, { ...installed, dll_file: 'b.dll' }]).some(tag => tag.key === 'prerelease'), false)
})

test('显式 beta 后缀在历史标记为 false 时仍保留单卡和包头的预发布标签', () => {
  const installed = mod({ version: '2.0-beta.3', prerelease: false, labels: ['latest'] })
  assert.equal(installedModTags(installed).filter(tag => tag.key === 'prerelease').length, 1)
  assert.equal(packageStatuses([installed, { ...installed, dll_file: 'b.dll' }]).filter(tag => tag.key === 'prerelease').length, 1)
})

test('版本结论类标签互斥：替换后不残留旧结论', () => {
  // 之前只过滤 latest/update_available，会留下 repository_unavailable，
  // 于是出现「最新版 + 仓库不可用」这种矛盾组合。
  const stale = ['repository_unavailable', 'latest', 'disabled']
  const replaced = replaceVersionConclusion(stale, 'update_available')
  assert.deepEqual(replaced, ['update_available', 'disabled'])
  assert.equal(replaced.filter((label) => isVersionConclusion(label)).length, 1)
})

test('替换版本结论时保留非结论类标签', () => {
  const labels = ['unknown', 'duplicate', 'duplicate_lower_version', 'load_warning']
  const replaced = replaceVersionConclusion(labels, 'latest')
  assert.deepEqual(replaced, ['latest', 'duplicate', 'duplicate_lower_version', 'load_warning'])
})

test('传 null 只剥离旧结论，不追加新结论', () => {
  const replaced = replaceVersionConclusion(['not_release_asset', 'duplicate'], null)
  assert.deepEqual(replaced, ['duplicate'])
})

test('所有版本结论标签都登记在互斥组里', () => {
  // 防止以后新增结论标签却忘了加进 VERSION_CONCLUSION_LABELS
  const conclusions = [
    'latest', 'update_available', 'not_release_asset', 'installed',
    'unknown', 'unbound', 'bound_repository_missing', 'repository_unavailable',
  ]
  for (const label of conclusions) {
    assert.equal(isVersionConclusion(label), true, `${label} 应在互斥组内`)
  }
  // 非结论标签不能被误判为结论
  for (const label of ['duplicate', 'duplicate_lower_version', 'disabled', 'dependency', 'prerelease']) {
    assert.equal(isVersionConclusion(label), false, `${label} 不应在互斥组内`)
  }
})

test('刷新中与版本未知是两个独立语义', () => {
  // 刷新进行中 → 刷新中（还在查）；查完但判断不出 → 版本未知。
  // 混用会让刷新期间看起来已经有结论了。
  assert.notEqual(STATUS_META.refreshing.text, STATUS_META.unknown.text)
  assert.equal(STATUS_META.refreshing.text, 'tools.refreshing')
  assert.equal(STATUS_META.unknown.text, 'tools.versionUnknown')
  // 两者都是中性档：此刻既没有结论也没有需要动作的东西
  assert.equal(STATUS_META.refreshing.tone, 'muted')
  assert.equal(STATUS_META.unknown.tone, 'muted')
  // 都参与版本结论互斥，不能同时出现
  assert.equal(isVersionConclusion('refreshing'), true)
  assert.equal(isVersionConclusion('unknown'), true)
  const replaced = replaceVersionConclusion(['unknown', 'duplicate'], 'refreshing')
  assert.deepEqual(replaced, ['refreshing', 'duplicate'])
})

/* ===== MOD 包状态：以包为主体 ===== */

test('包内多个 DLL 同为最新版时只显示一条「最新版」，不带数量', () => {
  // 同一个 ZIP、同一个仓库、同一个 tag：包里只有一个版本结论可谈。
  // 页头那种「1 最新版」会让用户以为包内还有一个不是最新版的东西。
  const pkg = [
    mod({ dll_file: 'pkg/A.dll', labels: ['latest'], version: '1.0.0' }),
    mod({ dll_file: 'pkg/B.dll', labels: ['latest'], version: '1.0.0' }),
  ]
  const tags = packageStatuses(pkg)
  assert.equal(tags.length, 1)
  assert.equal(tags[0].key, 'latest')
  assert.equal(tags[0].text, STATUS_META.latest.text)
  assert.equal(tags[0].count, undefined)
})

test('包内有更新时整包只出一条「可更新」，不是「2 个可更新」', () => {
  const pkg = [
    mod({ dll_file: 'pkg/A.dll', labels: ['update_available'], update_available: true }),
    mod({ dll_file: 'pkg/B.dll', labels: ['update_available'], update_available: true }),
  ]
  const tags = packageStatuses(pkg)
  assert.equal(tags.length, 1)
  assert.equal(tags[0].key, 'update_available')
  assert.equal(tags[0].count, undefined)
})

test('包内状态不一致时取最需要注意的结论', () => {
  // 可更新（moderate）比刷新中（muted）更需要注意，应优先显示
  const tags = packageStatuses([
    mod({ dll_file: 'a.dll', labels: ['refreshing'] }),
    mod({ dll_file: 'b.dll', labels: ['update_available'], update_available: true }),
  ])
  assert.deepEqual(tags.map((t) => t.key), ['update_available'])
})

test('包内只有刷新中时显示「刷新中」，不再被最新版结论掩盖', () => {
  const tags = packageStatuses([mod({ dll_file: 'a.dll', labels: ['refreshing'] })])
  assert.deepEqual(tags.map((t) => t.key), ['refreshing'])
  assert.equal(tags[0].count, undefined)
})

test('包内只有版本未知时显示「版本未知」', () => {
  const tags = packageStatuses([mod({ dll_file: 'a.dll', labels: ['unknown'] })])
  assert.deepEqual(tags.map((t) => t.key), ['unknown'])
})

test('重复与低版本等成员级告警保留数量', () => {
  // 这类状态确实可能只命中包内某几个 DLL，数量有意义
  const tags = packageStatuses([
    mod({ dll_file: 'a.dll', labels: ['latest', 'duplicate'], duplicate_guid: true }),
    mod({ dll_file: 'b.dll', labels: ['latest'] }),
  ])
  const dup = tags.find((t) => t.key === 'duplicate')
  assert.equal(dup?.count, 1)
})

test('预发布整包只出一条，同样不带数量', () => {
  const tags = packageStatuses([
    mod({ dll_file: 'a.dll', labels: ['latest'], version: '1.0.0-beta.1' }),
    mod({ dll_file: 'b.dll', labels: ['latest'], version: '1.0.0-beta.1' }),
  ])
  const pre = tags.filter((t) => t.key === 'prerelease')
  assert.equal(pre.length, 1)
  assert.equal(pre[0].count, undefined)
})

test('纯依赖 DLL 不参与包状态判断', () => {
  const tags = packageStatuses([
    mod({ dll_file: 'a.dll', labels: ['latest'] }),
    mod({ dll_file: 'stub.dll', is_dependency: true, labels: ['dependency'] }),
  ])
  assert.deepEqual(tags.map((t) => t.key), ['latest'])
})

test('包内全是依赖 DLL 时不产生状态标签', () => {
  assert.deepEqual(packageStatuses([mod({ is_dependency: true, labels: ['dependency'] })]), [])
})

test('页头聚合按单元计数：两个独立 MOD 各计 1', () => {
  // 页头统计页面上的展示单元数量，单 MOD 卡片各算一个
  const tags = summarizeUnits([
    { keys: ['latest'] },
    { keys: ['latest'] },
  ])
  assert.equal(tags.length, 1)
  assert.equal(tags[0].key, 'latest')
  assert.equal(tags[0].count, 2)
})

test('一个 MOD 包无论含几个 DLL，都只作为 1 个单元计数', () => {
  /*
   * 这是本次修复的核心：两个 DLL 的最新版包在页头显示「1 最新版」，
   * 与页面上看到的卡片数一致；包内子 DLL 不对外公开状态。
   */
  const pkgStatuses = packageStatuses([
    mod({ dll_file: 'pkg/A.dll', labels: ['latest'] }),
    mod({ dll_file: 'pkg/B.dll', labels: ['latest'] }),
    mod({ dll_file: 'pkg/Stub.dll', is_dependency: true, labels: ['dependency'] }),
  ])
  const tags = summarizeUnits([{ keys: pkgStatuses.map((s) => s.key) }])
  assert.equal(tags.length, 1)
  assert.equal(tags[0].key, 'latest')
  assert.equal(tags[0].count, 1)
})

test('单元内重复的状态只计一次', () => {
  const tags = summarizeUnits([{ keys: ['duplicate', 'duplicate', 'latest'] }])
  const dup = tags.find((t) => t.key === 'duplicate')
  assert.equal(dup.count, 1)
})

test('多个包与单 MOD 混合时按单元分别计数', () => {
  const tags = summarizeUnits([
    { keys: ['latest'] },              // 单 MOD
    { keys: ['latest'] },              // 一个包（内部两个 DLL）
    { keys: ['update_available'] },    // 另一个包
  ])
  assert.equal(tags.find((t) => t.key === 'latest').count, 2)
  assert.equal(tags.find((t) => t.key === 'update_available').count, 1)
})

test('页头聚合排序仍按严重度：可更新排在最新版之前', () => {
  const tags = summarizeUnits([
    { keys: ['latest'] },
    { keys: ['update_available'] },
    { keys: ['load_warning'] },
  ])
  assert.deepEqual(tags.map((t) => t.key), ['load_warning', 'update_available', 'latest'])
})

/*
 * 总览页的「可更新」必须和 MOD 管理页用同一个判定。
 *
 * 真实 bug：后端扫描时 `update_available=false`（那时 Release 还是旧的），
 * 之后仓库刷新回来发现新版，MOD 管理页用 modUpdateVerdict 实时重算并
 * 显示「可更新」，而总览直接读后端快照，写着「全部为最新版」。
 *
 * `modUpdateAvailable` 的作用就是让总览优先采信重算结果。
 */
test('modUpdateAvailable 优先采信 Release 缓存的重算结果', () => {
  const stale = mod({
    repository_id: 'repo-1', update_available: false, labels: ['latest'], version: '1.0.0',
  })
  // 仓库缓存里有更新的 Release（2.0.0 > 1.0.0）
  assert.equal(modUpdateAvailable(stale, [repo()]), true, '快照说不更新，但缓存有新版')
})

test('无法重算时退回后端快照', () => {
  // 没绑定任何仓库 → modUpdateVerdict 返回 known:false → 用快照值
  const bound = mod({ repository_id: 'no-such-repo', update_available: true, labels: ['update_available'] })
  assert.equal(modUpdateAvailable(bound, [repo()]), true, '无仓库可重算时保留快照')
})

test('仓库刷新失败时不拿旧缓存下结论，退回快照', () => {
  const target = mod({ update_available: false, labels: ['latest'], version: '1.0.0' })
  const failed = repo({ last_error: '请求超时' })
  // repo.last_error 存在 → 无法重算 → 用快照（false）
  assert.equal(modUpdateAvailable(target, [failed]), false)
})

/*
 * modUpdateVerdict 是唯一判定入口，返回「能不能判定」而不只是结论。
 *
 * 需要这个区分的地方：MOD 管理页要把「仓库还在抓、暂时不知道」显示成
 * 「检查中」，而不是拿旧结论冒充当前状态。
 */
test('modUpdateVerdict 能判定时给出明确结论', async () => {
  const { modUpdateVerdict } = await import('../src/statusTags.ts')
  const stale = mod({ repository_id: 'repo-1', update_available: false, version: '1.0.0' })
  const verdict = modUpdateVerdict(stale, [repo()])
  assert.equal(verdict.known, true)
  assert.equal(verdict.updateAvailable, true, '缓存里 2.0.0 > 本地 1.0.0')
})

test('modUpdateVerdict 无法判定时标记为 known:false', async () => {
  const { modUpdateVerdict } = await import('../src/statusTags.ts')
  // 未绑定任何仓库
  const unbound = mod({ repository_id: 'no-such-repo', version: '1.0.0' })
  assert.equal(modUpdateVerdict(unbound, [repo()]).known, false)
  // 仓库刷新失败：旧缓存不可信
  const failed = mod({ repository_id: 'repo-1', version: '1.0.0' })
  assert.equal(modUpdateVerdict(failed, [repo({ last_error: '超时' })]).known, false)
  // 纯依赖不参与
  const dep = mod({ repository_id: 'repo-1', version: '1.0.0', is_dependency: true })
  assert.equal(modUpdateVerdict(dep, [repo()]).known, false)
})

/* ===== 包级依赖告警：与单 MOD 卡片同语义 ===== */

test('仓库声明依赖被卸载后，多 DLL 包标题显示「需处理」', () => {
  /*
   * 真实 bug：关系标签既不是版本结论、也不在成员级告警里，多 DLL 包会被
   * 整条丢弃 —— 同一个包的单 MOD 兄弟显示「需处理」，包标题却只有「最新版」。
   * 依赖声明挂在仓库上，整包共用一份，所以这是包级结论、不带数量。
   */
  const tags = packageStatuses([
    mod({ dll_file: 'pkg/Loader.dll', labels: ['relation_needs_action', 'latest'] }),
    mod({ dll_file: 'pkg/debugLog.dll', labels: ['latest'] }),
  ])
  assert.deepEqual(tags.map((t) => t.key), ['relation_needs_action', 'latest'])
  const dep = tags[0]
  assert.equal(dep.tone, 'critical')
  assert.equal(dep.count, undefined)
})

test('包级依赖标签与单 MOD 顺序一致：预发布 → 依赖 → 版本结论', () => {
  const tags = packageStatuses([
    mod({ dll_file: 'a.dll', labels: ['relation_needs_action', 'latest'], version: '1.0.0-beta.1' }),
    mod({ dll_file: 'b.dll', labels: ['latest'], version: '1.0.0-beta.1' }),
  ])
  assert.deepEqual(tags.map((t) => t.key), ['prerelease', 'relation_needs_action', 'latest'])
})

test('同包「需处理」优先于「未确认」，只显示一枚关系标签', () => {
  const tags = packageStatuses([
    mod({ dll_file: 'a.dll', labels: ['relation_unconfirmed', 'latest'] }),
    mod({ dll_file: 'b.dll', labels: ['relation_needs_action', 'latest'] }),
  ])
  assert.deepEqual(tags.map((t) => t.key), ['relation_needs_action', 'latest'])
})

test('不同成员的「需处理」不重复显示，保留独立的可更新结论', () => {
  const tags = packageStatuses([
    mod({ dll_file: 'Core.dll', labels: ['relation_needs_action', 'update_available'] }),
    mod({ dll_file: 'Companion.dll', release_companion: true, labels: ['relation_needs_action', 'latest'] }),
  ])
  assert.deepEqual(tags.map(tag => [tag.key, tag.text, tag.count]), [
    ['relation_needs_action', 'tools.relationNeedsAction', undefined],
    ['update_available', 'tools.update', undefined],
  ])
})

test('包标题在没有已确认问题时保留「未确认」，采用中等颜色', () => {
  const unresolved = packageStatuses([
    mod({ dll_file: 'pkg/A.dll', labels: ['relation_unconfirmed', 'latest'] }),
    mod({ dll_file: 'pkg/B.dll', labels: ['latest'] }),
  ])
  assert.deepEqual(unresolved.map((t) => t.key), ['relation_unconfirmed', 'latest'])
  assert.equal(unresolved[0].tone, 'moderate')
  assert.equal(unresolved[0].text, 'tools.relationUnconfirmed')
})

test('本地比较版本以扫描结果为准，明确未知时不回退清单', async () => {
  const { localVersion } = await import('../src/modStatus.ts')
  const binding = { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v2.0.0', channel: 'all' }
  const disk = mod({ version: '1.2.0', repository_binding: binding })
  assert.equal(localVersion(disk), '1.2.0')
  assert.equal(localVersion({ ...disk, update_version: 'v2.0.0' }), 'v2.0.0')
  assert.equal(localVersion({ ...disk, update_version: null }), undefined)
})

test('手动降级后，即使清单 tag 等于线上最新版，两页也显示可更新', () => {
  const downgraded = mod({
    repository_id: 'repo-1', version: '1.2.0', update_version: '1.2.0', update_available: false, labels: ['latest'],
    repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v2.0.0', channel: 'all' },
  })
  const upstream = repo()
  const status = repositoryStatus(upstream, [downgraded])
  assert.equal(status.key, 'update_available')
  assert.equal(repositoryInstallAction(status, upstream), 'update')
  assert.equal(modUpdateAvailable(downgraded, [upstream]), true)
})

test('附属 DLL 不参与选择核心版本；正常安装可用包的 tag 比较', () => {
  const upstream = repo()
  const binding = { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v2.0.0', channel: 'all' }
  const core = mod({ version: '1.2.0', update_version: '1.2.0', repository_binding: binding })
  const companion = mod({ version: '8.0.0', update_version: '8.0.0', repository_binding: binding, release_companion: true })
  assert.equal(repositoryStatus(upstream, [companion, core]).key, 'update_available')
  assert.equal(repositoryStatus(upstream, [companion]).key, 'unknown')
  const unchanged = { ...core, update_version: 'v2.0.0' }
  assert.equal(repositoryStatus(upstream, [companion, unchanged]).key, 'latest')
  assert.equal(modUpdateAvailable(unchanged, [upstream]), false)
})

test('手动升级到线上同一版本后不再提示更新', () => {
  const upgraded = mod({
    repository_id: 'repo-1', version: '2.0.0', update_available: false, labels: ['latest'],
    repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v1.0.0', channel: 'all' },
  })
  assert.equal(modUpdateAvailable(upgraded, [repo()]), false)
})

test('线上真的发布更高版本时仍然提示更新', () => {
  const older = mod({
    repository_id: 'repo-1', version: '1.2.0', update_available: false, labels: ['latest'],
    repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v1.0.0', channel: 'all' },
  })
  const newer = repo({
    cached_releases: [
      { tag_name: 'v3.0.0', version: '3.0.0', name: '', body: '', prerelease: false, draft: false, assets: [] },
    ],
  })
  assert.equal(modUpdateAvailable(older, [newer]), true)
})
