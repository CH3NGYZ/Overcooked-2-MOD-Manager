import { compareVersions } from './modStatus.ts'
import type { AssetVersionEntry, LevelSetInfo, MapInfo } from './types'

export type MapVersionOrder = 'latest' | 'update_available' | 'ahead' | 'unknown'
export type MapVersionEvidence = 'table' | 'info' | 'unknown'
export interface MapVersionRef {
  tag: string
  latestMapsetVersion?: string | null
  latestInfoVersion?: string | null
  assetVersions?: readonly AssetVersionEntry[]
  levelSetName: string
  nameZh?: string
  author: string
}
export interface MapRefreshHint {
  entryStateUnavailable?: boolean
  maps: ReadonlySet<string>
  queued?: ReadonlySet<string>
  sources: ReadonlySet<string>
  sourceOrder?: readonly string[]
}
export type MapIdentity = { name: string; author: string; nameZh?: string | null }
export function isSameLevelSet(left: MapIdentity, right: MapIdentity): boolean {
  const norm = (value: string) => value.trim().toLowerCase()
  if (norm(left.name) !== norm(right.name) || norm(left.author) !== norm(right.author)) return false
  const leftZh = norm(left.nameZh ?? '')
  const rightZh = norm(right.nameZh ?? '')
  return !leftZh || !rightZh || leftZh === rightZh
}
export function mapsForLevelSet(
  set: Pick<LevelSetInfo, 'name' | 'author' | 'name_zh'>,
  maps: readonly MapInfo[],
): MapInfo[] {
  if (!set.name.trim()) return []
  // Every candidate is anchored to the local identity. A missing Chinese name
  // cannot bridge two distinct GAODA maps through a third online declaration.
  return maps.filter(map => isSameLevelSet(
    { name: set.name, author: set.author, nameZh: set.name_zh },
    { name: map.level_set_name, author: map.author, nameZh: map.level_set_name_zh },
  ))
}

/** Exact trimmed asset spelling wins; a semantic alias must locate one row. */
export function assetVersionIndex(table: readonly AssetVersionEntry[], value?: string | null): number | undefined {
  const text = value?.trim()
  if (!text) return undefined
  const exact = table.flatMap((row, i) => row.mapset_version.trim() === text ? [i] : [])
  if (exact.length) return exact.length === 1 ? exact[0] : undefined
  const aliases = table.flatMap((row, i) =>
    compareVersions(row.mapset_version.trim().toLowerCase(), text.toLowerCase()) === 0 ? [i] : [])
  return aliases.length === 1 ? aliases[0] : undefined
}
const infoEqual = (left?: string | null, right?: string | null) =>
  !!left?.trim() && !!right?.trim() && compareVersions(left, right) === 0
const orderOf = (order: number | null): MapVersionOrder =>
  order === null ? 'unknown' : order < 0 ? 'update_available' : order > 0 ? 'ahead' : 'latest'

/** The actual install tag, never a legacy scalar or a cache-derived history. */
export function mapVersionRef(map: Pick<MapInfo, 'latest_tag' | 'latest_mapset_version' | 'latest_info_version' | 'asset_versions'>): Pick<MapVersionRef, 'tag' | 'latestMapsetVersion' | 'latestInfoVersion' | 'assetVersions'> {
  const table = map.asset_versions ?? []
  const tag = map.latest_tag?.trim() ?? ''
  if (table.length) {
    // The actual install target must identify its own row. A blank tail owns
    // unknown values too, and may not revive obsolete scalar declarations.
    const tail = table.at(-1)!
    const index = tail.mapset_version.trim() && tail.info_version.trim() ? assetVersionIndex(table, tag) : undefined
    return { tag, assetVersions: table,
      latestMapsetVersion: index === undefined ? '' : table[index]!.mapset_version.trim(),
      latestInfoVersion: index === undefined ? '' : table[index]!.info_version.trim() }
  }
  return { tag, latestMapsetVersion: map.latest_mapset_version?.trim(), latestInfoVersion: map.latest_info_version?.trim() }
}

/** Reject conflicting complete tables, without ever stitching separate tables. */
function trustedTables(tables: readonly (readonly AssetVersionEntry[])[]): readonly (readonly AssetVersionEntry[])[] {
  const rejected = new Set<number>()
  for (let a = 0; a < tables.length; a++) {
    for (let b = a + 1; b < tables.length; b++) {
      const pairs = tables[a]!.flatMap((row, i) => {
        const j = assetVersionIndex(tables[b]!, row.mapset_version)
        return j === undefined ? [] : [{ i, j, left: row, right: tables[b]![j]! }]
      })
      const mappingConflict = pairs.some(pair => pair.left.info_version.trim() && pair.right.info_version.trim()
        && !infoEqual(pair.left.info_version, pair.right.info_version))
      const orderConflict = pairs.some((pair, i) => pairs.slice(i + 1).some(next => next.j <= pair.j))
      if (mappingConflict || orderConflict) { rejected.add(a); rejected.add(b) }
    }
  }
  return tables.filter((_, i) => !rejected.has(i))
}

function compareEvidence(
  localInfo: string | null | undefined, localAsset: string | null | undefined,
  targetInfo: string | null | undefined, targetAsset: string | null | undefined,
  tables: readonly (readonly AssetVersionEntry[])[],
): { order: MapVersionOrder; evidence: MapVersionEvidence } {
  const orders = tables.flatMap(table => {
    const from = assetVersionIndex(table, localAsset)
    const to = assetVersionIndex(table, targetAsset)
    if (from === undefined || to === undefined || !infoEqual(table[from]!.info_version, localInfo)
      || !infoEqual(table[to]!.info_version, targetInfo)) return []
    return [Math.sign(from - to)]
  })
  if (orders.length && orders.every(order => order === orders[0])) {
    return { order: orderOf(orders[0]!), evidence: 'table' }
  }
  const order = localInfo?.trim() && targetInfo?.trim() ? compareVersions(localInfo, targetInfo) : null
  return { order: orderOf(order), evidence: order === null ? 'unknown' : 'info' }
}

/** Package numbers belonging to different publishers are never semver-ranked. */
export function mapUpdateState(
  localVersion: string | null | undefined,
  remote: Pick<MapVersionRef, 'tag' | 'latestMapsetVersion' | 'latestInfoVersion' | 'assetVersions'> | null | undefined,
  localAssetVersion?: string | null,
): MapVersionOrder {
  if (!remote) return 'unknown'
  const table = remote.assetVersions ?? []
  if (table.length && (!table.at(-1)?.mapset_version.trim() || !table.at(-1)?.info_version.trim())) return 'unknown'
  const target = remote.latestMapsetVersion?.trim() || remote.tag.trim()
  const index = assetVersionIndex(table, target)
  const targetInfo = remote.latestInfoVersion ?? (index === undefined ? '' : table[index]!.info_version)
  return compareEvidence(localVersion, localAssetVersion, targetInfo, target, trustedTables(table.length ? [table] : [])).order
}

export interface MapUpdateCandidate {
  map: MapInfo
  mapId: string
  tag: string
  order: MapVersionOrder
  evidence: MapVersionEvidence
  infoVersion: string
  /** Actual target mapset row; release tag may be only a transport key. */
  targetVersion: string
}
export interface LevelSetVersionAggregate {
  order: MapVersionOrder | 'unbound' | 'refreshing'
  evidence: MapVersionEvidence
  candidates: MapUpdateCandidate[]
  updateCandidates: MapUpdateCandidate[]
  primaryTarget?: MapUpdateCandidate
  pending: boolean
  failed: boolean
}
export function aggregateLevelSetVersion(set: LevelSetInfo, maps: readonly MapInfo[], refreshing?: MapRefreshHint): LevelSetVersionAggregate {
  const matching = mapsForLevelSet(set, maps)
  const sourceOrder = new Map((refreshing?.sourceOrder ?? []).map((id, i) => [id, i]))
  const ordered = [...matching].sort((left, right) =>
    (sourceOrder.get(left.source_id ?? '') ?? Number.MAX_SAFE_INTEGER) - (sourceOrder.get(right.source_id ?? '') ?? Number.MAX_SAFE_INTEGER)
    || left.id.localeCompare(right.id))
  const isPending = (map: MapInfo) => !!refreshing && (refreshing.maps.has(map.id)
    || !!refreshing.queued?.has(map.id) || !!map.source_id && !!refreshing.sources?.has(map.source_id))
  const normalizedChineseNames = new Set(matching.map(map => map.level_set_name_zh?.trim().toLowerCase()).filter(Boolean))
  const ambiguousIdentity = !set.name_zh?.trim() && normalizedChineseNames.size > 1
  const completed = matching.filter(map => !map.last_error && !isPending(map) && !refreshing?.entryStateUnavailable)
  const tables = ambiguousIdentity ? [] : trustedTables(completed.map(map => map.asset_versions ?? [])
    .filter(table => !!table.at(-1)?.mapset_version.trim() && !!table.at(-1)?.info_version.trim()))
  const pending = !!refreshing?.entryStateUnavailable || matching.some(isPending)
  const failed = matching.some(map => !!map.last_error)
  const candidates = ordered.map(map => {
    const ref = mapVersionRef(map)
    const table = map.asset_versions ?? []
    const validTarget = !table.length || !!ref.latestMapsetVersion && !!ref.latestInfoVersion
    const result = !ambiguousIdentity && validTarget && !set.error && ref.tag && !map.last_error && !isPending(map) && !refreshing?.entryStateUnavailable
      ? compareEvidence(set.version, set.installed_mapset_version, ref.latestInfoVersion,
        ref.latestMapsetVersion || ref.tag, tables)
      : { order: 'unknown' as const, evidence: 'unknown' as const }
    return { map, mapId: map.id, tag: ref.tag, infoVersion: ref.latestInfoVersion ?? '', targetVersion: ref.latestMapsetVersion || ref.tag, ...result }
  })
  const updates = candidates.filter(candidate => candidate.order === 'update_available')
  // A partial order is not a sort comparator. Keep all maximal targets, then
  // apply the deterministic source/id preference only to incomparable maxima.
  const maxima = updates.filter(candidate => !updates.some(other => other !== candidate
    && compareEvidence(candidate.infoVersion, candidate.targetVersion, other.infoVersion, other.targetVersion, tables).order === 'update_available'))
  const primaryTarget = maxima[0]
  // A nonempty finite partial order has a maximal item. None means conflicting
  // global evidence, even when every pair of complete tables looked consistent.
  const order = primaryTarget ? 'update_available' : updates.length ? 'unknown' : pending ? 'refreshing'
    : !matching.length ? 'unbound'
      : candidates.some(candidate => candidate.order === 'unknown') ? 'unknown'
        : candidates.some(candidate => candidate.order === 'latest') ? 'latest' : 'ahead'
  return { order, evidence: primaryTarget?.evidence ?? (candidates.find(candidate => candidate.order === order)?.evidence ?? 'unknown'),
    candidates, updateCandidates: updates, primaryTarget, pending, failed }
}
