import type { RepositorySource } from './types'

export function repositorySourceName(source: Pick<RepositorySource, 'nickname' | 'nickname_i18n'>, language: string) {
  return source.nickname_i18n?.[language]?.trim() || source.nickname
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
