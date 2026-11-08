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
      source: `export const api = new Proxy({}, {get: (_, key) => (...args) => globalThis.__loadingApi[key](...args)})
        export const watchGameState = async () => () => {}
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
const { useRepositoryRefresh } = await import('../src/composables/useRepositoryRefresh.ts')
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

test('MOD engine publishes root and pending child titles, waits for every TOML, hides discovery, then refreshes installed first', async () => {
  const root = deferred(), child = deferred(), failed = deferred()
  const source = (id,parent_id) => ({id,parent_id,nickname:id,nickname_i18n:{},url:id+'.toml',enabled:true})
  let config = {repositories:[],repository_sources:[source('root')],region:'international',platform:'windows'}
  const calls=[]
  let retry=false
  globalThis.__loadingApi = {
    dashboard:async()=>({config:structuredClone(config),game:{kitchen_running:false}}),
    localMods:async()=>[{dll_file:'bound.dll',repository_id:'bound',labels:[],enabled:true}],
    modUpdates:async()=>[],config:async()=>structuredClone(config),
    clearRepositorySourceCache:async()=>structuredClone(config),
    importRepositorySource:async(id)=>{
      calls.push('toml:'+id)
      if(id==='root') { await root.promise; config={...config,repositories:[repo('ordinary','root')],repository_sources:[source('root'),source('child','root'),source('failed','root')]} }
      if(id==='child') { await child.promise; config.repositories.push(repo('bound','child')) }
      if(id==='failed') { if(!retry) await failed.promise; else config.repositories.push(repo('recovered','failed')) }
      return {access_error:null}
    },
    refreshRepository:async(id,force)=>{calls.push(['api',id,force]);return[]},
    discoverCommunityRepositories:async()=>{throw Error('discovery must stay paused')},
    syncDependencySnapshots:async()=>{},
  }
  const d=useDashboard(), e=useRepositoryRefresh()
  await d.load(); const pending=e.refreshEverything()
  await tick(); assert.deepEqual(calls,['toml:root'])
  root.resolve(); await tick(); await tick()
  assert.equal(d.config.value.repositories[0].id,'ordinary')
  assert.equal(e.sourceNodes.value.find(n=>n.id==='child').status,'loading')
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

test('map engine publishes channel snapshots immediately, keeps successful siblings after failure and scopes retry to the requested branch', async () => {
  const source=deferred(), requests=[];let progress
  const e=useMapSources()
  const first={...repo('root-map','root'),level_set_name:'Root',author:'A'}
  const nested={...repo('nested-map','child'),level_set_name:'Nested',author:'A'}
  const nodes=[node('root',null,'ready'),node('child','root','loading'),node('failed','root')]
  globalThis.__loadingApi={
    refreshMapSources:async(pass,force,onProgress,id)=>{requests.push(['toml',force,id]);progress=onProgress;await source.promise;return[first,nested]},
    refreshMapReleases:async(id,force)=>{requests.push(['api',id,force]);return[]},
    syncDependencySnapshots:async()=>{},maps:async()=>e.maps.value,
  }
  const pending=e.refreshAll();await tick()
  progress({nodes,maps:[first]});await tick()
  assert.deepEqual(e.maps.value.map(m=>m.id),['root-map'])
  assert.equal(e.sourceNodes.value[1].status,'loading')
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
