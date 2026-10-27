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
 * 社区源（GitHub / Gitee）默认地区为空，因此对全部地区可用；
 * 它们与官方源的关系由后端 `community::merge_results` 按身份闪避处理
 * （官方已收录的社区副本被置 `enabled=false`），不在地区维度表达。
 */
export function repositoryIsAvailable(
  repo: Pick<Repository, 'enabled' | 'region' | 'source_id'>,
  sources: Pick<RepositorySource, 'id' | 'region'>[] | undefined,
  currentRegion: string | undefined,
): boolean {
  if (!repo.enabled) return false
  if (!repositoryRegionMatches(repo.region, currentRegion)) return false
  if (!repo.source_id) return true
  const source = repositorySource(repo, sources)
  return !!source && sourceMatchesRegion(source, currentRegion)
}

/** 与后端 `repository_sources::DEFAULT_INDEX_ID` 一致。 */
export const OFFICIAL_INDEX_ID = 'official-repository-index'

/**
 * 是否应当改用内置快照兜底。
 *
 * 内置快照是落后几个版本的官方源，只在**确实拿不到在线内容**时使用，
 * 因此判断刻意保守 —— 三条同时满足才回退：
 *
 *   1. 本次源文档阶段确实有失败（`sourceErrors` 非空）；
 *   2. 失败覆盖到了官方根源（错误文案形如 `<昵称>: …`）；
 *   3. 当前官方源树下**一个仓库都没有**。
 *
 * 第 3 条是关键：只要有一个官方子源成功带回仓库，就说明在线可用，
 * 这时用旧的内置内容覆盖会丢掉刚拿到的真实数据（包括作者刚发布的仓库）。
 * 「子源可访问但没有仓库」也不算可用，会回退 —— 那通常意味着索引被清空
 * 或入口指向了废弃分支。
 *
 * 抽成纯函数是为了能在 Node 里直接测；副作用（真正发起回退请求）留在调用方。
 */
export function shouldFallbackToBuiltin(
  sources: Pick<RepositorySource, 'id' | 'nickname' | 'parent_id'>[],
  repositories: Pick<Repository, 'source_id'>[],
  sourceErrors: readonly string[],
): boolean {
  if (!sourceErrors.length) return false
  const officialRoot = sources.find((source) => source.id === OFFICIAL_INDEX_ID)
  if (!officialRoot) return false
  if (!sourceErrors.some((error) => error.startsWith(`${officialRoot.nickname}:`))) return false

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
