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
      source: `export const api = new Proxy({}, {get: (_, key) => async (...args) => key === 'rebindLocalMods' && !globalThis.__loadingApi[key] ? args[0] : globalThis.__loadingApi[key](...args)})
        export const watchGameState = async () => () => {}
        export const watchLevelSetCompression = async callback => { globalThis.__compressionEvent = callback; return () => {} }
        export const watchLevelSetScan = async () => () => {}`,
    }
    if (url.endsWith('/src/locale/backend.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const backendMessageOf = String; export const translateBackendMessage = String',
    }
    if (url.endsWith('/src/useNotify.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const useNotify = () => ({error: () => {}})',
    }
    if (url.endsWith('/src/i18n.ts')) return {
      format: 'module', shortCircuit: true,
      source: "export const pickI18n = (messages, fallback) => messages?.['zh-CN'] || fallback",
    }
    return next(url, context)
  },
})


const { useDashboard } = await import('../src/composables/useDashboardStore.ts')
const { useMapSources } = await import('../src/composables/useMapSources.ts')
const { useModRepositoryRefresh } = await import('../src/composables/useModRepositoryRefresh.ts')
const { createSourceTreeState } = await import('../src/sourceTreeState.ts')
const { buildSourceGroupTree } = await import('../src/composables/useSourceGroupTree.ts')
const tick = () => new Promise(resolve => setTimeout(resolve, 0))
function deferred() { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b }); return {promise,resolve,reject} }
const node = (id, parent_id, status = 'pending', error = null) => ({id,parent_id,label:id,url:id+'.toml',status,error})
const repo = (id, source_id) => ({id,source_id,provider:'github',username:'u',repository:id,name:id,enabled:true,download_mode:'release',cached_releases:[]})

test('shared source state clears only descendants and keeps failed empty declarations retryable', () => {
  const state = createSourceTreeState()
  state.declare([node('root'),node('child','root'),node('grandchild','child'),node('sibling','root')])
  assert.deepEqual([...state.begin('child')], ['child','grandchild'])
  assert.deepEqual(state.nodes.value.map(n=>n.id), ['root','child','sibling'])
  state.settle('child','offline')
  assert.equal(state.nodes.value[1].status,'error')
  state.begin('child')
  assert.equal(state.nodes.value[1].error,null)
  const tree = buildSourceGroupTree([{sourceId:'sibling',group:'',item:repo('map','sibling')}], {
    order: state.nodes.value.map(n=>({id:n.id,parentId:n.parent_id,label:n.label,status:n.status})),
    preserveEmpty:true,ungrouped:{id:'none',label:'none'},
  })
  assert.equal(tree.find(n=>n.id==='root').count,1)
  assert.equal(tree.find(n=>n.id==='child').groups.length,0)
})

test('source redeclaration replaces metadata and removes deleted notes while preserving loading and error state', () => {
  const state = createSourceTreeState()
  state.declare([{ ...node('same'), label: 'Old', note_i18n: { en: 'Old note' } }])
  state.begin('same')
  state.declare([{ ...node('same'), label: 'New', url: 'new.toml', note_i18n: { en: 'New note' } }])
  assert.deepEqual(state.nodes.value[0], { ...node('same', undefined, 'loading'), label: 'New', url: 'new.toml', note_i18n: { en: 'New note' } })
  state.settle('same', 'offline')
  state.declare([{ ...node('same'), label: 'Without note' }])
  assert.equal(state.nodes.value[0].note_i18n, undefined)
  assert.equal(state.nodes.value[0].label, 'Without note')
  assert.equal(state.nodes.value[0].status, 'error')
  assert.equal(state.nodes.value[0].error, 'offline')
})

test('MOD engine publishes root and pending child titles, waits for every TOML, hides discovery, then refreshes installed first', async () => {
  const root = deferred(), child = deferred(), failed = deferred()
  const source = (id,parent_id) => ({id,parent_id,nickname:id,nickname_i18n:{},note_i18n:{en:id+' source note'},url:id+'.toml',enabled:true})
  let config = {repositories:[],repository_sources:[source('root')],region:'international',platform:'windows'}
  const calls=[]
  let retry=false
  globalThis.__loadingApi = {
    dashboard:async()=>({config:structuredClone(config),game:{kitchen_running:false}}),
    localMods:async()=>({generation:1,mods:[{dll_file:'bound.dll',repository_id:'bound',labels:[],enabled:true}]}),
    modUpdates:async()=>[],config:async()=>structuredClone(config),
    clearRepositorySourceCache:async()=>structuredClone(config),
    importRepositorySource:async(id)=>{
      calls.push('toml:'+id)
      if(id==='root') { await root.promise; config={...config,repositories:[repo('ordinary','root')],repository_sources:[source('root'),source('child','root'),source('failed','root')]} }
      if(id==='child') { await child.promise; config.repositories.push(repo('bound','child')) }
      if(id==='failed') { if(!retry) await failed.promise; else config.repositories.push(repo('recovered','failed')) }
      return {access_error:null}
    },
    refreshModRepository:async(id,force)=>{calls.push(['api',id,force]);return{cached_releases:[],release_prerelease_metadata:{}}},
    discoverCommunityRepositories:async()=>{throw Error('discovery must stay paused')},
    syncDependencySnapshots:async()=>{},
  }
  const d=useDashboard(), e=useModRepositoryRefresh()
  await d.load(); const pending=e.refreshEverything()
  await tick(); assert.deepEqual(calls,['toml:root'])
  root.resolve(); await tick(); await tick()
  assert.equal(d.config.value.repositories[0].id,'ordinary')
  assert.equal(e.sourceNodes.value.find(n=>n.id==='child').status,'loading')
  assert.deepEqual(e.sourceNodes.value.find(n=>n.id==='child').note_i18n,{en:'child source note'})
  assert.equal(e.sourceNodes.value.find(n=>n.id==='failed').status,'pending')
  assert.ok(!calls.some(Array.isArray))
  child.resolve(); await tick(); await tick()
  assert.ok(d.config.value.repositories.some(r=>r.id==='bound'))
  assert.ok(!calls.some(Array.isArray))
  failed.reject(Error('offline')); const errors=await pending
  assert.equal(errors.length,1)
  assert.equal(e.sourceNodes.value.find(n=>n.id==='failed').status,'error')
  assert.deepEqual(calls.filter(Array.isArray),[['api','bound',false],['api','ordinary',false]])
  retry=true;calls.length=0
  await e.refreshSource('failed')
  assert.deepEqual(calls,['toml:failed',['api','recovered',false]])
  assert.equal(d.config.value.repositories.length,3)
})

test('cached maps reconstruct source titles and notes before an incremental tree arrives', async () => {
  const e = useMapSources()
  const cached = [
    { ...repo('cached-a', 'cached-source'), source_name: 'Cached source', source_name_i18n: { en: 'English source' },
      source_note_i18n: { en: 'Source instructions' }, note_i18n: { en: 'Map instructions' } },
    { ...repo('cached-b', 'cached-source'), source_note_i18n: { en: 'Source instructions' } },
    { ...repo('cached-c', 'plain-source'), note_i18n: { en: 'Must not become a source note' } },
  ]
  globalThis.__loadingApi = { maps: async () => cached }
  await e.loadMaps()
  assert.deepEqual(e.sourceNodes.value.map(node => node.id), ['cached-source', 'plain-source'])
  assert.equal(e.sourceNodes.value[0].label, 'Cached source')
  assert.deepEqual(e.sourceNodes.value[0].label_i18n, { en: 'English source' })
  assert.deepEqual(e.sourceNodes.value[0].note_i18n, { en: 'Source instructions' })
  assert.equal(e.sourceNodes.value[1].note_i18n, undefined)
})

test('map engine publishes channel snapshots immediately, keeps successful siblings after failure and scopes retry to the requested branch', async () => {
  const source=deferred(), requests=[];let progress
  const e=useMapSources()
  const first={...repo('root-map','root'),level_set_name:'Root',author:'A'}
  const nested={...repo('nested-map','child'),level_set_name:'Nested',author:'A'}
  const nodes=[{...node('root',null,'ready'),note_i18n:{en:'Root source'}},{...node('child','root','loading'),note_i18n:{ja:'子ソース'}},node('failed','root')]
  globalThis.__loadingApi={
    refreshMapSources:async(pass,force,onProgress,id)=>{requests.push(['toml',force,id]);progress=onProgress;await source.promise;return[first,nested]},
    refreshMapReleases:async(id,force)=>{requests.push(['api',id,force]);return{...[first,nested].find(map=>map.id===id),builtin:false,latest_release:null,latest_tag:null,cached_release_count:0,cached_at:123,last_error:null}},
    syncDependencySnapshots:async()=>{},maps:async()=>e.maps.value,
  }
  const pending=e.refreshAll();await tick()
  progress({nodes,maps:[first]});await tick()
  assert.deepEqual(e.maps.value.map(m=>m.id),['root-map'])
  assert.equal(e.sourceNodes.value[1].status,'loading')
  assert.deepEqual(e.sourceNodes.value[1].note_i18n,{ja:'子ソース'})
  assert.equal(requests.length,1)
  nodes[1].status='ready';nodes[2].status='error';nodes[2].error='offline'
  progress({nodes,maps:[first,nested]});source.resolve();await pending
  assert.equal(e.maps.value.length,2,'one failed source cannot restore or erase successful cards')
  globalThis.__loadingApi.refreshMapSources=async(pass,force,onProgress,id)=>{
    requests.push(['retry',force,id]);
    assert.deepEqual(e.maps.value.map(m=>m.id),['root-map'],'branch cards are removed before HTTP')
    onProgress({nodes:nodes.map(n=>n.id==='child'?{...n,status:'ready'}:n),maps:[first,nested]})
    return[first,nested]
  }
  requests.length=0;await e.refreshSource({id:'child'})
  assert.deepEqual(requests,[['retry',false,'child'],['api','nested-map',false]])
})

test('MOD source config read failure keeps the trusted config and attaches the source error', async () => {
  const config={repositories:[repo('before','snapshot-failure')],repository_sources:[{id:'snapshot-failure',nickname:'snapshot-failure',url:'child.toml'}],region:'international',platform:'windows'}
  let fail=false
  globalThis.__loadingApi={
    dashboard:async()=>({config:structuredClone(config),game:{kitchen_running:false}}),
    localMods:async()=>({generation:2,mods:[]}),modUpdates:async()=>[],
    config:async()=>{if(fail)throw Error('snapshot read failed');return structuredClone(config)},
    importRepositorySource:async()=>{fail=true;return {access_error:null}},
  }
  const d=useDashboard(),e=useModRepositoryRefresh()
  await d.load()
  const errors=await e.refreshSourceChain('snapshot-failure')
  assert.ok(errors.some(text=>text.includes('snapshot read failed')))
  const source=e.sourceNodes.value.find(n=>n.id==='snapshot-failure')
  assert.equal(source.status,'error')
  assert.match(source.error,/snapshot read failed/)
  assert.equal(d.config.value.repositories[0].id,'before')
})

test('MOD full refresh failure retains exact platform verdicts and never clears backend source snapshots', async () => {
  const trusted = { ...repo('promoted', 'trusted-source'), cached_releases: [],
    release_prerelease_metadata: { 'v1.0.0-beta': false }, releases_cached_at: 123 }
  const manual = repo('manual-without-source')
  const config = { repositories: [trusted, manual], repository_sources: [
    { id: 'trusted-source', nickname: 'Trusted', url: 'trusted.toml' },
  ], region: 'international', platform: 'windows' }
  const requests = []
  let clears = 0, scans = 0
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => { scans++; return { generation: 10, mods: [] } },
    modUpdates: async () => [], config: async () => structuredClone(config),
    clearRepositorySourceCache: async () => { clears++; config.repositories = []; return structuredClone(config) },
    importRepositorySource: async () => {
      assert.deepEqual(useDashboard().config.value.repositories, [trusted, manual])
      throw Error('source offline')
    },
    refreshModRepository: async (id, force) => {
      requests.push([id, force]); throw Error('API offline')
    },
    syncDependencySnapshots: async () => {},
  }
  const d = useDashboard(), e = useModRepositoryRefresh()
  await d.load()
  const previousScans = scans
  const errors = await e.refreshEverything({ force: true })
  assert.ok(errors.some(error => error.includes('source offline')))
  assert.ok(!errors.some(error => error.includes('API offline')), 'a failed main file cannot start this pass with stale Release targets')
  assert.equal(clears, 0)
  assert.equal(scans, previousScans, 'source refresh must not rescan DLLs')
  assert.deepEqual(requests, [])
  assert.deepEqual(d.config.value.repositories[0].release_prerelease_metadata, trusted.release_prerelease_metadata)
  assert.equal(d.config.value.repositories[0].releases_cached_at, 123)
  assert.equal(e.sourceNodes.value.find(node => node.id === 'trusted-source').status, 'error')
})

test('MOD source refresh never requests or declares a child whose ancestor excludes the current region', async () => {
  const sources = [
    { id: 'restricted-root', nickname: 'Mainland root', url: 'root.toml', region: 'cn' },
    { id: 'restricted-child', parent_id: 'restricted-root', nickname: 'Child', url: 'child.toml' },
    { id: 'available', nickname: 'Global source', url: 'global.toml', region: 'global', note_i18n: { en: 'Global note' } },
  ]
  const config = { repositories: [], repository_sources: sources, region: 'global', platform: 'windows' }
  const requests = []
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 3, mods: [] }), modUpdates: async () => [],
    config: async () => structuredClone(config),
    importRepositorySource: async id => { requests.push(id); return { access_error: null } },
  }
  const d = useDashboard(), e = useModRepositoryRefresh()
  await d.load()
  assert.deepEqual(await e.refreshSourceChain('restricted-child'), [])
  assert.deepEqual(await e.refreshSourceChain('available'), [])
  assert.deepEqual(requests, ['available'])
  assert.deepEqual(e.sourceNodes.value.map(node => node.id), ['available'])
  assert.deepEqual(e.sourceNodes.value[0].note_i18n, { en: 'Global note' })
})

test('map branch progress only publishes its scope, including newly declared descendants', async () => {
  const e = useMapSources(), gate = deferred(), requests = []
  const first = { ...repo('scope-a', 'scope'), level_set_name: 'A', latest_tag: '1' }
  const other = { ...repo('scope-b', 'other-scope'), level_set_name: 'B', latest_tag: '1' }
  const nodes = [node('scope', null, 'ready'), node('other-scope', null, 'ready')]
  globalThis.__loadingApi = {
    refreshMapSources: async (_pass, _force, progress) => { progress({ nodes, maps: [first, other] }); return [first, other] },
    refreshMapReleases: async id => [first, other].find(map => map.id === id),
    syncDependencySnapshots: async () => {}, config: async () => useDashboard().config.value,
  }
  await e.refreshAll()
  const untouched = e.maps.value.find(map => map.id === other.id)
  const untouchedNode = e.sourceNodes.value.find(node => node.id === 'other-scope')
  const added = { ...repo('scope-child', 'new-scope-child'), level_set_name: 'Child' }
  const updated = { ...first, latest_tag: '2' }
  const staleSibling = { ...other, latest_tag: '999' }
  globalThis.__loadingApi.refreshMapSources = async (_pass, _force, progress, sourceId) => {
    assert.equal(sourceId, 'scope')
    progress({ nodes: [...nodes, node('new-scope-child', 'scope', 'ready')], maps: [updated, staleSibling, added] })
    await gate.promise
    return [updated, staleSibling, added]
  }
  globalThis.__loadingApi.refreshMapReleases = async id => { requests.push(id); return [updated, added].find(map => map.id === id) }
  const pending = e.refreshSource({ id: 'scope' })
  await tick()
  const evidence = {
    globalBusy: e.preparingSources.value,
    untouched: e.maps.value.find(map => map.id === other.id),
    untouchedNode: e.sourceNodes.value.find(node => node.id === 'other-scope'),
  }
  gate.resolve(); await pending
  assert.equal(evidence.globalBusy, false, 'unrelated source buttons must not inherit source preparation')
  assert.equal(evidence.untouched, untouched, 'channel snapshots cannot publish another source even when API cache changed')
  assert.equal(evidence.untouchedNode, untouchedNode, 'unrelated declaration/status references stay stable')
  assert.equal(e.maps.value.find(map => map.id === other.id), untouched, 'final snapshot must obey the same projection')
  assert.deepEqual(requests.sort(), ['scope-a', 'scope-child'], 'a new descendant joins only this branch Release queue')
})

test('MOD group Release refresh does not publish unrelated repositories from a final config reread', async () => {
  const a = repo('group-a', 'same-source'), b = { ...repo('group-b', 'same-source'), name: 'Unselected' }
  let config = { repositories: [a, b], repository_sources: [{ id: 'same-source', nickname: 'Source', url: 'source.toml' }], region: 'global', platform: 'windows' }
  const requests = [], d = useDashboard(), e = useModRepositoryRefresh()
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 30, mods: [] }), modUpdates: async () => [],
    config: async () => structuredClone(config),
    refreshModRepository: async id => {
      requests.push(id)
      config = { ...config, repositories: [{ ...a, name: 'Refreshed A' }, { ...b, name: 'Backend B changed' }] }
      return { cached_releases: [{ tag_name: '2', assets: [] }], release_prerelease_metadata: {} }
    },
  }
  await d.load()
  const untouched = d.config.value.repositories[1]
  assert.deepEqual(await e.refreshMany([d.config.value.repositories[0]]), [])
  assert.deepEqual(requests, ['group-a'])
  assert.equal(d.config.value.repositories[1], untouched, 'group refresh cannot replace an unrelated row at finalize')
  assert.equal(d.config.value.repositories[1].name, 'Unselected')
  assert.equal(d.config.value.repositories[0].cached_releases[0].tag_name, '2')
})

test('MOD local source reload applies new children without replacing unrelated source data or marking it pending', async () => {
  const source = (id, parent_id) => ({ id, parent_id, nickname: id, url: id + '.toml' })
  let config = { repositories: [repo('local-a', 'local-source'), { ...repo('local-b', 'unselected-source'), name: 'Trusted B' }],
    repository_sources: [source('local-source'), source('unselected-source')], region: 'global', platform: 'windows' }
  const d = useDashboard(), e = useModRepositoryRefresh(), requests = []
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 31, mods: [] }), modUpdates: async () => [],
    config: async () => structuredClone(config),
    importRepositorySource: async id => {
      requests.push('source:' + id)
      if (id === 'local-source') config = { ...config,
        repository_sources: [source('local-source'), source('unselected-source'), source('local-child', 'local-source')],
        repositories: [repo('local-a', 'local-source'), { ...repo('local-b', 'unselected-source'), name: 'Backend B changed' }, repo('child-map', 'local-child')],
      }
      return { access_error: null }
    },
    refreshModRepository: async id => { requests.push(id); return { cached_releases: [], release_prerelease_metadata: {} } },
  }
  await d.load()
  const untouched = d.config.value.repositories[1], untouchedSource = d.config.value.repository_sources[1]
  assert.deepEqual(await e.refreshSource('local-source'), [])
  assert.equal(d.config.value.repositories.find(repo => repo.id === 'local-b'), untouched)
  assert.equal(d.config.value.repository_sources.find(source => source.id === 'unselected-source'), untouchedSource)
  assert.equal(e.sourceNodes.value.find(node => node.id === 'unselected-source').status, 'ready', 'an untouched declared source is never scheduled as pending')
  assert.deepEqual(requests, ['source:local-source', 'source:local-child', 'local-a', 'child-map'])
})

test('MOD full pass rebuilds only the online projection after main success, hiding stale grandchildren until their parent commits', async () => {
  const root = deferred(), child = deferred(), leaf = deferred(), release = deferred(), calls = []
  const source = (id, parent_id) => ({ id, parent_id, nickname: id, url: id + '.toml' })
  const oldChild = repo('projection-old-child', 'projection-child'), oldGrand = repo('projection-old-grand', 'projection-old-grand-source')
  const main = repo('projection-main', 'projection-root'), nextChild = repo('projection-new-child', 'projection-child')
  let config = { repositories: [oldChild, oldGrand], repository_sources: [source('projection-root'), source('projection-child', 'projection-root'), source('projection-old-grand-source', 'projection-child')], region: 'global', platform: 'windows' }
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 32, mods: [] }), modUpdates: async () => [], config: async () => structuredClone(config),
    importRepositorySource: async id => {
      calls.push('source:' + id)
      if (id === 'projection-root') { await root.promise; config = { ...config, repositories: [main, oldChild, oldGrand] } }
      if (id === 'projection-child') { await child.promise; config = { ...config, repositories: [main, nextChild], repository_sources: [source('projection-root'), source('projection-child', 'projection-root'), source('projection-leaf', 'projection-child')] } }
      if (id === 'projection-leaf') await leaf.promise
      return { access_error: null }
    },
    refreshModRepository: async id => { calls.push('api:' + id); await release.promise; return { cached_releases: [], release_prerelease_metadata: {} } },
    syncDependencySnapshots: async () => {},
  }
  const d = useDashboard(), e = useModRepositoryRefresh()
  await d.load()
  const before = e.visibleRepositories.value
  const pending = e.refreshEverything()
  await tick()
  assert.equal(e.repositoriesLoading.value, true, 'full spinner starts with the main document')
  assert.deepEqual(e.visibleRepositories.value, before, 'pending main cannot clear trusted old online cards')
  root.resolve(); await tick(); await tick()
  assert.deepEqual(e.visibleRepositories.value.map(row => row.id), [main.id])
  assert.deepEqual(e.visibleSources.value.map(row => row.id), ['projection-root', 'projection-child'])
  assert.ok(!e.sourceNodes.value.some(row => row.id === 'projection-old-grand-source'), 'old deep headings cannot appear before their parent document succeeds')
  assert.ok(d.config.value.repositories.some(row => row.id === oldChild.id), 'online clearing must preserve global trusted bindings')
  assert.ok(d.config.value.repositories.some(row => row.id === oldGrand.id))
  assert.ok(!calls.some(call => call.startsWith('api:')))
  child.resolve(); await tick(); await tick()
  assert.deepEqual(e.visibleRepositories.value.map(row => row.id), [main.id, nextChild.id])
  assert.deepEqual(e.visibleSources.value.map(row => row.id), ['projection-root', 'projection-child', 'projection-leaf'])
  assert.ok(!calls.some(call => call.startsWith('api:')))
  leaf.reject(Error('leaf offline')); await tick(); await tick()
  assert.equal(e.sourceNodes.value.find(row => row.id === 'projection-leaf').status, 'error')
  assert.equal(e.repositoriesLoading.value, true, 'full spinner covers the final Release queue too')
  assert.deepEqual(calls.filter(call => call.startsWith('api:')).sort(), ['api:' + main.id, 'api:' + nextChild.id].sort())
  release.resolve(); const errors = await pending
  assert.equal(errors.length, 1)
  assert.equal(e.repositoriesLoading.value, false)
  assert.equal(e.anyRefreshing.value, false)
})

test('map main failure preserves the old tree/cards and starts zero ordinary Release calls or unrelated MOD publication', async () => {
  const e = useMapSources(), d = useDashboard(), gate = deferred(), calls = []
  const old = { ...repo('failure-map', 'failure-map-child'), level_set_name: 'Old' }
  const nodes = [node('failure-map-root', null, 'ready'), node('failure-map-child', 'failure-map-root', 'ready')]
  let progress
  const trustedConfig = { repositories: [repo('do-not-publish', 'other-mod-source')], repository_sources: [{ id: 'other-mod-source', nickname: 'Other', url: 'other.toml' }], region: 'global', platform: 'windows', data_provider: 'github' }
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(trustedConfig), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 33, mods: [] }), modUpdates: async () => [],
    config: async () => ({ ...structuredClone(trustedConfig), data_provider: 'cloudflare', repositories: [{ ...trustedConfig.repositories[0], name: 'Unexpected changed MOD' }] }),
    maps: async () => [old], syncDependencySnapshots: async () => {},
    refreshMapSources: async (_pass, _force, callback) => { callback({ nodes, maps: [old] }); return [old] },
    refreshMapReleases: async id => { calls.push(id); return old },
  }
  await d.load(); await e.refreshAll()
  const trustedMod = d.config.value.repositories[0], oldMap = e.maps.value[0], oldChild = e.sourceNodes.value[1]
  calls.length = 0
  globalThis.__loadingApi.refreshMapSources = async (_pass, _force, callback) => {
    progress = callback
    callback({ nodes: [node('failure-map-root', null, 'loading')], maps: [] })
    await gate.promise
    callback({ nodes: [node('failure-map-root', null, 'error', 'main offline')], maps: [] })
    throw Error('main offline')
  }
  const pending = e.refreshAll(); await tick()
  assert.equal(e.refreshingAll.value, true)
  assert.equal(e.maps.value[0], oldMap)
  assert.equal(e.sourceNodes.value.find(row => row.id === oldChild.id), oldChild)
  gate.resolve(); await pending
  assert.deepEqual(calls, [], 'preserved old cards are not this failed pass\'s Release targets')
  assert.equal(e.maps.value[0], oldMap)
  assert.equal(e.sourceNodes.value.find(row => row.id === oldChild.id), oldChild)
  assert.equal(d.config.value.repositories[0], trustedMod, 'provider reread cannot replace unrelated MOD objects')
  assert.equal(d.config.value.data_provider, 'cloudflare')
  assert.equal(e.anyRefreshing.value, false)
  progress({ nodes, maps: [{ ...old, latest_tag: 'stale-after-failure' }] })
  assert.equal(e.maps.value[0], oldMap, 'a closed source pass rejects late Channel callbacks')
})

test('map full pass switches at root ready, fills successful children and rejects late source snapshots after Release publication', async () => {
  const e = useMapSources(), treeGate = deferred(), releaseGate = deferred(), calls = []
  const main = { ...repo('timed-main', 'timed-root'), level_set_name: 'Main' }
  const nested = { ...repo('timed-child-map', 'timed-child'), level_set_name: 'Child' }
  const fresh = { ...main, latest_tag: 'new-release' }
  let progress
  globalThis.__loadingApi = {
    config: async () => useDashboard().config.value,
    refreshMapSources: async (_pass, _force, callback) => { progress = callback; callback({ nodes: [node('timed-root', null, 'loading')], maps: [] }); await treeGate.promise; return [main, nested] },
    refreshMapReleases: async id => { calls.push(id); await releaseGate.promise; return id === main.id ? fresh : nested },
    syncDependencySnapshots: async () => {},
  }
  const before = e.maps.value, pending = e.refreshAll(); await tick()
  assert.equal(e.maps.value, before)
  const nodes = [node('timed-root', null, 'ready'), node('timed-child', 'timed-root', 'loading'), node('timed-failure', 'timed-root', 'pending')]
  progress({ nodes, maps: [main] }); await tick()
  assert.deepEqual(e.maps.value.map(row => row.id), [main.id])
  assert.deepEqual(e.sourceNodes.value.map(row => row.id), nodes.map(row => row.id))
  assert.deepEqual(calls, [])
  nodes[1] = node('timed-child', 'timed-root', 'ready'); nodes[2] = node('timed-failure', 'timed-root', 'error', 'child offline')
  progress({ nodes, maps: [main, nested] }); await tick()
  assert.deepEqual(e.maps.value.map(row => row.id), [main.id, nested.id])
  assert.deepEqual(calls, [])
  treeGate.resolve(); await tick(); await tick()
  assert.deepEqual(calls.sort(), [main.id, nested.id].sort())
  assert.equal(e.refreshingAll.value, true)
  releaseGate.resolve(); await pending
  assert.equal(e.maps.value.find(row => row.id === main.id).latest_tag, 'new-release')
  progress({ nodes, maps: [main] })
  assert.deepEqual(e.maps.value.map(row => row.id), [main.id, nested.id])
  assert.equal(e.maps.value.find(row => row.id === main.id).latest_tag, 'new-release', 'late source snapshot cannot roll back a fresher Release result')
  assert.equal(e.refreshingAll.value, false)
})

test('map provider reread cannot supersede a pending MOD source projection and lose newly declared children', async () => {
  const firstRead = deferred(), secondRead = deferred(), requests = []
  const source = (id, parent_id) => ({ id, parent_id, nickname: id, url: id + '.toml' })
  const initial = { repositories: [repo('cross-old', 'cross-root')], repository_sources: [source('cross-root')], region: 'global', platform: 'windows', data_provider: 'github' }
  const next = { ...initial, data_provider: 'cloudflare', repositories: [repo('cross-fresh', 'cross-root'), repo('cross-child-repo', 'cross-child')], repository_sources: [source('cross-root'), source('cross-child', 'cross-root')] }
  let reads = 0
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(initial), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 34, mods: [] }), modUpdates: async () => [],
    config: async () => { reads++; if (reads === 1) return firstRead.promise; if (reads === 2) return secondRead.promise; return structuredClone(next) },
    importRepositorySource: async id => { requests.push('source:' + id); return { access_error: null } },
    refreshMapSources: async () => [], syncDependencySnapshots: async () => {},
    refreshModRepository: async id => { requests.push('api:' + id); return { cached_releases: [], release_prerelease_metadata: {} } },
  }
  const d = useDashboard(), e = useModRepositoryRefresh()
  await d.load()
  const mod = e.refreshSource('cross-root'); await tick()
  assert.equal(reads, 1)
  const map = useMapSources().refreshAll(); await tick()
  firstRead.resolve(structuredClone(next)); await tick(); await tick()
  secondRead.resolve(structuredClone(next)); await Promise.all([mod, map])
  assert.ok(requests.includes('source:cross-child'), 'the source must apply its own successful config read before descending')
  assert.deepEqual(requests.filter(id => id.startsWith('api:')).sort(), ['api:cross-child-repo', 'api:cross-fresh'])
  assert.deepEqual(d.config.value.repository_sources.map(row => row.id), ['cross-root', 'cross-child'])
  assert.equal(d.config.value.data_provider, 'cloudflare')
})

test('a failed MOD root keeps its child status when another root succeeds, and a successful root still refreshes manual repositories', async () => {
  const otherRoot = deferred()
  const source = (id, parent_id) => ({ id, parent_id, nickname: id, url: id + '.toml' })
  const config = { repositories: [repo('multi-a-card', 'multi-a-child'), repo('multi-b-card', 'multi-b'), repo('multi-manual')],
    repository_sources: [source('multi-a'), source('multi-a-child', 'multi-a'), source('multi-b')], region: 'global', platform: 'windows' }
  let fail = false
  const requests = []
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 35, mods: [] }), modUpdates: async () => [], config: async () => structuredClone(config),
    importRepositorySource: async id => { requests.push('source:' + id); if (fail && id === 'multi-a') throw Error('root A offline'); if (fail && id === 'multi-b') await otherRoot.promise; return { access_error: null } },
    refreshModRepository: async id => { requests.push('api:' + id); return { cached_releases: [], release_prerelease_metadata: {} } },
    syncDependencySnapshots: async () => {},
  }
  const d = useDashboard(), e = useModRepositoryRefresh()
  await d.load(); await e.refreshSourceChain('multi-a'); await e.refreshSourceChain('multi-b')
  const oldChild = e.sourceNodes.value.find(row => row.id === 'multi-a-child')
  assert.equal(oldChild.status, 'ready')
  fail = true; requests.length = 0
  const pending = e.refreshEverything(); await tick()
  const failedBranchChild = e.sourceNodes.value.find(row => row.id === oldChild.id)
  assert.deepEqual(failedBranchChild, oldChild)
  otherRoot.resolve(); const errors = await pending
  assert.equal(errors.length, 1)
  assert.equal(e.sourceNodes.value.find(row => row.id === oldChild.id), failedBranchChild, 'another root cannot redeclare an unscheduled stale child as pending')
  assert.equal(e.anyRefreshing.value, false)
  assert.deepEqual(requests, ['source:multi-a', 'source:multi-b', 'api:multi-b-card', 'api:multi-manual'])
})

test('a successful MOD parent resets stale child errors to pending while preserving unrelated source status', async () => {
  const gate = deferred(), nextChild = deferred()
  const source = (id, parent_id) => ({ id, parent_id, nickname: id, url: id + '.toml' })
  const config = { repositories: [], repository_sources: [source('reset-parent'), source('reset-child', 'reset-parent'), source('reset-sibling', 'reset-parent')], region: 'global', platform: 'windows' }
  let round = 0
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 36, mods: [] }), modUpdates: async () => [], config: async () => structuredClone(config),
    importRepositorySource: async id => {
      if (!round && id !== 'reset-parent') throw Error('old child error')
      if (round && id === 'reset-parent') await gate.promise
      if (round && id === 'reset-child') await nextChild.promise
      return { access_error: null }
    }, syncDependencySnapshots: async () => {},
  }
  const d = useDashboard(), e = useModRepositoryRefresh()
  await d.load(); await e.refreshSourceChain('reset-parent')
  assert.equal(e.sourceNodes.value.find(row => row.id === 'reset-sibling').status, 'error')
  round = 1
  const pending = e.refreshEverything(); await tick()
  gate.resolve(); await tick(); await tick()
  assert.equal(e.sourceNodes.value.find(row => row.id === 'reset-child').status, 'loading')
  const sibling = e.sourceNodes.value.find(row => row.id === 'reset-sibling')
  assert.equal(sibling.status, 'pending')
  assert.equal(sibling.error, undefined, 'a new parent declaration cannot carry a previous pass\'s child failure')
  nextChild.resolve(); await pending
  assert.equal(e.anyRefreshing.value, false)
})


// Keep the production recursive refresh and projection; observe only the IPC boundary.
async function sourceForceFixture(prefix, includeOther = false) {
  const ids = Object.fromEntries(['root', 'child', 'leaf', 'other'].map(key => [key, `${prefix}-${key}`]))
  const source = (key, parent) => ({ id: ids[key], parent_id: parent && ids[parent], nickname: key, url: `${ids[key]}.toml`, enabled: true })
  const config = {
    repositories: [repo(ids.root + '-repo', ids.root), repo(ids.child + '-repo', ids.child), repo(ids.leaf + '-repo', ids.leaf)],
    repository_sources: [source('root'), source('child', 'root'), source('leaf', 'child')],
    region: 'global', platform: 'windows',
  }
  if (includeOther) {
    config.repositories.push(repo(ids.other + '-repo', ids.other))
    config.repository_sources.push(source('other'))
  }
  const imports = [], releases = [], failed = new Set()
  const fixture = { ids, config, imports, releases, failed, onImport: () => {} }
  globalThis.__loadingApi = {
    dashboard: async () => ({ config: structuredClone(config), game: { kitchen_running: false } }),
    localMods: async () => ({ generation: 37, mods: [] }), modUpdates: async () => [],
    config: async () => structuredClone(config),
    importRepositorySource: async (...args) => {
      imports.push(args)
      if (failed.has(args[0])) throw Error('source offline')
      fixture.onImport(args[0])
      return { access_error: null }
    },
    refreshModRepository: async (...args) => {
      releases.push(args)
      return { cached_releases: [], release_prerelease_metadata: {} }
    },
    syncDependencySnapshots: async () => {},
  }
  fixture.dashboard = useDashboard()
  fixture.engine = useModRepositoryRefresh()
  await fixture.dashboard.load()
  return fixture
}

for (const force of [false, true]) test(`MOD ${force ? 'user-forced' : 'startup'} full refresh forces the package only at the root`, async () => {
  const f = await sourceForceFixture(force ? 'force-user' : 'force-startup')
  const errors = await (force ? f.engine.refreshEverything({ force: true }) : f.engine.refreshEverything())
  assert.deepEqual(errors, [])
  assert.deepEqual(f.imports, [
    [f.ids.root, `${f.ids.root}.toml`, false, force],
    [f.ids.child, `${f.ids.child}.toml`, false, false],
    [f.ids.leaf, `${f.ids.leaf}.toml`, false, false],
  ], 'every document has an explicit fourth argument; descendants always reuse the root package')
  assert.deepEqual(f.releases.map(([id]) => id).sort(), f.config.repositories.map(repo => repo.id).sort())
  assert.ok(f.releases.every(([, requestedForce]) => requestedForce === force))
  assert.equal(f.engine.anyRefreshing.value, false)
})

test('MOD child source refresh reuses its package and isolates unrelated backend config changes', async () => {
  const f = await sourceForceFixture('force-partial', true)
  await f.engine.refreshSourceChain(f.ids.other)
  const untouchedRepo = f.dashboard.config.value.repositories.find(repo => repo.source_id === f.ids.other)
  const untouchedSource = f.dashboard.config.value.repository_sources.find(source => source.id === f.ids.other)
  const untouchedRoot = f.dashboard.config.value.repositories.find(repo => repo.source_id === f.ids.root)
  f.imports.length = 0
  f.onImport = () => {
    f.config.repositories = f.config.repositories.map(repo => [f.ids.root, f.ids.other].includes(repo.source_id) ? { ...repo, name: 'unrelated backend change' } : repo)
    f.config.repository_sources = f.config.repository_sources.map(source => source.id === f.ids.other ? { ...source, nickname: 'unrelated backend source' } : source)
  }
  assert.deepEqual(await f.engine.refreshSource(f.ids.child), [])
  assert.deepEqual(f.imports, [
    [f.ids.child, `${f.ids.child}.toml`, false, false],
    [f.ids.leaf, `${f.ids.leaf}.toml`, false, false],
  ], 'a child group must neither force nor request the official root')
  assert.deepEqual(f.releases.map(([id]) => id).sort(), [f.ids.child + '-repo', f.ids.leaf + '-repo'].sort())
  assert.ok(f.releases.every(([, requestedForce]) => requestedForce === false))
  assert.equal(f.dashboard.config.value.repositories.find(repo => repo.source_id === f.ids.other), untouchedRepo)
  assert.equal(f.dashboard.config.value.repository_sources.find(source => source.id === f.ids.other), untouchedSource)
  assert.equal(f.dashboard.config.value.repositories.find(repo => repo.source_id === f.ids.root), untouchedRoot)
})

test('MOD failed child package read stops descendants and Release checks without disturbing another source', async () => {
  const f = await sourceForceFixture('force-failure', true)
  await f.engine.refreshSourceChain(f.ids.other)
  const untouchedRepo = f.dashboard.config.value.repositories.find(repo => repo.source_id === f.ids.other)
  const untouchedSource = f.dashboard.config.value.repository_sources.find(source => source.id === f.ids.other)
  f.imports.length = 0
  f.failed.add(f.ids.child)
  const errors = await f.engine.refreshSource(f.ids.child)
  assert.equal(errors.length, 1)
  assert.match(errors[0], /source offline/)
  assert.deepEqual(f.imports, [[f.ids.child, `${f.ids.child}.toml`, false, false]])
  assert.deepEqual(f.releases, [], 'a failed scope cannot check stale repositories or unrelated groups')
  assert.equal(f.dashboard.config.value.repositories.find(repo => repo.source_id === f.ids.other), untouchedRepo)
  assert.equal(f.dashboard.config.value.repository_sources.find(source => source.id === f.ids.other), untouchedSource)
  assert.equal(f.engine.sourceNodes.value.find(node => node.id === f.ids.other).status, 'ready')
  assert.equal(f.engine.sourceNodes.value.find(node => node.id === f.ids.child).status, 'error')
  assert.equal(f.engine.anyRefreshing.value, false)
})
