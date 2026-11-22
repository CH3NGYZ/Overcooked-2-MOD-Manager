import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { mapCatalogCount } from '../src/mapCatalogCount.ts'

const map = (map_identity, options = {}) => ({ map_identity, ...options })
const gaoda = 'gaoda\u0001gaoda'
const highSchool = `${gaoda}\u0001胡闹糕中`
const college = `${gaoda}\u0001胡闹达学`

test('the same map from distinct sources counts once without merging titles or legacy identities', () => {
  const maps = [map(highSchool, { id: 'source-a:abc' }), map(highSchool, { id: 'source-b:def' }), map(college), map(gaoda)]
  assert.deepEqual(mapCatalogCount(maps, true), { count: 3, failed: 0, complete: true })
  assert.equal(maps.length, 4)
})

test('Release failures are counted once per identity and are distinct from incomplete catalog reads', () => {
  const maps = [map(highSchool, { last_error: 'timeout' }), map(highSchool, { last_error: 'rate limited' }), map(college)]
  assert.deepEqual(mapCatalogCount(maps, false), { count: 2, failed: 1, complete: false })
  assert.deepEqual(mapCatalogCount(maps, true), { count: 2, failed: 1, complete: true })
})

test('unread, partial empty, missing identity, and verified empty snapshots differ', () => {
  assert.deepEqual(mapCatalogCount([], false), { count: null, failed: 0, complete: false })
  assert.deepEqual(mapCatalogCount([], true), { count: 0, failed: 0, complete: true })
  assert.deepEqual(mapCatalogCount([map('')], true), { count: null, failed: 0, complete: false })
  assert.deepEqual(mapCatalogCount([map(college), map(undefined)], true), { count: 1, failed: 0, complete: false })
})

test('the catalog count shares the backend identity, current generation, and overview subtitle', () => {
  const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
  assert.match(read('src-tauri/src/map_sources/view.rs'), /map_identity: super::identity::map_identity\(map\)/)
  assert.match(read('src-tauri/src/commands/maps.rs'), /available_maps\(&config, &config.region\)/)
  assert.match(read('src/composables/useMapSources.ts'), /mapsGeneration.value === state.generation.value \? maps.value : \[\]/)
  assert.match(read('src/views/OverviewView.vue'), /const \{ count, failed, complete \} = onlineMapsCount.value/)
  assert.match(read('src/views/OverviewView.vue'), /String\(onlineMapsCount.count\)/)
})
