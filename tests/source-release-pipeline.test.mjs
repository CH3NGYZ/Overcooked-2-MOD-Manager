import assert from 'node:assert/strict'
import test from 'node:test'
import { createRefreshState } from '../src/refreshState.ts'
import { runSourceReleaseRefresh, runReleaseBatch, refreshReleaseItem } from '../src/releaseRefresh.ts'
import { runtimeModStatus } from '../src/modRuntimeStatus.ts'

const deferred = () => {
  let resolve
  const promise = new Promise(r => { resolve = r })
  return { promise, resolve }
}
const tick = () => new Promise(resolve => setTimeout(resolve, 0))

test('source TOMLs are the barrier; a bound MOD concludes before unrelated API finishes', async () => {
  const state = createRefreshState(2)
  const source = deferred(), bound = deferred(), slow = deferred()
  const calls = []
  const local = { dll_file: 'A.dll', version: '1.0.0', repository_id: 'bound', labels: ['latest'], enabled: true }
  const repos = ['bound', 'slow'].map(id => ({
    id, provider: 'github', username: 'u', repository: id, enabled: true,
    download_mode: 'release', cached_releases: [],
  }))
  const pipeline = runSourceReleaseRefresh({
    state, trigger: 'full',
    prepare: () => source.promise,
    refresh: () => runReleaseBatch({
      state, items: repos, trigger: 'single',
      fetch: item => { calls.push(item.id); return item.id === 'bound' ? bound.promise : slow.promise },
      apply: (item, releases) => { item.cached_releases = releases },
      errorText: () => 'failed',
    }),
  })
  await tick()
  assert.equal(state.preparingSources.value, true)
  assert.deepEqual(calls, [], 'no API before all source TOMLs settle')
  source.resolve([])
  await tick()
  assert.equal(state.preparingSources.value, false)
  assert.deepEqual(calls, ['bound', 'slow'])
  bound.resolve([{ tag_name: 'v2.0.0', version: '2.0.0', assets: [] }])
  await tick()
  const verdict = runtimeModStatus(local, [local], repos, {
    refreshing: state.refreshing.value, queued: state.queued.value,
    sourceRefreshing: {}, entryStateUnavailable: state.preparingSources.value,
  })
  assert.equal(state.refreshingAll.value, true, 'slow unrelated API is still running')
  assert.equal(verdict.mod.update_available, true, 'completed bound result is already usable')
  slow.resolve([])
  assert.equal((await pipeline).refreshed, 2)
  assert.equal(state.anyRefreshing.value, false)
})

test('card refresh waits for active source preparation, without starting another source pass', async () => {
  const state = createRefreshState(1)
  const source = deferred()
  let calls = 0
  const pipeline = runSourceReleaseRefresh({
    state, trigger: 'full', prepare: () => source.promise,
    refresh: () => runReleaseBatch({ state, items: [], trigger: 'single', fetch: async () => [], errorText: () => '' }),
  })
  const card = refreshReleaseItem(state, { id: 'card' }, {
    fetch: async () => { calls++; return [] }, errorText: () => '',
  })
  await tick()
  assert.equal(calls, 0)
  source.resolve([])
  await Promise.all([pipeline, card])
  assert.equal(calls, 1)
})

test('source failure releases phase and batch flags so the next refresh can retry', async () => {
  const state = createRefreshState(1)
  await assert.rejects(runSourceReleaseRefresh({
    state, trigger: 'bound', prepare: async () => { throw new Error('bad source') },
    refresh: async () => { throw new Error('must not refresh') },
  }), /bad source/)
  assert.equal(state.preparingSources.value, false)
  assert.equal(state.anyRefreshing.value, false)
})
