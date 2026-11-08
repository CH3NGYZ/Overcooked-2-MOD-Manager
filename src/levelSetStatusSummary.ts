import type { LevelSetCardTag, LevelSetTone } from './levelSetStatus.ts'
import type { LevelSetInfo } from './types'

export interface LevelSetStatusSummary {
  key: string
  text: string
  tone: LevelSetTone
  count: number
}

export function levelSetTagKey(tag: Pick<LevelSetCardTag, 'kind' | 'text'>): string {
  return `tag:${tag.kind}:${tag.text}`
}

export function levelSetStatusKeys(
  set: Pick<LevelSetInfo, 'enabled'>,
  tags: readonly LevelSetCardTag[],
): string[] {
  return [...tags.map(levelSetTagKey), set.enabled ? 'enabled' : 'disabled']
}

export function summarizeLevelSetTags(
  sets: readonly LevelSetInfo[],
  tagsOf: (set: LevelSetInfo) => readonly LevelSetCardTag[],
): LevelSetStatusSummary[] {
  const buckets = new Map<string, LevelSetStatusSummary>()
  for (const set of sets) {
    for (const tag of tagsOf(set)) {
      if (tag.kind === 'state' && tag.text === 'modsLocal.disabled') continue
      const key = levelSetTagKey(tag)
      const existing = buckets.get(key)
      if (existing) existing.count += 1
      else buckets.set(key, { key, text: tag.text, tone: tag.tone, count: 1 })
    }
  }
  const rank = (entry: LevelSetStatusSummary) =>
    entry.key.startsWith('tag:dependency:') ? 0 : 1
  return [...buckets.values()].sort(
    (left, right) => rank(left) - rank(right) || right.count - left.count,
  )
}

export function summarizeLevelSetToggleStatus(
  sets: readonly Pick<LevelSetInfo, 'enabled'>[],
): LevelSetStatusSummary[] {
  const entries: LevelSetStatusSummary[] = []
  for (const key of ['enabled', 'disabled'] as const) {
    const count = sets.filter((set) => key === 'enabled' ? set.enabled : !set.enabled).length
    if (!count) continue
    entries.push({
      key,
      text: key === 'enabled' ? 'modsLocal.enabled2' : 'modsLocal.disabled',
      tone: key === 'enabled' ? 'accent' : 'muted',
      count,
    })
  }
  return entries
}
