import { levelSetGroup } from './levelSetStatus.ts'
import type { LevelSetInfo, MapInfo } from './types'

export interface LevelSetGroupNode {
  key: string
  label: string
  sets: LevelSetInfo[]
}

export function buildLevelSetGroups(
  sets: readonly LevelSetInfo[],
  maps: readonly MapInfo[],
  locale: string,
  pickI18n: (map: Record<string, string> | undefined, fallback: string) => string,
  ungroupedLabel: string,
): LevelSetGroupNode[] {
  const buckets = new Map<string, { label: string; sets: LevelSetInfo[] }>()
  for (const set of sets) {
    const group = levelSetGroup(set, maps)
    let bucket = buckets.get(group.key)
    if (!bucket) {
      bucket = {
        label: pickI18n(group.labelI18n, group.canonical) || ungroupedLabel,
        sets: [],
      }
      buckets.set(group.key, bucket)
    }
    bucket.sets.push(set)
  }
  return [...buckets.entries()]
    .map(([key, bucket]) => ({ key, label: bucket.label, sets: bucket.sets }))
    .sort((left, right) => left.label.localeCompare(right.label, locale))
}
