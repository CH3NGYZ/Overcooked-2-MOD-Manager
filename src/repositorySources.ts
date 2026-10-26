import type { Repository, RepositorySource } from './types'
import { t } from './i18n'

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

/** 渠道展示文案（页头用）。 */
export function repositoryRegionLabel(region: string | undefined): string {
  return normalizeRepositoryRegion(region) === 'chinese_mainland'
    ? t('中国大陆地区')
    : t('国际地区')
}
