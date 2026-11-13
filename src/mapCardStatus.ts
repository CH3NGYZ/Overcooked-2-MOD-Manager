import { levelSetForMap, type LevelSetTone } from './levelSetStatus.ts'
import { aggregateLevelSetVersion } from './mapVersion.ts'
import { isPrereleaseVersion, versionDisplay, type VersionDisplay } from './modStatus.ts'
import type { LevelSetInfo, MapInfo } from './types'

/** Each source card describes its own actual install target. */
export type MapCardState = 'not_installed' | 'installed' | 'update_available' | 'latest' | 'ahead' | 'unknown' | 'cannot_check' | 'not_refreshed'
export const MAP_CARD_STATE_TAGS: Record<MapCardState, string> = {
  not_installed: 'overview.notInstalled', installed: 'modsLocal.installed',
  update_available: 'tools.update', latest: 'mapVersion.sameVersion',
  ahead: 'mapVersion.localNewer', unknown: 'mapVersion.incomparable',
  cannot_check: 'levelSet.cannotCheckUpdate', not_refreshed: 'modsOnline.notRefreshed',
}
export const MAP_CARD_STATE_TONES: Record<MapCardState, LevelSetTone> = {
  not_installed: 'muted', installed: 'accent', update_available: 'moderate',
  latest: 'accent', ahead: 'accent', unknown: 'muted', cannot_check: 'moderate', not_refreshed: 'muted',
}
export function mapCardState(map: MapInfo, sets: readonly LevelSetInfo[], maps: readonly MapInfo[] = [map]): MapCardState {
  if (map.last_error) return 'cannot_check'
  const set = levelSetForMap(map, sets)
  if (!map.latest_tag) return set ? 'installed' : 'not_refreshed'
  if (!set) return 'not_installed'
  return aggregateLevelSetVersion(set, maps).candidates.find(candidate => candidate.mapId === map.id)?.order ?? 'unknown'
}
export interface MapCardStatus { key: MapCardState; text: string; tone: LevelSetTone }
export function mapCardVersion(map: MapInfo, fallbackMarker = ''): VersionDisplay {
  const release = map.latest_release
  if (!release) return versionDisplay(null)
  const value = release.tag_name || release.version
  return versionDisplay(value, isPrereleaseVersion(value, release.apiPrerelease, release.prerelease), fallbackMarker)
}
export function mapCardStatus(map: MapInfo, sets: readonly LevelSetInfo[], maps: readonly MapInfo[] = [map]): MapCardStatus {
  const key = mapCardState(map, sets, maps)
  return { key, text: MAP_CARD_STATE_TAGS[key], tone: MAP_CARD_STATE_TONES[key] }
}
export type MapCardAction = 'install' | 'reinstall' | 'update' | 'refresh'
export function mapCardAction(map: MapInfo, sets: readonly LevelSetInfo[], maps: readonly MapInfo[] = [map]): MapCardAction {
  if (map.last_error || !map.latest_tag) return 'refresh'
  const state = mapCardState(map, sets, maps)
  return state === 'update_available' ? 'update' : state === 'latest' ? 'reinstall' : 'install'
}
export function mapCardActionLabel(map: MapInfo, sets: readonly LevelSetInfo[], maps: readonly MapInfo[] = [map]): string {
  const state = mapCardState(map, sets, maps)
  if (state === 'ahead') return 'mapVersion.installThisVersion'
  if (state === 'unknown') return 'mapVersion.chooseThisVersion'
  const action = mapCardAction(map, sets, maps)
  return { install: 'common.action.install', reinstall: 'common.action.reinstall', update: 'common.action.update', refresh: 'common.action.refresh' }[action]
}
