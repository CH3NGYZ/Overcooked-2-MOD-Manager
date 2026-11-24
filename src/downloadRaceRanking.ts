import type { DownloadRaceProgress, DownloadRaceSource } from './types'

const VISIBLE_SOURCE_LIMIT = 5
const REORDER_AFTER_MS = 1500

export function isActiveDownloadSource(source: DownloadRaceSource): boolean {
  return source.phase === 'connecting' || source.phase === 'downloading' || source.phase === 'verifying'
}

function rankGroup(source: DownloadRaceSource): number {
  if (!isActiveDownloadSource(source)) return 2
  return source.phase === 'downloading' && source.speed_bytes !== null ? 0 : 1
}

function leadsBy30Percent(challenger: number, leader: number): boolean {
  return challenger > leader && challenger * 10 >= leader * 13
}

export function createDownloadRaceRanking() {
  let raceId: string | undefined
  let previousTime = 0
  let order: number[] = []
  const leadsSince = new Map<string, number>()

  return (snapshot: DownloadRaceProgress): DownloadRaceSource[] => {
    const { id, sources, elapsed_ms, locked_id } = snapshot
    if (raceId !== id || elapsed_ms < previousTime) {
      raceId = id
      order = []
      leadsSince.clear()
    }
    previousTime = elapsed_ms
    const sourceById = new Map(sources.map(source => [source.id, source]))
    const previous = new Set(order)
    order = order.filter(sourceId => sourceById.has(sourceId))
    order.push(...sources.filter(source => !previous.has(source.id)).map(source => source.id))
    const ranked = order.map(sourceId => sourceById.get(sourceId)!)

    if (locked_id != null) {
      leadsSince.clear()
      return ranked
    }

    ranked.sort((left, right) => rankGroup(left) - rankGroup(right))
    const sustained = new Set<string>()
    for (const [index, before] of ranked.entries()) {
      if (rankGroup(before) !== 0) break
      for (const after of ranked.slice(index + 1)) {
        if (rankGroup(after) !== 0) break
        const earlier = before.speed_bytes!
        const later = after.speed_bytes!
        if (!leadsBy30Percent(later, earlier)) continue
        const key = `${before.id}:${after.id}`
        if (!leadsSince.has(key)) leadsSince.set(key, elapsed_ms)
        sustained.add(key)
      }
    }
    for (const key of leadsSince.keys()) if (!sustained.has(key)) leadsSince.delete(key)

    for (let index = 1; index < ranked.length; index += 1) {
      for (let position = index; position > 0; position -= 1) {
        const before = ranked[position - 1]
        const after = ranked[position]
        const since = leadsSince.get(`${before.id}:${after.id}`)
        if (since === undefined || elapsed_ms - since < REORDER_AFTER_MS) break
        ranked[position - 1] = after
        ranked[position] = before
      }
    }
    order = ranked.map(source => source.id)
    return ranked
  }
}

export function visibleDownloadSources(sources: readonly DownloadRaceSource[], expanded: boolean, ...pinned: (number | null | undefined)[]) {
  if (expanded || sources.length <= VISIBLE_SOURCE_LIMIT) return sources
  const required = new Set(pinned.filter(id => sources.some(source => source.id === id)))
  let remaining = VISIBLE_SOURCE_LIMIT - required.size
  return sources.filter(source => required.has(source.id) || remaining-- > 0)
}
