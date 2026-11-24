import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { effectScope } from 'vue'
import { createChangeWorkflow } from '../src/modChanges/workflow.ts'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch {}
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const modules = {
      '/src/services/tauri.ts': `export const watchLevelSetZipProgress=async()=>()=>{};
        export const api=new Proxy({}, {get:(_,key)=>(...args)=>globalThis.__chainEntry.api(key,args)})`,
      '/src/modChanges/store.ts': 'export const useModChanges=()=>({run:request=>globalThis.__chainEntry.run(request)})',
      '/src/composables/useDashboardStore.ts': 'export const useDashboard=()=>({game:{get value(){return {kitchen_running:globalThis.__chainEntry.running}}}})',
      '/src/composables/useMapSources.ts': 'export const useMapSources=()=>({maps:{value:globalThis.__chainEntry.maps}})',
      '/src/composables/useDropRouting.ts': `export const useDropRouting=()=>({acceptDrop:async (path, want)=>{
        const entry=globalThis.__chainEntry;entry.calls.push(['acceptDrop',path,want]);return entry.acceptLocal !== false
      }})`,
      '/src/useNotify.ts': 'export const useNotify=()=>new Proxy({}, {get:(_,kind)=>text=>globalThis.__chainEntry.notices.push([kind,text])})',
      '/src/i18n.ts': 'export const t=key=>key; export const pickI18n=(_,fallback)=>fallback',
      '/src/locale/backend.ts': 'export const backendMessageOf=String; export const translateBackendMessage=String',
    }
    for (const [suffix, source] of Object.entries(modules)) if (url.endsWith(suffix)) return {format:'module',shortCircuit:true,source}
    return next(url, context)
  },
})
const { useModInstallFlow } = await import('../src/composables/useModInstallFlow.ts')
const { useLevelSetImport } = await import('../src/composables/useLevelSetImport.ts')
const { useModRepositoryPreview } = await import('../src/composables/useModRepositoryPreview.ts')
const { gameOperationNotice, useGameOperationGate } = await import('../src/composables/useGameOperationGate.ts')
const tick = () => new Promise(resolve => setImmediate(resolve))
function fixture(extra={}) {
  const f={calls:[],plans:[],notices:[],maps:[],running:false,
    api(key,args){this.calls.push([key,...args]); if(key==='analyzeModSource')return Promise.resolve({preview_id:'preview-1'});
      if(key==='cancelModPreview'||key==='discardMapSource')return Promise.resolve({ok:true,message:''});
      if(key==='stageMapSource'||key==='stageMapRelease')return Promise.resolve(this.staged); throw new Error(`Unexpected API ${key}`)},
    async run(request){this.plans.push(request);return {ok:true,message:'modChanges.completed'}},...extra}
  globalThis.__chainEntry=f; return f
}

for(const entry of ['picker','drop','routed'])test(`manual ${entry} installs one preview root through the shared batch and releases staging`,async()=>{
  const f=fixture(),flow=useModInstallFlow()
  await flow.handleSourceSelect({kind:'local',value:'sample.zip'},entry)
  assert.deepEqual(f.plans,[{roots:[{node_id:'preview:preview-1',enabled:true}]}])
  assert.deepEqual(f.calls,[
    ...(entry==='routed'?[]:[['acceptDrop','sample.zip','mod']]),
    ['analyzeModSource','local','sample.zip'],['cancelModPreview','preview-1'],
  ])
  assert.deepEqual(f.notices,[['success','modChanges.completed']]);assert.equal(flow.sourceBusy.value,false)
})

for(const entry of ['picker','drop'])test(`a map with a bundled DLL from the MOD ${entry} is never staged as a MOD`,async()=>{
  const f=fixture({acceptLocal:false}),flow=useModInstallFlow()
  await flow.handleSourceSelect({kind:'local',value:'map-with-dll.zip'},entry)
  assert.deepEqual(f.calls,[['acceptDrop','map-with-dll.zip','mod']])
  assert.deepEqual(f.plans,[])
  assert.equal(flow.sourceBusy.value,false)
})

for(const result of [null,new Error('blocked')])test(`cancelled or failed batch never pre-uninstalls and cleans its preview: ${result}`,async()=>{
  const f=fixture({run:async function(request){this.plans.push(request);if(result instanceof Error)throw result;return result}})
  await useModInstallFlow().openPreview({preview_id:'preview-1',conflicts:[{handle:'old.dll'}]})
  assert.deepEqual(f.calls,[['cancelModPreview','preview-1']]);assert.equal(f.notices.some(([kind])=>kind==='success'),false)
})

test('running game blocks manual and online MOD installation and still cleans a submitted preview',async t=>{
  gameOperationNotice.value=false
  t.after(()=>{gameOperationNotice.value=false})
  const f=fixture({running:true}),flow=useModInstallFlow()
  flow.openSource()
  assert.equal(flow.sourceOpen.value,false)
  assert.equal(gameOperationNotice.value,true)
  await flow.handleSourceSelect({kind:'local',value:'sample.zip'})
  gameOperationNotice.value=false
  const {blockModOperation}=useGameOperationGate()
  await useModRepositoryPreview({gameBlocked:blockModOperation}).previewModRepository({
    id:'mod',cached_releases:[{tag_name:'v1.0',assets:[]}],
  })
  assert.equal(gameOperationNotice.value,true)
  assert.deepEqual(f.calls,[]);assert.deepEqual(f.plans,[])
  await flow.openPreview({preview_id:'preview-1'})
  assert.deepEqual(f.calls,[['cancelModPreview','preview-1']])
})

test('source re-entry is blocked until its whole batch finishes',async()=>{
  let finish;const f=fixture({run:request=>{f.plans.push(request);return new Promise(resolve=>{finish=resolve})}})
  const flow=useModInstallFlow(),pending=flow.handleSourceSelect({kind:'local',value:'first.zip'})
  await tick();await flow.handleSourceSelect({kind:'local',value:'second.zip'})
  assert.equal(f.plans.length,1);assert.equal(flow.sourceBusy.value,true)
  finish(null);await pending;assert.equal(flow.sourceBusy.value,false)
})

const mapItem={dir_name:'Map-A',name:'Map A',author:'Author',version:'1.0',level_count:1,plugins:[],conflicts:[{handle:'old-map'}]}
for(const kind of ['local','url'])test(`running game allows ${kind} maps to reach the shared installation plan`,async t=>{
  const staged=kind==='url'
  const value=staged?'https://example.com/map.zip':'map.zip'
  const download=staged?{download_id:'download-1'}:{}
  let finishPlan
  const f=fixture({running:true,staged:{path:'map.zip',staged,sets:[mapItem],origin:null,...download},
    run(request){this.plans.push(request);return new Promise(resolve=>{finishPlan=resolve})}})
  const scope=effectScope();const flow=scope.run(()=>useLevelSetImport());t.after(()=>scope.stop())
  const pending=flow.importSource({kind,value})
  await tick()
  assert.equal(flow.importing.value,true)
  assert.deepEqual(f.plans,[{roots:[],map_import:{zip_path:'map.zip',selected:['Map-A'],origin:null,...download}}])
  assert.deepEqual(f.calls,staged?[['stageMapSource',kind,value]]:[['acceptDrop',value,'map'],['stageMapSource',kind,value]])
  finishPlan({ok:true})
  await pending
  assert.equal(flow.importing.value,false)
  assert.deepEqual(f.notices,[['success','levelSet.imported']])
  assert.deepEqual(f.calls.filter(([key])=>key==='discardMapSource'),staged?[['discardMapSource','map.zip']]:[])
  assert.equal(f.calls.some(([key])=>['deleteLevelSet','uninstallLocalMod','importLevelSets'].includes(key)),false)
})

for(const [tag,asset] of [[undefined,undefined],['v1.0','chosen.zip']])test(`running game allows the online map ${tag?'selected asset':'card default'}`,async t=>{
  const targetTag=tag??'v2.0'
  const origin={map_id:'online',tag:targetTag}
  const f=fixture({running:true,maps:[{id:'online',name:'Map A',latest_tag:'v2.0',cached_releases:[{tag_name:'v2.0'},{tag_name:'v1.0'}]}],
    staged:{path:'cache-map.zip',staged:true,sets:[mapItem],origin,download_id:'release-download'}})
  const scope=effectScope();const flow=scope.run(()=>useLevelSetImport());t.after(()=>scope.stop())
  assert.equal(await flow.installRelease('online',tag,asset),true)
  assert.deepEqual(f.calls,[['stageMapRelease','online',targetTag,asset],['discardMapSource','cache-map.zip']])
  assert.deepEqual(f.plans,[{roots:[],map_import:{zip_path:'cache-map.zip',selected:['Map-A'],origin,download_id:'release-download'}}])
})

for(const staged of [false,true])test(`cancelling the shared map plan ${staged?'cleans the cached ZIP':'preserves the user ZIP'}`,async t=>{
  const kind=staged?'url':'local'
  const path=staged?'cache.zip':'user.zip'
  const value=staged?'https://example.com/map.zip':path
  const f=fixture({staged:{path,staged,sets:[mapItem],origin:null}})
  let state
  const actions=[{node_id:'map:root',kind:'install'}]
  const plan={plan_id:'map-plan',status:'ready',actions,blockers:[],install_notes:[],roots:[],impacts:[]}
  const workflow=createChangeWorkflow({
    async plan(request){f.plans.push(request);return plan},
    async cancel(planId){f.calls.push(['cancelModChanges',planId])},
    async prepare(planId){f.calls.push(['prepareModChanges',planId]);return {...plan,prepared:true}},
    async execute(){f.calls.push(['unexpectedExecute'])},
  },next=>{state=next})
  f.run=workflow.run
  const scope=effectScope(),flow=scope.run(()=>useLevelSetImport());t.after(()=>scope.stop())
  const pending=flow.importSource({kind,value},'routed')
  await tick()
  assert.equal(state?.phase,'reconfirming')
  assert.deepEqual(f.plans,[{roots:[],map_import:{zip_path:path,selected:['Map-A'],origin:null}}])
  assert.deepEqual(f.calls,[['stageMapSource',kind,value],['prepareModChanges','map-plan']])
  await workflow.cancel()
  await pending
  assert.equal(state,null)
  assert.equal(flow.importing.value,false)
  assert.deepEqual(f.notices,[])
  assert.deepEqual(f.calls,[['stageMapSource',kind,value],['prepareModChanges','map-plan'],['cancelModChanges','map-plan'],
    ...(staged?[['discardMapSource',path]]:[])])
})

for(const entry of ['picker','drop'])test(`a MOD package entering from the map ${entry} is never staged as a map`,async t=>{
  const f=fixture({acceptLocal:false})
  const scope=effectScope(),flow=scope.run(()=>useLevelSetImport());t.after(()=>scope.stop())
  await flow.importSource({kind:'local',value:'mixed.zip'},entry)
  assert.deepEqual(f.calls,[['acceptDrop','mixed.zip','map']])
  assert.deepEqual(f.plans,[])
  assert.equal(flow.importing.value,false)
  assert.equal(flow.sourceOpen.value,false)
})
