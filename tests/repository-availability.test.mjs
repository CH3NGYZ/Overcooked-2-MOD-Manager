import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 仓库「是否存在」：地区与 enabled 共同决定，且必须与后端一致。
 * 真实源码直接执行（不 import i18n / naive-ui）。
 */
const {
  OFFICIAL_INDEX_ID,
  repositoryIsAvailable,
  repositoryRegionMatches,
  sourceMatchesRegion,
  shouldFallbackToBuiltin,
} = await import('../src/repositoryAvailability.ts')

const sources = [
  { id: 'official', region: 'chinese_mainland' },
  { id: 'intl-source', region: 'international' },
  { id: 'any-source', region: undefined },
  { id: 'gitee-community', region: 'chinese_mainland' },
]

/** 默认所有属性都「可用」，逐项覆盖。 */
const repo = (extra = {}) => ({
  enabled: true,
  region: undefined,
  source_id: 'any-source',
  ...extra,
})

test('enabled=false 在两种地区都不可用', () => {
  for (const region of ['chinese_mainland', 'international']) {
    assert.equal(repositoryIsAvailable(repo({ enabled: false }), sources, region), false, region)
  }
})

test('international 仓库在中国大陆不可用', () => {
  const intl = repo({ region: 'international' })
  assert.equal(repositoryIsAvailable(intl, sources, 'chinese_mainland'), false)
  assert.equal(repositoryIsAvailable(intl, sources, 'international'), true)
})

test('chinese_mainland 仓库在国际地区不可用', () => {
  const cn = repo({ region: 'chinese_mainland' })
  assert.equal(repositoryIsAvailable(cn, sources, 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(cn, sources, 'international'), false)
})

test('地区为空的仓库在两种地区都可用', () => {
  const any = repo()
  assert.equal(repositoryIsAvailable(any, sources, 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(any, sources, 'international'), true)
})

test('所属来源地区不适用时仓库也不可用', () => {
  // 仓库自身没写地区，但它挂在一个 international 子源下
  const child = repo({ source_id: 'intl-source' })
  assert.equal(repositoryIsAvailable(child, sources, 'chinese_mainland'), false)
  assert.equal(repositoryIsAvailable(child, sources, 'international'), true)
})

test('来源已从配置删除时仓库不可用（与后端一致）', () => {
  assert.equal(repositoryIsAvailable(repo({ source_id: 'gone' }), sources, 'chinese_mainland'), false)
})

test('社区源不限地区：Gitee 社区在国际地区同样可用', () => {
  /*
   * 两个社区源默认 region 为空，因此对所有地区可用。
   * 它们与官方源的关系由后端按身份闪避（官方已收录的副本 enabled=false），
   * 不在地区维度表达。
   */
  const gitee = repo({ region: undefined, source_id: 'gitee-community' })
  const github = repo({ region: undefined, source_id: 'github-community' })
  const sources = [
    { id: 'gitee-community', region: undefined },
    { id: 'github-community', region: undefined },
  ]
  for (const region of ['chinese_mainland', 'international', undefined]) {
    assert.equal(repositoryIsAvailable(gitee, sources, region), true, `gitee ${region}`)
    assert.equal(repositoryIsAvailable(github, sources, region), true, `github ${region}`)
  }
})

test('没有 source_id 的仓库只看 enabled 与自身地区', () => {
  const standalone = repo({ source_id: undefined })
  assert.equal(repositoryIsAvailable(standalone, sources, 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(standalone, sources, 'international'), true)
  assert.equal(repositoryIsAvailable({ ...standalone, enabled: false }, sources, 'international'), false)
})

test('旧地区别名按归一化后的值判断', () => {
  assert.equal(repositoryRegionMatches('other', 'international'), true)
  assert.equal(repositoryRegionMatches('other', 'chinese_mainland'), false)
  assert.equal(repositoryRegionMatches('mainland-china', 'chinese_mainland'), true)
  assert.equal(sourceMatchesRegion({ region: 'other' }, 'chinese_mainland'), false)
})

test('当前地区为空按「非中国大陆」处理，与后端 current_user_region 一致', () => {
  const intl = repo({ region: 'international' })
  assert.equal(repositoryIsAvailable(intl, sources, undefined), true)
  assert.equal(repositoryIsAvailable(repo({ region: 'chinese_mainland' }), sources, undefined), false)
})

/* ===== 内置源回退判定 ===== */

/** 官方根 + 两个子源 + 一个自定义源。 */
const fallbackSources = [
  { id: OFFICIAL_INDEX_ID, nickname: '官方仓库源', parent_id: undefined },
  { id: 'official-child-a', nickname: '官方子源 A', parent_id: OFFICIAL_INDEX_ID },
  { id: 'official-child-b', nickname: '官方子源 B', parent_id: OFFICIAL_INDEX_ID },
  { id: 'custom', nickname: '自定义源', parent_id: undefined },
]
const officialError = '官方仓库源: connection refused'

test('官方源全部失败且树上没有仓库时回退内置源', () => {
  assert.equal(shouldFallbackToBuiltin(fallbackSources, [], [officialError]), true)
})

test('没有失败时不回退', () => {
  assert.equal(shouldFallbackToBuiltin(fallbackSources, [], []), false)
})

test('失败没有覆盖到官方根源时不回退', () => {
  // 只有自定义源失败：官方树可能是好的，不该拿内置内容顶替
  assert.equal(shouldFallbackToBuiltin(fallbackSources, [], ['自定义源: timeout']), false)
})

test('官方子源成功带回仓库时不回退（在线可用）', () => {
  // 这是最关键的一条：只要官方树里还有仓库，就不能用落后几版的内置快照覆盖
  const repos = [{ source_id: 'official-child-a' }]
  assert.equal(shouldFallbackToBuiltin(fallbackSources, repos, [officialError]), false)
})

test('自定义源下的仓库不算官方在线可用', () => {
  // 自定义源有内容不代表官方源可用，仍应回退
  const repos = [{ source_id: 'custom' }]
  assert.equal(shouldFallbackToBuiltin(fallbackSources, repos, [officialError]), true)
})

test('孙子代子源带回的仓库同样算官方在线可用', () => {
  const nested = [
    ...fallbackSources,
    { id: 'grandchild', nickname: '孙子源', parent_id: 'official-child-a' },
  ]
  const repos = [{ source_id: 'grandchild' }]
  assert.equal(shouldFallbackToBuiltin(nested, repos, [officialError]), false)
})

test('没有官方根源时不回退', () => {
  const noRoot = [{ id: 'custom', nickname: '自定义源', parent_id: undefined }]
  assert.equal(shouldFallbackToBuiltin(noRoot, [], [officialError]), false)
})

test('官方树里只有无归属仓库时仍然回退', () => {
  // source_id 为空（手动安装的遗留条目）不属于任何官方源
  assert.equal(shouldFallbackToBuiltin(fallbackSources, [{ source_id: undefined }], [officialError]), true)
})

/* ===== 昵称多语言：回退前缀必须与报错前缀同源 ===== */

/*
 * 官方源昵称改成只认 nickname_i18n 之后，报错前缀（sourceError）和回退
 * 判定（shouldFallbackToBuiltin）必须取自同一个函数。否则英文/繁体界面下
 * 报错写的是翻译值、判定比的是 zh-CN 镜像值，前缀对不上，回退被静默跳过。
 */
const localizedSources = [{
  id: OFFICIAL_INDEX_ID,
  nickname: '官方仓库源',
  nickname_i18n: {
    'zh-CN': '官方仓库源',
    en: 'Official repositories',
  },
  parent_id: undefined,
}]

test('回退判定按当前语言匹配报错前缀', () => {
  assert.equal(
    shouldFallbackToBuiltin(localizedSources, [], ['Official repositories: timeout'], 'en'),
    true,
  )
  // 英文界面下，用中文前缀的错误不算命中（文案确实不是它产生的）
  assert.equal(
    shouldFallbackToBuiltin(localizedSources, [], ['官方仓库源: timeout'], 'en'),
    false,
  )
})

test('不传语言时按 zh-CN 兜底匹配', () => {
  assert.equal(
    shouldFallbackToBuiltin(localizedSources, [], ['官方仓库源: timeout']),
    true,
  )
})

test('没有翻译表时退回单语镜像值', () => {
  const legacy = [{ id: OFFICIAL_INDEX_ID, nickname: '官方仓库源', parent_id: undefined }]
  assert.equal(shouldFallbackToBuiltin(legacy, [], ['官方仓库源: timeout']), true)
})

/* ===== 平台限定（与 region 同语义层）===== */

const { repositoryPlatformMatches, normalizeRepositoryPlatform } = await import(
  '../src/repositoryAvailability.ts'
)

test('平台为空或缺失时全平台可用', () => {
  assert.equal(repositoryPlatformMatches(undefined, 'windows'), true)
  assert.equal(repositoryPlatformMatches('', 'windows'), true)
  assert.equal(repositoryPlatformMatches('  ', 'macos'), true)
  assert.equal(normalizeRepositoryPlatform(undefined), '')
})

test('平台限定只对匹配的平台可用', () => {
  assert.equal(repositoryPlatformMatches('windows', 'windows'), true)
  assert.equal(repositoryPlatformMatches('windows', 'macos'), false)
  assert.equal(repositoryPlatformMatches('macos', 'macos'), true)
  assert.equal(repositoryPlatformMatches('macos', 'windows'), false)
})

test('平台别名归一化（win / mac / osx / darwin）', () => {
  assert.equal(normalizeRepositoryPlatform('win'), 'windows')
  assert.equal(normalizeRepositoryPlatform('WIN'), 'windows')
  assert.equal(normalizeRepositoryPlatform('mac'), 'macos')
  assert.equal(normalizeRepositoryPlatform('osx'), 'macos')
  assert.equal(normalizeRepositoryPlatform('darwin'), 'macos')
  // linux 归入 windows（本项目 Linux 走 Wine 跑 Windows 版 MOD），
  // 见下面单独的测试；这里只验真正无法识别的值
  assert.equal(normalizeRepositoryPlatform('freebsd'), '')
})

test('repositoryIsAvailable 把平台不适用当作「不存在」', () => {
  const macOnly = { enabled: true, region: undefined, source_id: undefined, platform: 'macos' }
  assert.equal(repositoryIsAvailable(macOnly, undefined, undefined, 'macos'), true)
  assert.equal(repositoryIsAvailable(macOnly, undefined, undefined, 'windows'), false)
  /*
   * 不传平台时按本机构建目标判断，而不是跳过过滤。
   * 「忘了传参数」若等于「平台限定失效」，那种失效没有任何报错，
   * 只会让本该隐藏的仓库悄悄出现在别的平台列表里。
   * Node 下没有 navigator，回落到 windows，所以 macOS 专属仓库不可用。
   */
  assert.equal(repositoryIsAvailable(macOnly, undefined, undefined), false)
})

/* 内置源：只有 MacosControllerFix 限定 macOS，其余全平台 */
test('内置仓库源里只有 macOS 手柄修复限定了平台', async () => {
  const { readFileSync, readdirSync } = await import('node:fs')
  const dir = new URL('../src-tauri/resources/', import.meta.url)
  const files = readdirSync(dir).filter((name) =>
    name.startsWith('repositories-') && name.endsWith('.toml') && name !== 'repositories-proxies.toml')

  let totalRepos = 0
  let totalPlatformLines = 0
  const macOnly = []
  for (const file of files) {
    const text = readFileSync(new URL(file, dir), 'utf8').replace(/\r\n/g, '\n')
    // 按 [[repositories]] 切块，逐块检查
    const blocks = text.split(/^\[\[repositories\]\]$/m).slice(1)
    for (const block of blocks) {
      totalRepos += 1
      const platform = block.match(/^platform = "(.*)"$/m)
      if (platform) totalPlatformLines += 1
      if (platform && platform[1].trim() === 'macos') {
        const repo = block.match(/^repository = "(.*)"$/m)
        macOnly.push(repo ? repo[1] : '(unknown)')
      }
      // 不允许出现其它平台限定
      if (platform && platform[1].trim() && platform[1].trim() !== 'macos') {
        assert.fail(`${file} 出现非预期平台限定：${platform[1]}`)
      }
    }
  }

  assert.equal(totalRepos, totalPlatformLines, '每个仓库都应有 platform 字段')
  assert.deepEqual(macOnly, ['Overcooked-2-MacosControllerFix'], '只有 macOS 手柄修复限定 macOS')
})

/* Linux 归入 windows：本项目 Linux 走 Wine 跑 Windows 版 MOD。
 * 不归一会导致「PublicConfig.platform = linux」被当成无法识别，
 * 连平台上限定 windows 的仓库都被过滤掉。 */
test('linux 平台归入 windows', () => {
  assert.equal(normalizeRepositoryPlatform('linux'), 'windows')
  assert.equal(repositoryPlatformMatches('windows', 'linux'), true)
  assert.equal(repositoryPlatformMatches('macos', 'linux'), false)
})
