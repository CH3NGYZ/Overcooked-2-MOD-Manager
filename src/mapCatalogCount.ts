import type { MapInfo } from './types'

export function mapCatalogCount(
  maps: readonly Pick<MapInfo, 'map_identity' | 'last_error'>[],
  verified: boolean,
) {
  const identities = new Set<string>()
  const failures = new Set<string>()
  let complete = verified
  for (const map of maps) {
    if (!map.map_identity) {
      complete = false
      continue
    }
    identities.add(map.map_identity)
    if (map.last_error) failures.add(map.map_identity)
  }
  return { count: identities.size || (complete ? 0 : null), failed: failures.size, complete }
}
