import { levelSetForMap, mapUpdateState, type LevelSetTone } from './levelSetStatus.ts'
import type { LevelSetInfo, MapInfo } from './types'

/** 在线地图下载卡片的展示状态，不包含本地管理页的停用/解析状态。 */
export type MapCardState =
  | 'not_installed'
  | 'installed'
  | 'update_available'
  | 'latest'
  | 'cannot_check'
  | 'not_refreshed'

/** 状态 → i18n key。全部复用 MOD 侧已有条目（`cannotCheckUpdate` 除外）。 */
export const MAP_CARD_STATE_TAGS: Record<MapCardState, string> = {
  not_installed: 'overview.notInstalled',
  installed: 'modsLocal.installed',
  update_available: 'tools.update',
  latest: 'modsLocal.latest',
  // 地图刷新失败时，用户要重试的是刷新而不是重装。
  cannot_check: 'levelSet.cannotCheckUpdate',
  not_refreshed: 'modsOnline.notRefreshed',
}

/** 状态 → 语义色调，与 MOD 侧同一档位。 */
export const MAP_CARD_STATE_TONES: Record<MapCardState, LevelSetTone> = {
  not_installed: 'muted',
  installed: 'accent',
  update_available: 'moderate',
  latest: 'accent',
  cannot_check: 'moderate',
  not_refreshed: 'muted',
}

type MapCardSource = Pick<
  MapInfo,
  'level_set_name' | 'author' | 'latest_tag' | 'latest_mapset_version' | 'latest_info_version' | 'last_error'
>

/**
 * 刷新错误优先于缓存版本；没有 Release 时只确认是否已安装；版本语义由
 * levelSetStatus.ts 的身份匹配与 mapUpdateState 唯一计算。
 */
export function mapCardState(map: MapCardSource, sets: readonly LevelSetInfo[]): MapCardState {
  if (map.last_error) return 'cannot_check'
  const set = levelSetForMap(map, sets)
  if (!map.latest_tag) return set ? 'installed' : 'not_refreshed'
  if (!set) return 'not_installed'
  const order = mapUpdateState(set.version, {
    tag: map.latest_tag,
    latestMapsetVersion: map.latest_mapset_version,
    latestInfoVersion: map.latest_info_version ?? undefined,
  }, set.installed_mapset_version)
  if (order === 'unknown') return 'cannot_check'
  return order === 'update_available' ? 'update_available' : 'latest'
}

/** 下载页卡片状态区的渲染值。 */
export interface MapCardStatus {
  key: MapCardState
  /** i18n key（视图层 `t()`）。 */
  text: string
  tone: LevelSetTone
}

/** `mapCardState` + 文案 + 色调，视图层直接用这一份。 */
export function mapCardStatus(map: MapCardSource, sets: readonly LevelSetInfo[]): MapCardStatus {
  const key = mapCardState(map, sets)
  return { key, text: MAP_CARD_STATE_TAGS[key], tone: MAP_CARD_STATE_TONES[key] }
}

/** 下载页卡片动作，含义与 `OnlineCard` 的 `CardActionKind` 对齐。 */
export type MapCardAction = 'install' | 'reinstall' | 'update' | 'refresh'

/**
 * 没有可用 Release 时刷新；未安装时安装；版本落后时更新；其余已安装状态重装。
 */
export function mapCardAction(map: MapCardSource, sets: readonly LevelSetInfo[]): MapCardAction {
  if (map.last_error || !map.latest_tag) return 'refresh'
  const set = levelSetForMap(map, sets)
  if (!set) return 'install'
  const order = mapUpdateState(set.version, {
    tag: map.latest_tag,
    latestMapsetVersion: map.latest_mapset_version,
    latestInfoVersion: map.latest_info_version ?? undefined,
  }, set.installed_mapset_version)
  return order === 'update_available' ? 'update' : 'reinstall'
}
