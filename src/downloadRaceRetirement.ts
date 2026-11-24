import type { DownloadRaceProgress } from './types'

const RETIRED_GRACE_MS = 3000

type Retirement = { phase: 'failed' | 'stopped'; expiresAt: number }

export function createDownloadRaceRetirement() {
  let raceId: string | undefined
  let retirement = new Map<number, Retirement>()

  return (race: DownloadRaceProgress, now: number) => {
    if (raceId !== race.id) {
      raceId = race.id
      retirement.clear()
    }
    const nextRetirement = new Map<number, Retirement>()
    const folded = new Set<number>()
    const counts = { failed: 0, stopped: 0 }
    let nextExpiry = Infinity

    for (const { id, phase } of race.sources) {
      if (id === race.locked_id || (phase !== 'failed' && phase !== 'stopped')) continue
      const previous = retirement.get(id)
      const expiresAt = previous?.phase === phase ? previous.expiresAt : now + RETIRED_GRACE_MS
      nextRetirement.set(id, { phase, expiresAt })
      if (now < expiresAt) nextExpiry = Math.min(nextExpiry, expiresAt)
      else {
        folded.add(id)
        counts[phase] += 1
      }
    }

    retirement = nextRetirement
    return { folded, counts, nextExpiry }
  }
}
