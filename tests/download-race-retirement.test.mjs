import assert from 'node:assert/strict'
import test from 'node:test'
import { createDownloadRaceRetirement } from '../src/downloadRaceRetirement.ts'

const source = (id, phase) => ({ id, phase, route: 'proxy', downloaded: 12, total: null, speed_bytes: null, history_preferred: false })
const snapshot = (sources, rest = {}) => ({ id: 'race-1', primary_id: 1, can_lock: true, waiting: 0, elapsed_ms: 0, sources, ...rest })

test('each failure or stop folds three seconds after its first frame, not its latest snapshot', () => {
  const retirement = createDownloadRaceRetirement()
  const keep = source(1, 'downloading'), failure = source(2, 'failed'), stopped = source(3, 'stopped')
  const race = snapshot([keep, failure])
  assert.deepEqual(retirement(race, 350), { folded: new Set(), counts: { failed: 0, stopped: 0 }, nextExpiry: 3350 })
  const more = snapshot([keep, failure, stopped], { elapsed_ms: 5000 })
  assert.equal(retirement(more, 1500).nextExpiry, 3350)
  assert.equal(retirement(more, 3349).folded.size, 0)
  assert.deepEqual(retirement(more, 3350), { folded: new Set([2]), counts: { failed: 1, stopped: 0 }, nextExpiry: 4500 })
  assert.deepEqual(retirement(more, 4500), { folded: new Set([2, 3]), counts: { failed: 1, stopped: 1 }, nextExpiry: Infinity })
})

test('locked failures stay actionable, and unlocking starts the ordinary grace period', () => {
  const retirement = createDownloadRaceRetirement()
  const sources = [source(1, 'stopped'), source(2, 'failed')]
  const locked = snapshot(sources, { locked_id: 2 })
  retirement(locked, 0)
  assert.deepEqual(retirement(locked, 9000), { folded: new Set([1]), counts: { failed: 0, stopped: 1 }, nextExpiry: Infinity })
  const unlocked = snapshot(sources)
  assert.equal(retirement(unlocked, 9500).nextExpiry, 12500)
  assert.deepEqual(retirement(unlocked, 12500).folded, new Set([1, 2]))
})

test('a reactivated route or a new race gets fresh grace time despite candidate ID reuse', () => {
  const retirement = createDownloadRaceRetirement()
  const failed = snapshot([source(2, 'failed')])
  retirement(failed, 0)
  assert.deepEqual(retirement(failed, 3000).folded, new Set([2]))
  assert.equal(retirement(snapshot([source(2, 'connecting')]), 3100).folded.size, 0)
  assert.equal(retirement(failed, 4100).nextExpiry, 7100)
  assert.deepEqual(retirement(failed, 7100).folded, new Set([2]))
  const other = snapshot(failed.sources, { id: 'race-2' })
  assert.deepEqual(retirement(other, 7200), { folded: new Set(), counts: { failed: 0, stopped: 0 }, nextExpiry: 10200 })
})
