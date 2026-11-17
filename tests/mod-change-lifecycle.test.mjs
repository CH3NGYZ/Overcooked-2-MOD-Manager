import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { createRenderer, nextTick, shallowRef } from 'vue'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const modules = {
      '/src/composables/useDashboardStore.ts': 'export const useDashboard=()=>globalThis.__changeLifecycle.dashboard',
      '/src/composables/useMapSources.ts': 'export const useMapSources=()=>globalThis.__changeLifecycle.maps',
      '/src/composables/useLevelSetScan.ts': 'export const useLevelSetScan=()=>globalThis.__changeLifecycle.levels',
      '/src/modChanges/store.ts': 'export const useModChanges=()=>globalThis.__changeLifecycle.changes',
      '/src/services/tauri.ts': 'export const watchModChangesApplied=callback=>globalThis.__changeLifecycle.listen(callback)',
      '/src/tutorial/demo.ts': 'import{shallowRef}from"vue";export const demoActive=shallowRef(false)',
      '/src/runtime.ts': 'export const isTauri=()=>true',
    }
    for (const [path, source] of Object.entries(modules)) if (url.endsWith(path)) return { format: 'module', shortCircuit: true, source }
    return next(url, context)
  },
})
const { useModChangesLifecycle } = await import('../src/modChanges/useModChangesLifecycle.ts')
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
const deferred = () => { let resolve; const promise = new Promise(yes => { resolve = yes }); return { promise, resolve } }
const host = createRenderer({ patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null })

async function setup(t, registration) {
  const calls = [], listeners = new Set()
  let load = async () => {}, stops = 0
  const fixture = {
    dashboard: { config: shallowRef({}), mods: shallowRef([]), game: shallowRef({ path: 'game', kitchen_running: false }),
      load: async () => { calls.push('mods'); await load(); fixture.dashboard.mods.value = [{}] } },
    maps: { maps: shallowRef([]), loadMaps: async options => { assert.deepEqual(options, { force: true }); calls.push('maps'); fixture.maps.maps.value = [{}] } },
    levels: { levelSets: shallowRef([]), scanLevelSets: async () => { calls.push('levels'); fixture.levels.levelSets.value = [{}] } },
    changes: { clearEvaluation: () => calls.push('clear'), invalidate: async () => { calls.push('evaluate') } },
    listen: async callback => { if (registration) await registration.promise; listeners.add(callback); return () => { stops++; listeners.delete(callback) } },
  }
  globalThis.__changeLifecycle = fixture
  const app = host.createApp({ setup() { useModChangesLifecycle(); return () => null } })
  app.mount({})
  let disposed = false
  const dispose = () => { if (!disposed) { disposed = true; app.unmount() } }
  t.after(dispose)
  await tick()
  return { ...fixture, calls, get stops() { return stops }, listeners, dispose, setLoad(value) { load = value }, emit() { for (const callback of listeners) callback() } }
}

test('one applied batch refreshes both domains before reevaluating, with no intermediate evaluation', async t => {
  const f = await setup(t), hold = deferred()
  f.setLoad(() => hold.promise)
  f.emit(); f.emit(); await tick()
  assert.deepEqual(f.calls.filter(call => call !== 'clear'), ['mods', 'maps'])
  assert.equal(f.calls.includes('evaluate'), false)
  hold.resolve(); await tick(); await tick()
  assert.deepEqual(f.calls.filter(call => call !== 'clear'), ['mods', 'maps', 'levels', 'evaluate'])
  f.dispose(); assert.equal(f.stops, 1); assert.equal(f.listeners.size, 0)
})

test('a later applied batch during refresh receives another complete refresh', async t => {
  const f = await setup(t), hold = deferred()
  let loads = 0
  f.setLoad(async () => { if (++loads === 1) await hold.promise })
  f.emit(); await tick(); f.emit(); hold.resolve(); await tick(); await tick()
  assert.equal(loads, 2)
  assert.deepEqual(f.calls.filter(call => call !== 'clear'), ['mods', 'maps', 'levels', 'evaluate', 'mods', 'maps', 'levels', 'evaluate'])
})

test('game and source snapshot changes invalidate the shared evaluation once per watcher flush', async t => {
  const f = await setup(t)
  f.dashboard.config.value = { source: 'new' }; f.dashboard.game.value = { path: 'other-game', kitchen_running: false }
  await tick()
  assert.deepEqual(f.calls, ['clear', 'evaluate'])
})

test('unmount cleans a late event subscription and never retains a page-owned listener', async t => {
  const registration = deferred(), f = await setup(t, registration)
  f.dispose(); registration.resolve(); await tick()
  assert.equal(f.stops, 1); assert.equal(f.listeners.size, 0); assert.deepEqual(f.calls, [])
})
