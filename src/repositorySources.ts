import type { RepositorySource } from './types'
import { t } from './i18n'

export function repositorySourceName(source: Pick<RepositorySource, 'nickname' | 'nickname_i18n'>, language: string) {
  return [source.nickname_i18n?.[language], source.nickname_i18n?.['zh-CN'], source.nickname]
    .find((value): value is string => typeof value === 'string' && !!value.trim())
    ?.trim() || ''
}

export function normalizeRepositoryRegion(region: string | undefined) {
  const value = region?.trim().toLowerCase()
  if (value === 'mainland-china') return 'chinese_mainland'
  if (value === 'other') return 'international'
  return value || ''
}

export function repositoryRegionMatches(region: string | undefined, currentRegion: string | undefined) {
  const value = normalizeRepositoryRegion(region)
  if (value === 'chinese_mainland') return currentRegion === 'chinese_mainland'
  if (value === 'international') return currentRegion !== 'chinese_mainland'
  return true
}

export function repositorySourceKey(source: Pick<RepositorySource, 'url' | 'region'>) {
  return JSON.stringify([source.url.trim().toLowerCase(), normalizeRepositoryRegion(source.region)])
}

export function repositorySourceRegionLabel(region?: string) {
  const normalized = normalizeRepositoryRegion(region)
  if (normalized === 'chinese_mainland') return t('中国大陆地区')
  if (normalized === 'international') return t('国际地区')
  return t('全部地区')
}
