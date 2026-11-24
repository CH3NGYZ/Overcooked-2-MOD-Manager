import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createDownloadRaceRanking, visibleDownloadSources } from '../src/downloadRaceRanking.ts'

const source = (id, speed_bytes, phase = 'downloading') => ({
  id, phase, route: 'proxy', downloaded: 0, total: null, speed_bytes, history_preferred: false,
})
const snapshot = (elapsed_ms, sources, rest = {}) => ({
  id: 'race-1', elapsed_ms, sources, can_lock: true, waiting: 0, primary_id: 1, ...rest,
})
const ids = (sources) => sources.map(({ id }) => id)

test('ranking moves only after a 30% advantage persists for 1.5 seconds', () => {
  const rank = createDownloadRaceRanking()
  const speeds = (fast) => [source(1, 100), source(2, fast)]
  assert.deepEqual(ids(rank(snapshot(0, speeds(129)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(2000, speeds(129)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(2250, speeds(130)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(3749, speeds(130)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(3750, speeds(130)))), [2, 1])
})

test('a drop, missing sample or new race resets the sustained advantage', () => {
  const rank = createDownloadRaceRanking()
  const speeds = (fast) => [source(1, 100), source(2, fast)]
  assert.deepEqual(ids(rank(snapshot(0, speeds(130)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(1400, speeds(null)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(1500, speeds(130)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(2900, speeds(129)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(3000, speeds(130)))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(4500, speeds(130), { id: 'race-2' }))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(6000, speeds(130), { id: 'race-2' }))), [2, 1])
})

test('locked races freeze order without letting evidence accumulate for unlock', () => {
  const rank = createDownloadRaceRanking()
  const speeds = [source(1, 100), source(2, 200)]
  assert.deepEqual(ids(rank(snapshot(0, speeds))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(1700, [source(1, null, 'stopped'), speeds[1]], { locked_id: 2 }))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(2000, speeds))), [1, 2])
  assert.deepEqual(ids(rank(snapshot(3500, speeds))), [2, 1])
})

test('five default slots include the primary, locked and focused rows without moving them', () => {
  const sources = Array.from({ length: 8 }, (_, index) => source(index + 1, index * 100))
  assert.deepEqual(ids(visibleDownloadSources(sources, false, 6, 8, 7)), [1, 2, 6, 7, 8])
  assert.deepEqual(ids(visibleDownloadSources(sources, false, undefined, null, 100)), [1, 2, 3, 4, 5])
  assert.deepEqual(ids(visibleDownloadSources(sources, true, 6, 8)), ids(sources))
})

test('backend preference stays visible regardless of the speed ranking', () => {
  const rank = createDownloadRaceRanking()
  const speeds = Array.from({ length: 6 }, (_, index) => source(index + 1, index === 1 ? 130 : 100))
  rank(snapshot(0, speeds))
  const preferred = snapshot(1500, speeds, { primary_id: 6 })
  const ranked = rank(preferred)
  assert.deepEqual(ids(ranked), [2, 1, 3, 4, 5, 6])
  assert.deepEqual(ids(visibleDownloadSources(ranked, false, preferred.primary_id)), [2, 1, 3, 4, 6])
})

test('the row badge uses backend preference, with a lock taking priority and history independent', () => {
  const component = readFileSync(new URL('../src/components/DownloadRace.vue', import.meta.url), 'utf8')
  const row = readFileSync(new URL('../src/components/DownloadRaceRow.vue', import.meta.url), 'utf8')
  assert.match(component, /<DownloadRaceRow\b[^>]*:primary="race\.primary_id === source\.id"\s+:locked="isLocked\(source\)"/)
  assert.match(row, /v-if="locked"[^>]*>[^<]*tools\.downloadRace\.locked[^<]*<\/span>\s*<span v-else-if="primary"[^>]*>[^<]*tools\.downloadRace\.preferred/)
  assert.match(row, /v-if="source\.history_preferred"[^>]*>[^<]*tools\.downloadRace\.historyPreferred/)
  assert.equal([...row.matchAll(/t\('tools\.downloadRace\.preferred'\)/g)].length, 1)
})
