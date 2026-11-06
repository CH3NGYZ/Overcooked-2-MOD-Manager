import { composeAssetRule, compareMapVersions, latestVersionOf, renderAssetName, versionFromValidAsset } from './mapPublishAsset.ts'
import { unregisteredVersions, type AssetVersionRow } from './mapPublishFlow.ts'
import type { PublishRelease } from './types'

export function displayName(release: PublishRelease): string {
  return release.metadata?.level_set_name.trim() || release.title.trim() || release.tag
}

export function latestVersionOfRelease(release: PublishRelease): string {
  return release.metadata?.asset_versions.at(-1)?.mapset_version.trim() ||
    latestVersionOf(release.assets.map(asset => asset.name)) ||
    release.metadata?.latest_mapset_version.trim() || ''
}

export function releaseAssetRule(release: PublishRelease): string {
  return release.metadata?.asset.trim() || composeAssetRule(release.tag)
}

/** 已登记的按作者顺序，表外附件接在末尾；不生成指向缺失附件的行。 */
export function versionRowsOf(release: PublishRelease | null): AssetVersionRow[] {
  if (!release) return []
  const rule = releaseAssetRule(release)
  const assets = new Map(release.assets.flatMap(asset => {
    const version = versionFromValidAsset(rule, asset.name)
    return version ? [[version, asset] as const] : []
  }))
  const rows: AssetVersionRow[] = []
  for (const entry of release.metadata?.asset_versions ?? []) {
    const version = entry.mapset_version.trim()
    const asset = assets.get(version)
    if (!asset) continue
    assets.delete(version)
    rows.push({ mapsetVersion: version, infoVersion: entry.info_version.trim(), size: asset.size })
  }
  for (const [version, asset] of [...assets].sort(([a], [b]) => compareMapVersions(a, b))) {
    rows.push({ mapsetVersion: version, infoVersion: '', size: asset.size })
  }
  return rows
}

/** 上传或删除附件不能抹掉尚未保存的排序与 info 版本修正。 */
export function reconcileVersionRows(rows: readonly AssetVersionRow[], release: PublishRelease): AssetVersionRow[] {
  const remaining = new Map(versionRowsOf(release).map(row => [row.mapsetVersion, row]))
  const result: AssetVersionRow[] = []
  for (const row of rows) {
    const remote = remaining.get(row.mapsetVersion)
    if (!remote) continue
    remaining.delete(row.mapsetVersion)
    result.push({ ...row, size: remote.size })
  }
  return [...result, ...remaining.values()]
}

export function unregisteredReleaseVersions(release: PublishRelease): string[] {
  return unregisteredVersions(
    versionRowsOf(release).map(row => row.mapsetVersion),
    (release.metadata?.asset_versions ?? []).map(row => row.mapset_version),
  )
}

export function assetNameForRow(release: PublishRelease, row: AssetVersionRow): string {
  return renderAssetName(releaseAssetRule(release), '', row.mapsetVersion)
}

export function upsertPublishRelease(releases: readonly PublishRelease[], saved: PublishRelease): PublishRelease[] {
  const found = releases.some(item => item.id === saved.id)
  return found ? releases.map(item => item.id === saved.id ? saved : item) : [saved, ...releases]
}
