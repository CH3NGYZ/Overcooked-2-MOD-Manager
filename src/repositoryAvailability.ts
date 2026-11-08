import type { Repository, RepositorySource } from './types'

/**
 * 仓库「在当前地区是否存在」的纯函数。
 *
 * 单独成模块（不 import i18n / naive-ui）有两个原因：
 *   1. 安装页、刷新引擎、MOD 管理页都要用同一套判定，不能各写一份；
 *   2. 直接给 Node 测试引用，避免把浏览器环境依赖带进测试进程。
 *
 * 判定必须与后端 `config::repository_matches_channel` + `enabled` 保持一致，
 * 否则会出现「前端排队刷新 → 后端拒绝」这类只在日志里可见的假失败。
 */

/** 地区值归一化：兼容旧别名，空值表示不限地区。 */
export function normalizeRepositoryRegion(region: string | undefined): string {
  const value = region?.trim().toLowerCase()
  if (value === 'mainland-china') return 'chinese_mainland'
  if (value === 'other') return 'international'
  return value || ''
}

/** 仓库/源是否适用于当前下载渠道。空地区适用全部。 */
export function repositoryRegionMatches(region: string | undefined, currentRegion: string | undefined): boolean {
  const value = normalizeRepositoryRegion(region)
  if (value === 'chinese_mainland') return currentRegion === 'chinese_mainland'
  if (value === 'international') return currentRegion !== 'chinese_mainland'
  return true
}

export function repositoryMatchesRegion(repo: Pick<Repository, 'region'>, currentRegion: string | undefined): boolean {
  return repositoryRegionMatches(repo.region, currentRegion)
}

export function sourceMatchesRegion(source: Pick<RepositorySource, 'region'>, currentRegion: string | undefined): boolean {
  return repositoryRegionMatches(source.region, currentRegion)
}

/**
 * 归一化仓库的平台字段。
 *
 * 空值 / 缺失 / 无法识别都返回 `''`（全平台可用）——与后端
 * `config::normalize_repository_platform` 保持一致。
 *
 * 合法取值只有 `windows` 与 `macos`（本项目只发布这两个平台）。
 * 别的值（包括旧配置里的 `linux`）在后端导入时就会被拒绝，
 * 前端这里按「无法识别」处理，不会把它误判成某个平台。
 */
export function normalizeRepositoryPlatform(platform: string | undefined): string {
  const value = platform?.trim().toLowerCase()
  if (!value) return ''
  if (value === 'windows' || value === 'win') return 'windows'
  if (value === 'macos' || value === 'mac' || value === 'osx' || value === 'darwin') return 'macos'
  return ''
}

/** 仓库是否适用于当前平台。空平台表示全平台可用。 */
export function repositoryPlatformMatches(
  platform: string | undefined,
  currentPlatform: string | undefined,
): boolean {
  const required = normalizeRepositoryPlatform(platform)
  if (!required) return true
  return required === normalizeRepositoryPlatform(currentPlatform)
}

/** 仓库所属来源；来源已从配置中删除时返回 undefined。 */
export function repositorySource(
  repo: Pick<Repository, 'source_id'>,
  sources: Pick<RepositorySource, 'id' | 'region'>[] | undefined,
): Pick<RepositorySource, 'id' | 'region'> | undefined {
  if (!repo.source_id) return undefined
  return (sources ?? []).find((entry) => entry.id === repo.source_id)
}

/**
 * 仓库在当前地区是否真实存在。
 *
 * 「不存在」是比「隐藏」更强的语义：它既不显示卡片，也不参与刷新排队、
 * 不产生更新状态，前端更不会为它发起任何请求。以下任一条件成立即不存在：
 *   - `enabled=false`（用户关闭，或被官方仓库源覆盖的社区副本）；
 *   - 仓库自身地区不适用于当前地区（例如 international 仓库在中国大陆）；
 *   - 所属来源地区不适用于当前地区，或来源已被删除（与后端一致）。
 *
 * 社区发现暂停期间，三平台的发现来源均不可用；保留旧配置以便之后恢复。
 */
export function repositoryIsAvailable(
  repo: Pick<Repository, 'enabled' | 'region' | 'source_id' | 'platform'>,
  sources: Pick<RepositorySource, 'id' | 'region'>[] | undefined,
  currentRegion: string | undefined,
  currentPlatform?: string,
): boolean {
  if (repo.source_id && ['github-community', 'gitee-community', 'gitcode-community'].includes(repo.source_id)) return false

  if (!repo.enabled) return false
  if (!repositoryRegionMatches(repo.region, currentRegion)) return false
  /*
   * 平台不适用与地区不适用同义：仓库在当前环境「不存在」。
   *
   * 不传 `currentPlatform` 时按**本机构建目标**判断，而不是跳过过滤 ——
   * 「忘了传参数」不该变成「平台限定失效」：那样的失效没有任何报错，
   * 只会让本该隐藏的仓库悄悄出现在别的平台列表里。
   */
  const platform = normalizeRepositoryPlatform(currentPlatform) || buildPlatform()
  if (!repositoryPlatformMatches(repo.platform, platform)) return false
  if (!repo.source_id) return true
  const source = repositorySource(repo, sources)
  return !!source && sourceMatchesRegion(source, currentRegion)
}

/**
 * 前端运行平台（编译期常量）。
 *
 * 与后端 `config::current_platform` 一致：只区分 macOS 与 Windows。
 *
 * 平台来源是 `navigator.platform`：Tauri webview 里就是真实系统。Node 21+ 同样
 * 提供了 `globalThis.navigator.platform`（本模块要能被 `node --test` 直接引用），
 * 值也是宿主平台（macOS runner 上是 `MacIntel`），所以测试不能假设这里固定为
 * windows —— 平台相关的断言要么显式传参，要么写成与平台无关的不变量。
 */
function buildPlatform(): string {
  const platform = typeof navigator === 'undefined' ? '' : navigator.platform ?? ''
  return /Mac/i.test(platform) ? 'macos' : 'windows'
}

/** 与后端 `repository_sources::DEFAULT_INDEX_ID` 一致。 */
export const OFFICIAL_INDEX_ID = 'official-repository-index'

/**
 * 源的显示名：`nickname_i18n` 按界面语言选，缺失回落到英文、再回落到
 * zh-CN 与镜像值。
 *
 * 这里**不 import `i18n.ts`**：本模块要能被 Node 测试直接引用，而 i18n
 * 依赖 Naive UI 的语言包。规则与 `pickI18n` 一致（精确语言 → en →
 * zh-CN → 第一个非空 → 兜底值），只少了「当前语言」这个响应式来源 ——
 * 调用方传入。
 */
function sourceNickname(
  source: Pick<RepositorySource, 'nickname' | 'nickname_i18n'>,
  locale?: string,
): string {
  const map = source.nickname_i18n
  if (map) {
    if (locale) {
      const exact = map[locale]
      if (exact && exact.trim()) return exact
    }
    for (const key of ['en', 'zh-CN']) {
      const value = map[key]
      if (value && value.trim()) return value
    }
    for (const value of Object.values(map)) {
      if (value && value.trim()) return value
    }
  }
  return source.nickname
}

/**
 * 是否应当改用内置快照兜底。
 *
 * 内置快照可能落后于在线官方源；目前按错误昵称和结果集合判断，
 * 不区分网络错误与解析错误。三条同时满足才尝试回退：
 *
 *   1. 本次源文档阶段确实有失败（`sourceErrors` 非空）；
 *   2. 失败覆盖到了官方根源（错误文案形如 `<昵称>: …`）；
 *   3. 当前官方源树下**一个仓库都没有**。
 *
 * 第 3 条是关键：只要有一个官方子源成功带回仓库，就说明在线可用，
 * 这时用旧的内置内容覆盖会丢掉刚拿到的真实数据（包括作者刚发布的仓库）。
 * 空的官方树还必须伴随官方根错误；仅子源失败不会自动满足第 2 条。
 *
 * 抽成纯函数是为了能在 Node 里直接测；副作用（真正发起回退请求）留在调用方。
 */
export function shouldFallbackToBuiltin(
  sources: Pick<RepositorySource, 'id' | 'nickname' | 'nickname_i18n' | 'parent_id'>[],
  repositories: Pick<Repository, 'source_id'>[],
  sourceErrors: readonly string[],
  /** 当前界面语言；不传时按 zh-CN 兜底值匹配。 */
  locale?: string,
): boolean {
  if (!sourceErrors.length) return false
  const officialRoot = sources.find((source) => source.id === OFFICIAL_INDEX_ID)
  if (!officialRoot) return false
  /*
   * 前缀须遵循与报错显示相同的语言回退顺序。
   *
   * useRepositoryRefresh.sourceError 用 pickI18n；此处的纯函数不加载 UI
   * 依赖，因此以相同顺序取昵称，避免跨语言错误前缀失配。
   */
  const rootName = sourceNickname(officialRoot, locale)
  if (!sourceErrors.some((error) => error.startsWith(`${rootName}:`))) return false

  // 官方树 = 根源 + 它声明出来的全部后代源
  const officialIds = new Set<string>([officialRoot.id])
  let grew = true
  while (grew) {
    grew = false
    for (const source of sources) {
      if (source.parent_id && officialIds.has(source.parent_id) && !officialIds.has(source.id)) {
        officialIds.add(source.id)
        grew = true
      }
    }
  }
  return !repositories.some((repo) => !!repo.source_id && officialIds.has(repo.source_id))
}
