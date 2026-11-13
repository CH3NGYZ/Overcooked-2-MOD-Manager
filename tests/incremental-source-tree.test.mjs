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
    refreshModRepository:async(id,force)=>{calls.push(['api',id,force]);return[]},
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
    refreshMapReleases:async(id,force)=>{requests.push(['api',id,force]);return[]},
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

test('MOD source config read failure is attached to its cleared group, never reported ready and blank', async () => {
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
