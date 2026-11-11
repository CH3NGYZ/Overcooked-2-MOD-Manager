import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { readFileSync } from 'node:fs'
import { computed, effectScope, nextTick, ref } from 'vue'

// Exercise production stores, refresh queues and preview lifecycle. Only their
// IPC/notice boundaries are replaced so every attempted real read is counted.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* try directory */ }
      try { return next(`${specifier}/index.ts`, context) } catch { /* normal error */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/services/tauri.ts')) return { format: 'module', shortCircuit: true, source: `
      export const api = new Proxy({}, {get: (_, key) => (...args) => {
        globalThis.__tutorialCalls.push([key, ...args]);
        return globalThis.__tutorialApi[key]?.(...args) ?? Promise.resolve(undefined);
      }})
      export const watchGameState = async () => () => {}
      export const watchLevelSetScan = async () => () => {}
      export const watchLevelSetCompression = async callback => {
        globalThis.__tutorialCalls.push(['watchLevelSetCompression']);
        globalThis.__tutorialCompressionEvent = callback;
        await globalThis.__tutorialCompressionSubscription;
        return () => {};
      }
    ` }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true, source: 'export const useNotify = () => ({error() {}, warning() {}})' }
    if (url.endsWith('/src/locale/backend.ts')) return { format: 'module', shortCircuit: true, source: 'export const backendMessageOf = String; export const translateBackendMessage = String' }
    return next(url, context)
  },
})

globalThis.__tutorialCalls = []
globalThis.__tutorialApi = {}
const demo = await import('../src/tutorial/demo.ts')
const { useDashboard } = await import('../src/composables/useDashboardStore.ts')
const { useLevelSetScan } = await import('../src/composables/useLevelSetScan.ts')
const { useMapSources, requestMapSourcesStartupRefresh } = await import('../src/composables/useMapSources.ts')
const { useModRepositoryRefresh } = await import('../src/composables/useModRepositoryRefresh.ts')
const { useLevelPreview } = await import('../src/composables/useLevelPreview.ts')
const { useModRepositoryReadme } = await import('../src/composables/useModRepositoryReadme.ts')
const { prefetchAvatars } = await import('../src/avatars.ts')
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
const resetCalls = () => { globalThis.__tutorialCalls.length = 0 }
const localMod = name => ({ name, dll_file: `${name}.dll`, enabled: true, version_source: 'metadata', duplicate_guid: false, update_available: false, status: 'latest' })
const realConfig = () => ({ repositories: [], repository_sources: [], region: 'global', platform: 'windows', game: { path: 'real-game', runtime: 'native' } })

test('every shared data/refresh entry stays offline in tutorial and leaves real stores unchanged', async () => {
  const dashboard = useDashboard(), local = useLevelSetScan(), maps = useMapSources(), repositories = useModRepositoryRefresh()
  const config = realConfig()
  const installed = [localMod('real')]
  globalThis.__tutorialApi = {
    dashboard: async () => ({ config, game: { path: 'real-game', kitchen_running: false } }),
    localMods: async () => ({ generation: 1, mods: installed }),
    rebindLocalMods: async mods => mods,
    maps: async () => [{ id: 'real-map', level_set_name: 'Real map', author: 'Real author' }],
    levelSets: async () => [{ dir_name: 'real-set', name: 'Real map', author: 'Real author', enabled: true, version: '1.0' }],
    modUpdates: async () => [],
  }
  await dashboard.load()
  await maps.loadMaps()
  await local.scanLevelSets()
  await tick()
  const before = { dashboard: dashboard.dashboard.value, mods: dashboard.mods.value, maps: maps.maps.value, sets: local.levelSets.value }
  demo.startTutorialDemo()
  resetCalls()
  try {
    await dashboard.load()
    await dashboard.ensureLoaded()
    await dashboard.ensureLocalModsLoaded()
    await dashboard.reloadConfig()
    await dashboard.checkModUpdates()
    await dashboard.recheckGameState()
    await local.scanLevelSets()
    await maps.loadMaps({ force: true })
    await maps.refreshAll({ force: true })
    await maps.refreshSource({ id: 'real-source' })
    await maps.refreshReleases([{ id: 'real-map' }])
    await maps.refreshBoundMaps([{ id: 'real-map' }])
    await maps.syncDependencySnapshots()
    requestMapSourcesStartupRefresh()
    await repositories.refreshOne({ id: 'real-repo' })
    await repositories.refreshMany([{ id: 'real-repo' }])
    await repositories.refreshBoundRepositories([{ id: 'real-repo' }])
    await repositories.refreshSource('real-source')
    await repositories.refreshSourceChain('real-source')
    await repositories.refreshSourceDocuments()
    await repositories.refreshEverything({ force: true })
    repositories.requestStartupRefresh()
    await prefetchAvatars(demo.withDemoRepositories([]))
    await prefetchAvatars(demo.withDemoMaps([]))
    await prefetchAvatars([{ provider: 'github', username: 'real-owner-not-sample' }])
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls, [])
    assert.equal(dashboard.dashboard.value, before.dashboard)
    assert.equal(dashboard.mods.value, before.mods)
    assert.equal(maps.maps.value, before.maps)
    assert.equal(local.levelSets.value, before.sets)
    assert.equal(maps.anyRefreshing.value, false)
    assert.equal(repositories.anyRefreshing.value, false)
  } finally { demo.stopTutorialDemo() }
  assert.equal(demo.withDemoMods(dashboard.mods.value), before.mods)
  assert.equal(demo.withDemoMaps(maps.maps.value), before.maps)
  assert.equal(demo.withDemoLevelSets(local.levelSets.value), before.sets)
})

test('late real summary/scan/map results cannot replace tutorial projections or start follow-up reads', async () => {
  const dashboard = useDashboard(), maps = useMapSources(), local = useLevelSetScan()
  const base = deferred(), mods = deferred(), online = deferred()
  globalThis.__tutorialApi = {
    dashboard: () => base.promise,
    localMods: () => mods.promise,
    maps: () => online.promise,
  }
  const loading = dashboard.load()
  const loadingMaps = maps.loadMaps()
  const scanning = local.scanLevelSets()
  demo.startTutorialDemo()
  const shownMods = computed(() => demo.withDemoMods(dashboard.mods.value))
  const shownMaps = computed(() => demo.withDemoMaps(maps.maps.value))
  const shownSets = computed(() => demo.withDemoLevelSets(local.levelSets.value))
  const before = [shownMods.value, shownMaps.value, shownSets.value]
  resetCalls()
  try {
    mods.resolve({ generation: 2, mods: [localMod('late-real')] })
    base.resolve({ config: realConfig(), game: { path: 'real-game' } })
    online.resolve([{ id: 'late-real-map', level_set_name: 'Late real map', author: 'Real author' }])
    await Promise.all([loading, loadingMaps, scanning])
    await tick()
    assert.equal(shownMods.value, before[0])
    assert.equal(shownMaps.value, before[1])
    assert.equal(shownSets.value, before[2])
    assert.deepEqual(globalThis.__tutorialCalls, [], 'late scans must not trigger rebind, comparisons or map IPC')
    assert.equal(local.scanning.value, false)
  } finally { demo.stopTutorialDemo() }
  assert.equal(shownMods.value, dashboard.mods.value)
  assert.equal(shownMaps.value, maps.maps.value)
  assert.equal(shownSets.value, local.levelSets.value)
})

test('a queued real Release batch stops issuing IPC after tutorial starts', async () => {
  const dashboard = useDashboard(), repositories = useModRepositoryRefresh(), maps = useMapSources()
  const first = deferred(), second = deferred()
  const repoList = Array.from({ length: 6 }, (_, index) => ({ id: `real-repo-${index}`, name: 'real', cached_releases: [], last_error: 'original-error' }))
  const mapList = Array.from({ length: 6 }, (_, index) => ({ id: `real-map-${index}`, level_set_name: `Real ${index}`, author: 'Real author', cached_releases: [], last_error: 'original-error' }))
  globalThis.__tutorialApi = {
    refreshModRepository: () => first.promise,
    refreshMapReleases: () => second.promise,
    config: async () => realConfig(),
    rebindLocalMods: async mods => mods,
    syncDependencySnapshots: async () => {},
    maps: async () => mapList,
  }
  dashboard.applyConfigSnapshot({ ...realConfig(), repositories: repoList })
  await maps.loadMaps({ force: true })
  await tick()
  const beforeRepos = dashboard.config.value.repositories.slice(), beforeMaps = maps.maps.value.slice()
  resetCalls()
  const repoBatch = repositories.refreshMany(repoList)
  const mapBatch = maps.refreshReleases(mapList)
  await tick()
  assert.equal(globalThis.__tutorialCalls.filter(call => call[0] === 'refreshModRepository').length, 4)
  assert.equal(globalThis.__tutorialCalls.filter(call => call[0] === 'refreshMapReleases').length, 4)
  demo.startTutorialDemo()
  resetCalls()
  try {
    first.resolve([])
    second.resolve([])
    await Promise.all([repoBatch, mapBatch])
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls, [], 'queued workers and finalizers stay offline')
    assert.equal(dashboard.config.value.repositories[0].last_error, undefined, 'an already dispatched real result may update its real cache')
    assert.equal(maps.maps.value[0].last_error, null)
    for (const index of [4, 5]) {
      assert.equal(dashboard.config.value.repositories[index], beforeRepos[index], 'a paused worker cannot mark its cached repository successful')
      assert.equal(maps.maps.value[index], beforeMaps[index], 'a paused worker cannot mark its cached map successful')
      assert.equal(beforeRepos[index].last_error, 'original-error')
      assert.equal(beforeMaps[index].last_error, 'original-error')
    }
  } finally { demo.stopTutorialDemo() }
})

test('demo local-map catalog/detail/retry and outgoing dialog never invoke real preview IPC', async () => {
  const scope = effectScope()
  const preview = scope.run(useLevelPreview)
  demo.startTutorialDemo()
  const set = demo.withDemoLevelSets([])[0]
  resetCalls()
  try {
    await preview.open(set)
    await tick()
    assert.equal(preview.levels.value.length, set.level_count)
    assert.equal(preview.current.value.name, set.levels[0].name)
    assert.deepEqual(preview.recipes.value, [])
    preview.retry()
    await tick()
    await preview.open({ dir_name: 'real-before-tour', enabled: true })
    assert.equal(preview.target.value.dir_name, set.dir_name, 'a real target is refused during tutorial')
    demo.stopTutorialDemo()
    await preview.open(set)
    await tick()
    preview.retry()
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls, [])
  } finally { demo.stopTutorialDemo(); scope.stop() }
})

test('repository README pauses real reads, rejects late results and never reads outgoing demo IDs', async () => {
  const scope = effectScope(), id = ref('real-repo-before-tutorial'), old = deferred()
  globalThis.__tutorialApi = {
    repositoryReadme: () => old.promise,
    cancelRepositoryReadme: async () => {},
  }
  resetCalls()
  const detail = scope.run(() => useModRepositoryReadme(() => id.value))
  assert.equal(globalThis.__tutorialCalls.filter(call => call[0] === 'repositoryReadme').length, 1)
  demo.startTutorialDemo()
  assert.equal(globalThis.__tutorialCalls.filter(call => call[0] === 'cancelRepositoryReadme').length, 1)
  resetCalls()
  try {
    old.resolve({ content: 'late real README', exists: true, url: 'real' })
    await tick()
    assert.equal(detail.readme.value, null)
    assert.equal(detail.loading.value, false)
    id.value = demo.withDemoRepositories([])[0].id
    assert.deepEqual(detail.readme.value, demo.demoReadme(id.value))
    await detail.reload()
    demo.stopTutorialDemo()
    await detail.reload()
    assert.deepEqual(globalThis.__tutorialCalls, [], 'demo IDs remain offline through exit and explicit retries')
    demo.startTutorialDemo()
    id.value = 'real-repo-during-tutorial'
    await detail.reload()
    assert.deepEqual(globalThis.__tutorialCalls, [], 'a real ID is also paused while tutorial is active')
    const fresh = { content: 'fresh real README', exists: true, url: 'real' }
    globalThis.__tutorialApi.repositoryReadme = async () => fresh
    demo.stopTutorialDemo()
    await tick()
    assert.deepEqual(detail.readme.value, fresh)
    assert.deepEqual(globalThis.__tutorialCalls.map(call => call[0]), ['repositoryReadme'])
  } finally { demo.stopTutorialDemo(); scope.stop() }
})

test('titlebar conversion status defers initialization, isolates in-flight reads and restores real event cache', async () => {
  const subscription = deferred(), snapshot = deferred()
  globalThis.__tutorialCompressionSubscription = subscription.promise
  globalThis.__tutorialApi.levelCompressionProgress = () => snapshot.promise
  demo.startTutorialDemo()
  resetCalls()
  const { useLevelCompression } = await import('../src/composables/useLevelCompression.ts')
  const status = useLevelCompression()
  const shown = status.compression.value
  try {
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls, [], 'mounting during tutorial must not subscribe or request real status')
    assert.equal(shown.running, false)
    assert.equal(shown.total, 0)
    demo.stopTutorialDemo()
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls.map(call => call[0]), ['watchLevelSetCompression'])
    demo.startTutorialDemo()
    resetCalls()
    subscription.resolve()
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls, [], 'a subscription that finishes during tutorial must not request its snapshot')
    assert.equal(status.compression.value, shown)
    demo.stopTutorialDemo()
    await tick()
    assert.deepEqual(globalThis.__tutorialCalls.map(call => call[0]), ['levelCompressionProgress'])
    demo.startTutorialDemo()
    resetCalls()
    const event = { running: true, done: 2, total: 3, current: 'real-map' }
    globalThis.__tutorialCompressionEvent(event)
    snapshot.resolve({ running: false, done: 0, total: 0 })
    await tick()
    assert.equal(status.compression.value, shown, 'real events and delayed snapshots cannot replace the static tutorial status')
    assert.deepEqual(globalThis.__tutorialCalls, [])
    demo.stopTutorialDemo()
    await tick()
    assert.equal(status.compression.value, event, 'exit immediately restores the latest real event, preserving the revision check')
    assert.deepEqual(globalThis.__tutorialCalls, [], 'the existing subscription and cache are reused after exit')
  } finally {
    subscription.resolve()
    snapshot.resolve(null)
    demo.stopTutorialDemo()
  }
})

test('page wiring projects every data lane, skips mount/drop reads and restores live loading on exit', () => {
  const read = file => readFileSync(new URL(`../src/views/${file}.vue`, import.meta.url), 'utf8')
  for (const file of ['ModsLocalView', 'ModsOnlineView', 'MapsOnlineView', 'LevelsView', 'OverviewView']) {
    const source = read(file)
    assert.match(source, /withDemoConfig\(realConfig\.value\)/, file)
    assert.match(source, /withDemoMods\((?:realMods|scannedMods)\.value\)/, file)
    assert.match(source, /onMounted\(\(\) => \{\s*if \(demoActive\.value\) return/, file)
    assert.match(source, /watch\(demoActive, active =>/, file)
    if (file !== 'OverviewView') assert.match(source, /busy: \(\) => demoActive\.value \|\|/, file)
  }
  for (const file of ['MapsOnlineView', 'LevelsView', 'OverviewView']) {
    const source = read(file)
    assert.match(source, /withDemoMaps\(realMaps\.value\)/, file)
    assert.match(source, /withDemoLevelSets\(real(?:Level)?Sets\.value\)/, file)
  }
  assert.match(read('MapsOnlineView'), /withDemoMapSourceNodes\(realSourceNodes\.value\)/)
  assert.match(read('LevelsView'), /busyReader\(\(\) => demoActive\.value \|\|/)
  assert.match(read('OverviewView'), /async function loadDiyLevel\(\) \{\s*if \(demoActive\.value\) return/)
  const titlebar = readFileSync(new URL('../src/components/AppTitleBar.vue', import.meta.url), 'utf8')
  assert.match(titlebar, /withDemoGame\(realGame\.value\)/)
  assert.match(titlebar, /async function toggleGame\(\) \{\s*if \(demoActive\.value \|\|/)
})
