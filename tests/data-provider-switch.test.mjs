import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { readFileSync } from 'node:fs'

// The real switch coordinator, refresh state and both domain engines run here.
// Only IPC / messages / localization are fixtures; no user files are touched.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal below */ }
      try { return next(`${specifier}/index.ts`, context) } catch { /* normal below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/services/tauri.ts')) return {
      format: 'module', shortCircuit: true,
      source: `export const api = new Proxy({}, {get: (_, key) => async (...args) => key === 'rebindLocalMods' ? args[0] : globalThis.__providerApi[key](...args)})
        export const watchGameState = async () => () => {}
        export const watchLevelSetCompression = async () => () => {}
        export const watchLevelSetScan = async () => () => {}`,
    }
    if (url.endsWith('/src/locale/backend.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const backendMessageOf = String; export const translateBackendMessage = String',
    }
    if (url.endsWith('/src/useNotify.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const useNotify = () => ({error: message => globalThis.__providerErrors.push(message)})',
    }
    if (url.endsWith('/src/i18n.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const pickI18n = (_messages, fallback) => fallback',
    }
    return next(url, context)
  },
})

const { useDataProviderSwitch } = await import('../src/composables/useDataProviderSwitch.ts')
const { useDashboard } = await import('../src/composables/useDashboardStore.ts')
const { useMapSources } = await import('../src/composables/useMapSources.ts')
const { useModRepositoryRefresh } = await import('../src/composables/useModRepositoryRefresh.ts')
const { demoActive } = await import('../src/tutorial/demo.ts')
const tick = () => new Promise(resolve => setTimeout(resolve, 0))
function deferred() { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
const source = (id, parent_id) => ({ id, parent_id, nickname: id, url: `${id}.toml`, enabled: true })
const repository = id => ({ id, source_id: 'child', provider: 'github', username: 'u', repository: id, name: id, enabled: true, download_mode: 'release', cached_releases: [] })
const map = id => ({ id, source_id: 'maps-child', name: id, enabled: true, provider: 'github', username: 'u', repository: id, cached_releases: [] })
const node = (id, parent_id, status = 'ready') => ({ id, parent_id, label: id, status })

function setup() {
  let config = {
    data_provider: 'github', region: 'international', platform: 'windows',
    repository_sources: [source('root')], repositories: [], network_policy: { id: 'old-plan' }, github_proxies: { id: 'old-proxies' },
  }
  const calls = [], errors = [], packageReady = deferred()
  globalThis.__providerErrors = errors
  const api = globalThis.__providerApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    config: async () => structuredClone(config),
    maps: async () => [map('trusted-map')],
    saveSettings: async patch => {
      if (patch.region) {
        calls.push(['region', patch.region])
        config = { ...config, region: patch.region, data_provider: patch.region === 'cn' ? 'gitcode' : 'github' }
      } else {
        calls.push(['select', patch.data_provider])
        config = { ...config, data_provider: patch.data_provider }
      }
      return structuredClone(config)
    },
    refreshNetworkConfig: async () => {
      calls.push(['zip'])
      await packageReady.promise
      config = { ...config, network_policy: { id: 'new-plan' }, github_proxies: { id: 'new-proxies' } }
      return structuredClone(config)
    },
    importRepositorySource: async (id, _url, fallback, force) => {
      calls.push(['toml', id, fallback, force])
      if (id === 'root') config = { ...config, repository_sources: [source('root'), source('child', 'root')], repositories: [] }
      if (id === 'child') config = { ...config, repositories: [repository('fresh-mod')] }
      return { access_error: null }
    },
    refreshMapSources: async (_pass, force, progress) => {
      calls.push(['maps.toml', force])
      const result = { nodes: [node('maps-root'), node('maps-child', 'maps-root')], maps: [map('fresh-map')], excluded_map_identities: [] }
      progress(result)
      return result
    },
    refreshModRepository: async (id, force) => { calls.push(['mod-release', id, force]); return { cached_releases: [{ tag_name: 'new' }] } },
    refreshMapReleases: async (id, force) => { calls.push(['map-release', id, force]); return { ...map(id), latest_tag: 'new' } },
    syncDependencySnapshots: async () => {},
  }
  return { calls, errors, packageReady, api, get config() { return config }, set config(value) { config = value } }
}

test('a provider switch downloads one package before both real source engines rebuild their roots and children without force', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  await dashboard.loadBase(); await maps.loadMaps()
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'cloudflare' })
  await tick()
  assert.deepEqual(f.calls, [['select', 'cloudflare'], ['zip']])
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  assert.equal(useDataProviderSwitch().switching.value, true)
  f.packageReady.resolve(); const result = await pending
  assert.equal(result.ready, true)
  assert.deepEqual(result.errors, [])
  assert.equal(f.calls.filter(([kind]) => kind === 'zip').length, 1)
  assert.deepEqual(f.calls.filter(([kind]) => kind === 'toml'), [['toml', 'root', false, false], ['toml', 'child', false, false]])
  assert.deepEqual(f.calls.filter(([kind]) => kind === 'maps.toml'), [['maps.toml', false]])
  assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), ['fresh-mod'])
  assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
  assert.equal(maps.maps.value[0].latest_tag, 'new')
  assert.equal(dashboard.config.value.network_policy.id, 'new-plan')
  assert.equal(dashboard.config.value.github_proxies.id, 'new-proxies')
  assert.equal(dashboard.config.value.data_provider, 'cloudflare')
  assert.ok(f.calls.every(([kind, , force]) => !kind.endsWith('-release') || force === false))
  assert.equal(useDataProviderSwitch().switching.value, false)
})

test('switch waits for a busy old MOD request, skips its remaining queue, rejects its stale write, then refreshes both domains', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  f.config = { ...f.config, repository_sources: [source('root'), source('child', 'root')], repositories: [repository('shared-id')] }
  await dashboard.loadBase()
  const old = deferred(), calls = []
  f.api.refreshModRepository = async (id, force) => {
    calls.push(id)
    if (id === 'shared-id' || id.startsWith('old-')) { await old.promise; return { cached_releases: [{ tag_name: 'obsolete' }] } }
    return { cached_releases: [{ tag_name: 'new' }] }
  }
  const oldBatch = mods.refreshMany([repository('shared-id'), ...Array.from({length: 9}, (_, i) => repository(`old-${i}`))])
  await tick()
  assert.equal(calls.length, 4)
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'builtin' })
  await tick()
  assert.equal(f.calls.some(([kind]) => kind === 'toml'), false, 'new tree waits for the old batch to finish')
  old.resolve(); await oldBatch
  assert.equal(dashboard.config.value.repositories[0].cached_releases.length, 0, 'the old result cannot publish even before the new tree starts')
  f.packageReady.resolve(); const result = await pending
  assert.equal(result.ready, true)
  assert.equal(calls.filter(id => id.startsWith('old-')).length, 3, 'queued old work must never reach IPC')
  assert.equal(dashboard.config.value.repositories.some(item => item.cached_releases?.[0]?.tag_name === 'obsolete'), false)
  assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
})

test('a busy old MOD source cannot publish descendants or start their requests after a selection change', async () => {
  const f = setup(), dashboard = useDashboard(), mods = useModRepositoryRefresh()
  f.config = { ...f.config, repositories: [repository('trusted-mod')] }
  await dashboard.loadBase()
  const old = deferred(), importFresh = f.api.importRepositorySource
  f.api.importRepositorySource = async id => { f.calls.push(['obsolete-toml', id]); await old.promise; return { access_error: null } }
  const oldBatch = mods.refreshEverything()
  await tick()
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'cloudflare' })
  await tick()
  old.resolve(); await oldBatch
  assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), ['trusted-mod'])
  assert.equal(f.calls.some(([kind]) => kind.endsWith('-release')), false)
  assert.deepEqual(f.calls.filter(([kind]) => kind === 'obsolete-toml'), [['obsolete-toml', 'root']])
  f.api.importRepositorySource = importFresh
  f.packageReady.resolve(); await pending
  assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), ['fresh-mod'])
})

test('a busy old map Release cannot publish success or failure after a selection change', async () => {
  for (const fail of [false, true]) {
    const f = setup(), dashboard = useDashboard(), maps = useMapSources()
    await dashboard.loadBase(); await maps.loadMaps()
    const old = deferred(), refreshFresh = f.api.refreshMapReleases
    f.api.refreshMapReleases = async () => { await old.promise; if (fail) throw Error('obsolete error'); return { ...map('trusted-map'), latest_tag: 'obsolete' } }
    const oldBatch = maps.refreshReleases([map('trusted-map')])
    await tick()
    const pending = useDataProviderSwitch().switchSelection({ data_provider: 'builtin' })
    await tick()
    old.resolve(); await oldBatch
    assert.equal(maps.maps.value[0].latest_tag, undefined)
    assert.equal(maps.maps.value[0].last_error, undefined)
    f.api.refreshMapReleases = refreshFresh
    f.packageReady.resolve(); await pending
    assert.equal(maps.maps.value[0].latest_tag, 'new')
  }
})

test('invalidated old map channel callbacks cannot mutate trusted cards while the newly selected package is pending', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources()
  await dashboard.loadBase(); await maps.loadMaps()
  const old = deferred(); let publish
  const refreshFresh = f.api.refreshMapSources
  const obsolete = { nodes: [node('old-map-root')], maps: [map('obsolete-map')], excluded_map_identities: [] }
  f.api.refreshMapSources = async (_pass, _force, onProgress) => { publish = onProgress; await old.promise; return obsolete }
  const oldBatch = maps.refreshAll()
  await tick()
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'gitcode' })
  await tick()
  publish(obsolete)
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  f.api.refreshMapSources = refreshFresh
  old.resolve(); await oldBatch
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  f.packageReady.resolve(); await pending
  assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
})

test('package failure leaves both trusted source displays intact and never starts either source rebuild', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  await dashboard.loadBase(); await maps.loadMaps()
  const trustedMods = mods.visibleRepositories.value.map(item => item.id)
  f.api.refreshNetworkConfig = async () => { f.calls.push(['zip']); throw Error('invalid package') }
  const result = await useDataProviderSwitch().switchSelection({ data_provider: 'gitee' })
  assert.equal(result.ready, false)
  assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), trustedMods)
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  assert.deepEqual(f.calls, [['select', 'gitee'], ['zip']])
  await mods.refreshOne(repository('after-failure'))
  await maps.refreshReleases([map('trusted-map')])
  assert.ok(f.calls.some(([kind, id]) => kind === 'mod-release' && id === 'after-failure'))
  assert.ok(f.calls.some(([kind]) => kind === 'map-release'))
  assert.equal(useDataProviderSwitch().switching.value, false)
})

test('source switching rejects new single, group and full refresh work during package preparation and old-task drain', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  await dashboard.loadBase(); await maps.loadMaps()
  const old = deferred(), refreshFresh = f.api.refreshModRepository
  f.api.refreshModRepository = async id => { f.calls.push(['old-release', id]); await old.promise; return { cached_releases: [] } }
  const oldTask = mods.refreshOne(repository('old-busy'))
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'cloudflare' })
  await tick()
  const before = structuredClone(f.calls)
  async function tryRefresh() {
    await Promise.all([
      mods.refreshOne(repository('blocked-single')),
      mods.refreshMany([repository('blocked-group')]),
      mods.refreshSource('root'), mods.refreshSourceChain('root'),
      mods.refreshSourceDocuments(), mods.refreshEverything(),
      mods.refreshBoundRepositories([repository('blocked-bound')]),
      maps.refreshReleases([map('blocked-map')]),
      maps.refreshSource({ id: 'maps-root' }), maps.refreshAll(),
      maps.refreshBoundMaps([map('blocked-bound-map')]),
    ])
    assert.deepEqual(f.calls, before, 'blocked entrances must not reach either source or Release IPC')
    assert.equal(mods.anyRefreshing.value, true)
    assert.equal(maps.anyRefreshing.value, true, 'the pause gate is shared across domains')
  }
  await tryRefresh()
  f.packageReady.resolve(); await tick()
  await tryRefresh()
  f.api.refreshModRepository = refreshFresh
  old.resolve(); await oldTask; const result = await pending
  assert.equal(result.ready, true, 'whenIdle must ignore the scheduling pause itself')
  assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), ['fresh-mod'])
  assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
  assert.equal(mods.anyRefreshing.value, false)
  assert.equal(maps.anyRefreshing.value, false)
})

test('a failed provider selection releases the shared scheduling gate for both domains', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  await dashboard.loadBase(); await maps.loadMaps()
  f.api.saveSettings = async () => { throw Error('selection failed') }
  await assert.rejects(useDataProviderSwitch().switchSelection({ data_provider: 'builtin' }), /selection failed/)
  await mods.refreshOne(repository('after-selection-error'))
  await maps.refreshReleases([map('trusted-map')])
  assert.ok(f.calls.some(([kind]) => kind === 'mod-release'))
  assert.ok(f.calls.some(([kind]) => kind === 'map-release'))
  assert.equal(useDataProviderSwitch().switching.value, false)
  assert.equal(mods.anyRefreshing.value, false)
  assert.equal(maps.anyRefreshing.value, false)
})

test('an invalidated map source finally cannot roll the selected provider back through its config projection', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources()
  await dashboard.loadBase(); await maps.loadMaps()
  const old = deferred(), refreshFresh = f.api.refreshMapSources, configFresh = f.api.config
  const staleConfig = structuredClone(f.config)
  f.api.refreshMapSources = async () => { await old.promise; return { nodes: [], maps: [map('obsolete-map')], excluded_map_identities: [] } }
  const oldTask = maps.refreshAll()
  await tick()
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'cloudflare' })
  await tick()
  assert.equal(dashboard.config.value.data_provider, 'cloudflare')
  // This read belongs to the invalidated old source operation; even a stale
  // provider received after saveSettings may not publish into the new choice.
  f.api.config = async () => staleConfig
  old.resolve(); await oldTask
  assert.equal(dashboard.config.value.data_provider, 'cloudflare')
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  f.api.refreshMapSources = refreshFresh; f.api.config = configFresh
  f.packageReady.resolve(); await pending
  assert.equal(dashboard.config.value.data_provider, 'cloudflare')
})

test('a package failure keeps new scheduling paused until the invalidated old request has released its flags', async () => {
  const f = setup(), dashboard = useDashboard(), mods = useModRepositoryRefresh(), maps = useMapSources()
  await dashboard.loadBase(); await maps.loadMaps()
  const old = deferred(), refreshFresh = f.api.refreshModRepository
  f.api.refreshModRepository = async () => { await old.promise; return { cached_releases: [] } }
  const oldTask = mods.refreshOne(repository('old-busy'))
  f.api.refreshNetworkConfig = async () => { throw Error('package failure') }
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'builtin' })
  await tick()
  assert.equal(useDataProviderSwitch().switching.value, true)
  await maps.refreshReleases([map('blocked-until-drain')])
  assert.equal(f.calls.some(([kind]) => kind === 'map-release'), false)
  old.resolve(); await oldTask
  assert.equal((await pending).ready, false)
  f.api.refreshModRepository = refreshFresh
  await mods.refreshOne(repository('old-busy'))
  await maps.refreshReleases([map('trusted-map')])
  assert.ok(f.calls.some(([kind]) => kind === 'mod-release'))
  assert.ok(f.calls.some(([kind]) => kind === 'map-release'))
  assert.equal(mods.anyRefreshing.value, false)
  assert.equal(maps.anyRefreshing.value, false)
})

test('an invalidated local map snapshot cannot overwrite trusted cards during package preparation', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources()
  await dashboard.loadBase(); await maps.loadMaps()
  const old = deferred()
  f.api.maps = async () => { await old.promise; return [map('obsolete-local-map')] }
  const oldTask = maps.loadMaps()
  const pending = useDataProviderSwitch().switchSelection({ data_provider: 'cloudflare' })
  await tick()
  old.resolve(); await oldTask
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  f.packageReady.resolve(); await pending
  assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
})

test('the module-level switch lock blocks a second Settings instance until the selected package has applied', async () => {
  const f = setup(), dashboard = useDashboard()
  await dashboard.loadBase()
  const first = useDataProviderSwitch().switchSelection({ data_provider: 'github' })
  await tick()
  assert.equal(await useDataProviderSwitch().switchSelection({ data_provider: 'cloudflare' }), null)
  assert.deepEqual(f.calls.filter(([kind]) => kind === 'select'), [['select', 'github']])
  f.packageReady.resolve(); await first
})

test('region selection delegates default source choice to the backend and refreshes both real source engines once', async () => {
  for (const region of ['cn', 'global']) {
    const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
    f.config = { ...f.config, region: region === 'cn' ? 'global' : 'cn', data_provider: 'cloudflare' }
    await dashboard.loadBase(); await maps.loadMaps()
    const select = f.api.saveSettings
    f.api.saveSettings = async patch => { assert.deepEqual(patch, { region }); return select(patch) }
    const pending = useDataProviderSwitch().switchSelection({ region })
    await tick()
    assert.deepEqual(f.calls, [['region', region], ['zip']])
    assert.equal(dashboard.config.value.region, region)
    assert.equal(dashboard.config.value.data_provider, region === 'cn' ? 'gitcode' : 'github')
    f.packageReady.resolve(); const result = await pending
    assert.equal(result.ready, true)
    assert.equal(result.config.region, region)
    assert.equal(f.calls.filter(([kind]) => kind === 'zip').length, 1)
    assert.deepEqual(f.calls.filter(([kind]) => kind === 'toml'), [['toml', 'root', false, false], ['toml', 'child', false, false]])
    assert.deepEqual(f.calls.filter(([kind]) => kind === 'maps.toml'), [['maps.toml', false]])
    assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), ['fresh-mod'])
    assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
    assert.equal(dashboard.config.value.network_policy.id, 'new-plan')
    assert.equal(dashboard.config.value.github_proxies.id, 'new-proxies')
    assert.ok(f.calls.every(([kind, , force]) => !kind.endsWith('-release') || force === false))
  }
})

test('changing region invalidates old MOD results, map channels and config projections even when the provider stays the same', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  f.config = { ...f.config, region: 'global', data_provider: 'gitcode', repositories: [repository('trusted-mod')] }
  await dashboard.loadBase(); await maps.loadMaps()
  const staleConfig = structuredClone(f.config), old = deferred()
  const mapFresh = f.api.refreshMapSources, modFresh = f.api.refreshModRepository, configFresh = f.api.config
  let publish
  const obsolete = { nodes: [node('obsolete-map-root')], maps: [map('obsolete-map')], excluded_map_identities: [] }
  f.api.refreshMapSources = async (_pass, _force, progress) => { publish = progress; await old.promise; return obsolete }
  f.api.refreshModRepository = async () => { await old.promise; return { cached_releases: [{ tag_name: 'obsolete' }] } }
  const oldMap = maps.refreshAll(), oldMod = mods.refreshOne(repository('trusted-mod'))
  await tick()
  const pending = useDataProviderSwitch().switchSelection({ region: 'cn' })
  await tick()
  assert.equal(dashboard.config.value.data_provider, 'gitcode')
  publish(obsolete)
  f.api.config = async () => staleConfig
  old.resolve(); await Promise.all([oldMap, oldMod])
  assert.equal(dashboard.config.value.region, 'cn')
  assert.equal(dashboard.config.value.repositories[0].cached_releases.length, 0)
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  f.api.refreshMapSources = mapFresh; f.api.refreshModRepository = modFresh; f.api.config = configFresh
  f.packageReady.resolve(); await pending
  assert.equal(dashboard.config.value.region, 'cn')
  assert.deepEqual(maps.maps.value.map(item => item.id), ['fresh-map'])
})

test('region save failure preserves trusted configuration and releases the shared lock for retry', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources()
  await dashboard.loadBase(); await maps.loadMaps()
  const trusted = structuredClone(dashboard.config.value), select = f.api.saveSettings
  f.api.saveSettings = async () => { throw Error('region save failed') }
  await assert.rejects(useDataProviderSwitch().switchSelection({ region: 'cn' }), /region save failed/)
  assert.deepEqual(dashboard.config.value, trusted)
  assert.deepEqual(f.calls, [])
  assert.equal(useDataProviderSwitch().switching.value, false)
  f.api.saveSettings = select; f.packageReady.resolve()
  assert.equal((await useDataProviderSwitch().switchSelection({ region: 'cn' })).ready, true)
})

test('package failure keeps the saved region and backend fallback source while preserving trusted lists', async () => {
  const f = setup(), dashboard = useDashboard(), maps = useMapSources(), mods = useModRepositoryRefresh()
  f.config = { ...f.config, repositories: [repository('trusted-mod')] }
  await dashboard.loadBase(); await maps.loadMaps()
  f.api.refreshNetworkConfig = async () => {
    f.calls.push(['zip'])
    f.config = { ...f.config, data_provider: 'builtin' }
    throw Error('package failure')
  }
  const result = await useDataProviderSwitch().switchSelection({ region: 'cn' })
  assert.equal(result.ready, false)
  assert.equal(result.config.region, 'cn')
  assert.equal(result.config.data_provider, 'builtin')
  assert.deepEqual(mods.visibleRepositories.value.map(item => item.id), ['trusted-mod'])
  assert.deepEqual(maps.maps.value.map(item => item.id), ['trusted-map'])
  assert.deepEqual(f.calls, [['region', 'cn'], ['zip']])
  assert.equal(useDataProviderSwitch().switching.value, false)
})

test('region and manual provider selection share the same cross-instance lock', async () => {
  for (const [first, second, calls] of [
    [{ region: 'cn' }, { data_provider: 'cloudflare' }, [['region', 'cn']]],
    [{ data_provider: 'cloudflare' }, { region: 'cn' }, [['select', 'cloudflare']]],
  ]) {
    const f = setup(), dashboard = useDashboard()
    await dashboard.loadBase()
    const pending = useDataProviderSwitch().switchSelection(first)
    await tick()
    assert.equal(await useDataProviderSwitch().switchSelection(second), null)
    assert.deepEqual(f.calls.filter(([kind]) => kind === 'select' || kind === 'region'), calls)
    f.packageReady.resolve(); await pending
  }
})

test('tutorial mode blocks both region and provider selection without saving or preparing a package', async () => {
  const f = setup()
  demoActive.value = true
  try {
    assert.equal(await useDataProviderSwitch().switchSelection({ region: 'cn' }), null)
    assert.equal(await useDataProviderSwitch().switchSelection({ data_provider: 'builtin' }), null)
    assert.deepEqual(f.calls, [])
    assert.equal(useDataProviderSwitch().switching.value, false)
  } finally { demoActive.value = false }
})

test('Settings uses the shared switch coordinator instead of a page-local network-only refresh', () => {
  const view = readFileSync(new URL('../src/views/SettingsView.vue', import.meta.url), 'utf8')
  assert.match(view, /switching: switchingDataProvider, switchSelection \} = useDataProviderSwitch\(\)/)
  assert.match(view, /const result = await switchSelection\(patch\)/)
  assert.doesNotMatch(view, /api\.refreshNetworkConfig\(/)
})
