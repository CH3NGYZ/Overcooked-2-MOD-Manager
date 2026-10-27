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

/** 与后端 `community::GITEE_SOURCE_ID` 对应的前端常量。 */
export const GITEE_COMMUNITY_SOURCE_ID = 'gitee-community'

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
 * Gitee 社区源额外遵循「仅中国大陆」规则：它的地区字段允许为空，
 * 单看来源地区会漏判，所以与后端 `provider_matches_region` 一样显式判断。
 */
export function repositoryIsAvailable(
  repo: Pick<Repository, 'enabled' | 'region' | 'source_id'>,
  sources: Pick<RepositorySource, 'id' | 'region'>[] | undefined,
  currentRegion: string | undefined,
): boolean {
  if (!repo.enabled) return false
  if (!repositoryRegionMatches(repo.region, currentRegion)) return false
  if (!repo.source_id) return true
  if (repo.source_id === GITEE_COMMUNITY_SOURCE_ID && currentRegion !== 'chinese_mainland') return false
  const source = repositorySource(repo, sources)
  return !!source && sourceMatchesRegion(source, currentRegion)
}
