import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { readFileSync } from 'node:fs'

// Execute the production stores. Only IPC and notification/translation adapters
// are replaced; delays belong to those external boundaries, not copied logic.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* directory below */ }
      try { return next(`${specifier}/index.ts`, context) } catch { /* normal error below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/services/tauri.ts')) return {
      format: 'module', shortCircuit: true,
      source: `export const api = new Proxy({}, {get: (_, key) => (...args) => key === 'rebindLocalMods' && !globalThis.__loadingApi[key] ? Promise.resolve(args[0]) : globalThis.__loadingApi[key](...args)})
        export const watchGameState = async () => () => {}
        export const watchLevelSetCompression = async callback => { globalThis.__compressionEvent = callback; return () => {} }
        export const watchLevelSetScan = async () => () => {}`,
    }
    if (url.endsWith('/src/locale/backend.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const backendMessageOf = String',
    }
    if (url.endsWith('/src/useNotify.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const useNotify = () => ({error: () => {}})',
    }
    return next(url, context)
  },
})

const { useDashboard } = await import('../src/composables/useDashboardStore.ts')
const { useMapSources } = await import('../src/composables/useMapSources.ts')
const { useLevelSetScan } = await import('../src/composables/useLevelSetScan.ts')
const { levelSetIsRefreshing } = await import('../src/levelSetStatus.ts')
const tick = () => new Promise(resolve => setTimeout(resolve, 0))
function deferred() { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }

test('online MOD/map results publish while local scans remain pending; local scans single-flight', async () => {
  const mods = deferred(), sets = deferred()
  let modScans = 0, setScans = 0
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: { repositories: [{ id: 'online-mod' }] }, game: { kitchen_running: false } }),
    localMods: () => { modScans++; return mods.promise },
    maps: async () => [{ id: 'online-map' }],
    levelSets: () => { setScans++; return sets.promise },
  }
  const dashboard = useDashboard(), maps = useMapSources(), local = useLevelSetScan()
  const first = dashboard.ensureLoaded(), second = dashboard.ensureLoaded()
  const scan = local.scanLevelSets(), duplicate = local.scanLevelSets()
  await Promise.all([first, second, maps.loadMaps()])
  assert.equal(dashboard.config.value.repositories.length, 1)
  assert.equal(maps.maps.value.length, 1)
  assert.equal(dashboard.modsLoading.value, true)
  assert.equal(local.scanning.value, true)
  assert.equal(modScans, 1)
  assert.equal(setScans, 1)
  assert.strictEqual(scan, duplicate)
  mods.resolve([{ dll_file: 'local.dll', version: '1.0' }])
  sets.resolve([{ dir_name: 'local-map', name: 'Fast', author: 'A', enabled: true, version: '1.0' }])
  await scan
  await tick()
  assert.equal(dashboard.mods.value.length, 1)
  assert.equal(local.levelSets.value.length, 1)
})

test('local MOD results publish without waiting for a slow summary; a failed online map read preserves local results', async () => {
  const base = deferred()
  globalThis.__loadingApi.dashboard = () => base.promise
  globalThis.__loadingApi.localMods = async () => [{ dll_file: 'new.dll', version: '3.0' }]
  globalThis.__loadingApi.maps = async () => { throw new Error('offline') }
  const dashboard = useDashboard(), maps = useMapSources(), local = useLevelSetScan()
  const reload = dashboard.load()
  await tick()
  assert.equal(dashboard.mods.value[0].dll_file, 'new.dll')
  assert.equal(dashboard.backgroundLoading.value, 1)
  await maps.loadMaps()
  assert.equal(local.levelSets.value.length, 1)
  base.resolve({ config: { repositories: [] }, game: { kitchen_running: false } })
  await reload
})

test('map engine honors cache on cards/checks, forces only full refresh, and updates fast cards before slow ones', async () => {
  const engine = useMapSources()
  const singleMap = { id: 'fast', level_set_name: 'Fast', author: 'A' }
  const slowMap = { id: 'slow', level_set_name: 'Slow', author: 'B' }
  const slow = deferred()
  const source = deferred()
  const requests = []
  globalThis.__loadingApi = {
    refreshMapSources: async (pass, force) => { requests.push(['source', force, pass]); await source.promise; return [singleMap, slowMap] },
    refreshMapReleases: async (id, force, pass) => {
      requests.push([id, force, pass])
      return id === 'slow' ? slow.promise : [{ tag_name: 'v2', assets: [] }]
    },
    syncDependencySnapshots: async () => {},
    maps: async () => engine.maps.value,
  }
  const refresh = engine.refreshAll({ force: true })
  await tick()
  assert.deepEqual(requests.map(r => r[0]), ['source'])
  source.resolve()
  await tick()
  assert.equal(engine.maps.value[0].latest_tag, 'v2')
  assert.equal(engine.refreshingAll.value, true)
  assert.equal(levelSetIsRefreshing({ name: 'Fast', author: 'A' }, engine.maps.value, engine.refreshingHint.value), false)
  assert.equal(levelSetIsRefreshing({ name: 'Slow', author: 'B' }, engine.maps.value, engine.refreshingHint.value), true)
  assert.equal(engine.refreshing.value.slow, true)
  assert.ok(requests.every(r => r[1] === true))
  assert.equal(new Set(requests.map(r => r[2])).size, 1)
  slow.resolve([])
  await refresh
  requests.length = 0
  await engine.refreshReleases([singleMap], { trigger: 'single' })
  assert.deepEqual(requests.map(r => r.slice(0, 2)), [['fast', false]])
  requests.length = 0
  await engine.refreshBoundMaps([singleMap])
  assert.deepEqual(requests.map(r => r.slice(0, 2)), [['source', false], ['fast', false]])
})

test('a late initial map snapshot cannot overwrite a newer source/Release result', async () => {
  const engine = useMapSources()
  const stale = deferred()
  globalThis.__loadingApi.maps = () => stale.promise
  const initial = engine.loadMaps()
  globalThis.__loadingApi.refreshMapSources = async () => [{ id: 'new-map', level_set_name: 'New', author: 'A' }]
  globalThis.__loadingApi.refreshMapReleases = async () => [{ tag_name: 'v3', assets: [] }]
  globalThis.__loadingApi.maps = async () => engine.maps.value
  await engine.refreshAll()
  stale.resolve([{ id: 'stale-map' }])
  await initial
  assert.equal(engine.maps.value[0].id, 'new-map')
  assert.equal(engine.maps.value[0].latest_tag, 'v3')
})

test('late configuration reads cannot roll back a newer per-repository result or a newer read', async () => {
  const dashboard = useDashboard()
  const stale = deferred(), latest = deferred()
  globalThis.__loadingApi.config = () => stale.promise
  const first = dashboard.reloadConfig()
  globalThis.__loadingApi.config = () => latest.promise
  const second = dashboard.reloadConfig()
  latest.resolve({ repositories: [{ id: 'latest' }] })
  await second
  stale.resolve({ repositories: [{ id: 'stale' }] })
  await first
  assert.equal(dashboard.config.value.repositories[0].id, 'latest')
  const beforePatch = deferred()
  globalThis.__loadingApi.config = () => beforePatch.promise
  const pending = dashboard.reloadConfig()
  dashboard.applyConfigSnapshot({ repositories: [{ id: 'patched' }] })
  beforePatch.resolve({ repositories: [{ id: 'old-api-snapshot' }] })
  await pending
  assert.equal(dashboard.config.value.repositories[0].id, 'patched')
})

test('overview and local maps have no combined online/local publication barrier; summary contains no DLL scan', () => {
  const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
  const overview = read('src/views/OverviewView.vue')
  const levels = read('src/views/LevelsView.vue')
  assert.doesNotMatch(overview, /Promise\.all\(/)
  assert.doesNotMatch(levels, /Promise\.all\(\[scanLevelSets\(\), loadMaps\(\)\]/)
  assert.match(overview, /void loadOnlineMaps\(\)/)
  assert.match(overview, /void loadLocalMaps\(\)/)
  const dashboard = read('src-tauri/src/commands/dashboard.rs')
  const summary = dashboard.slice(dashboard.indexOf('fn get_dashboard_blocking'), dashboard.indexOf('pub async fn get_local_mods'))
  assert.doesNotMatch(summary, /scan_mods_with_manifests|collect_update_status/)
})

test('release-tags source results are already update evidence and do not fan out into per-map IPC calls', async () => {
  const engine = useMapSources()
  const derived = { id: 'derived', source_id: 'bundle', release_derived: true, level_set_name: 'Derived', author: 'A', cached_at: Date.now(), cached_releases: [{tag_name:'2.0',assets:[]}], latest_tag:'2.0' }
  const ordinary = { id: 'ordinary', source_id: 'toml', level_set_name: 'Ordinary', author: 'B' }
  const requests = []
  globalThis.__loadingApi = {
    refreshMapSources: async (_pass, force) => {requests.push(['source',force]); return [derived,ordinary]},
    refreshMapReleases: async (id,force) => {requests.push([id,force]);return [{tag_name:'3.0',assets:[]}]},
    maps: async () => engine.maps.value,
    syncDependencySnapshots: async () => {},
  }
  await engine.refreshAll({force:true})
  assert.deepEqual(requests,[['source',true],['ordinary',true]])
  assert.equal(engine.maps.value.find(m=>m.id==='derived').latest_tag,'2.0')
  requests.length=0
  await engine.refreshBoundMaps([derived])
  assert.deepEqual(requests,[['source',false]])
  requests.length=0
  await engine.refreshReleases([derived],{trigger:'single'})
  assert.deepEqual(requests,[['derived',false]],'a standalone card still reads the bundle snapshot via the backend')
})

test('local metadata and each arriving source reconcile bindings in both arrival orders without rescanning DLLs', async () => {
  const d=useDashboard(), scan=deferred(); let scans=0, repos=[];const calls=[]
  const missing={dll_file:'bound.dll',guid:'known.guid',version:'1.0',repository_id:null,repository_ids:[],status:'bound_repository_missing',labels:['bound_repository_missing']}
  globalThis.__loadingApi={
    dashboard:async()=>({config:{repositories:repos},game:{}}),
    config:async()=>({repositories:repos}),
    localMods:async()=>{scans++;return scan.promise},
    rebindLocalMods:async(installed)=>{calls.push(installed.map(m=>m.dll_file));return installed.map(m=>({...m,repository_id:repos.length?'arrived':null,repository_ids:repos.length?['arrived']:[],status:repos.length?'unknown':'bound_repository_missing',labels:repos.length?['unknown']:['bound_repository_missing']}))},
  }
  const loading=d.loadLocalMods();scan.resolve([missing]);await loading
  repos=[{id:'arrived',plugin_guid:'known.guid'}]
  await d.reloadConfig()
  assert.equal(d.mods.value[0].repository_id,'arrived','TOML arrival must reconcile the already-scanned local snapshot')
  assert.equal(d.mods.value[0].labels.includes('bound_repository_missing'),false)
  assert.equal(scans,1)
  globalThis.__loadingApi.localMods=async()=>{scans++;return[missing]}
  await d.loadLocalMods()
  assert.equal(d.mods.value[0].repository_id,'arrived','scan completion must reconcile against repositories that arrived earlier')
  assert.ok(calls.length>=2)
})

test('late binding responses cannot overwrite a newer TOML snapshot and pending rebinds coalesce', async () => {
  const d=useDashboard(), stale=deferred();let calls=0
  globalThis.__loadingApi.rebindLocalMods=async installed=>{
    calls++
    if(calls===1)return stale.promise
    return installed.map(m=>({...m,repository_id:'newest'}))
  }
  globalThis.__loadingApi.config=async()=>({repositories:[{id:'first'}]})
  const reload=d.reloadConfig();await tick()
  d.applyConfigSnapshot({repositories:[{id:'second'}]})
  d.applyConfigSnapshot({repositories:[{id:'newest'}]})
  stale.resolve(d.mods.value.map(m=>({...m,repository_id:'stale'})))
  await reload
  assert.equal(d.mods.value[0].repository_id,'newest')
  assert.equal(calls,2,'multiple source changes while in flight need only one latest reconciliation')
})

test('conversion status is independent of local scanning and receives queue events', async () => {
  globalThis.__loadingApi.levelCompressionProgress = async () => ({ running: false, done: 0, total: 0 })
  const { useLevelCompression } = await import('../src/composables/useLevelCompression.ts')
  const status = useLevelCompression()
  await tick()
  const event = { running: true, done: 1, total: 3, sets_done: 0, sets_total: 1, files_done: 1, files_total: 3 }
  globalThis.__compressionEvent(event)
  assert.equal(useLevelSetScan().scanning.value, false)
  assert.deepEqual(status.compression.value, event)
})
