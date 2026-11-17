import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { effectScope, nextTick } from 'vue'

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
      '/src/composables/useMapSources.ts': 'export const useMapSources=()=>({maps:{value:[]}})',
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
const tick = () => new Promise(resolve => setImmediate(resolve))
function fixture(extra={}) {
  const f={calls:[],plans:[],notices:[],running:false,refreshes:0,
    api(key,args){this.calls.push([key,...args]); if(key==='analyzeModSource')return Promise.resolve({preview_id:'preview-1'});
      if(key==='cancelModPreview'||key==='discardMapSource')return Promise.resolve({ok:true,message:''});
      if(key==='stageMapSource')return Promise.resolve(this.staged); throw new Error(`Unexpected API ${key}`)},
    async run(request){this.plans.push(request);return {ok:true,message:'modChanges.completed'}},...extra}
  globalThis.__chainEntry=f; return f
}

test('manual source installs one preview root through the shared batch and then releases staging',async()=>{
  const f=fixture(),flow=useModInstallFlow(()=>{f.refreshes++})
  await flow.handleSourceSelect({kind:'file',value:'sample.zip'})
  assert.deepEqual(f.plans,[{roots:[{node_id:'preview:preview-1',enabled:true}]}])
  assert.deepEqual(f.calls,[['analyzeModSource','file','sample.zip'],['cancelModPreview','preview-1']])
  assert.equal(f.refreshes,1);assert.equal(flow.sourceBusy.value,false)
})

for(const result of [null,new Error('blocked')])test(`cancelled or failed batch never pre-uninstalls and cleans its preview: ${result}`,async()=>{
  const f=fixture({run:async function(request){this.plans.push(request);if(result instanceof Error)throw result;return result}})
  await useModInstallFlow(()=>{f.refreshes++}).openPreview({preview_id:'preview-1',conflicts:[{handle:'old.dll'}]})
  assert.deepEqual(f.calls,[['cancelModPreview','preview-1']]);assert.equal(f.refreshes,0)
})

test('running game prevents manual analysis and a received preview is still cleaned',async()=>{
  const f=fixture({running:true}),flow=useModInstallFlow()
  await flow.handleSourceSelect({kind:'file',value:'sample.zip'})
  assert.deepEqual(f.calls,[]);assert.deepEqual(f.plans,[])
  await flow.openPreview({preview_id:'preview-1'})
  assert.deepEqual(f.calls,[['cancelModPreview','preview-1']])
})

test('source re-entry is blocked until its whole batch finishes',async()=>{
  let finish;const f=fixture({run:request=>{f.plans.push(request);return new Promise(resolve=>{finish=resolve})}})
  const flow=useModInstallFlow(),pending=flow.handleSourceSelect({kind:'file',value:'first.zip'})
  await tick();await flow.handleSourceSelect({kind:'file',value:'second.zip'})
  assert.equal(f.plans.length,1);assert.equal(flow.sourceBusy.value,true)
  finish(null);await pending;assert.equal(flow.sourceBusy.value,false)
})

for(const staged of [false,true])test(`map detail hands the actual selected ZIP to one batch; staged=${staged}`,async t=>{
  const item={dir_name:'Map-A',name:'Map A',author:'Author',version:'1.0',level_count:1,plugins:[],conflicts:[{handle:'old-map'}]}
  const f=fixture({staged:{path:'map.zip',staged,sets:[item],origin:{map_id:'online'}}})
  const scope=effectScope();const flow=scope.run(()=>useLevelSetImport(()=>{f.refreshes++}));t.after(()=>scope.stop())
  const pending=flow.importSource({kind:'file',value:'map.zip'})
  await tick();assert.equal(flow.previewOpen.value,true);assert.deepEqual(f.plans,[])
  flow.confirmImport();await nextTick();await pending
  assert.deepEqual(f.plans,[{roots:[],map_import:{zip_path:'map.zip',selected:['Map-A'],origin:{map_id:'online'}}}])
  assert.equal(f.refreshes,1)
  assert.deepEqual(f.calls.filter(([key])=>key==='discardMapSource'),staged?[['discardMapSource','map.zip']]:[])
  assert.equal(f.calls.some(([key])=>['deleteLevelSet','uninstallLocalMod','importLevelSets'].includes(key)),false)
})

test('cancelling map detail makes no plan and preserves the user ZIP',async t=>{
  const f=fixture({staged:{path:'user.zip',staged:false,sets:[{dir_name:'Map-A',name:'Map A',plugins:[],conflicts:[]}]}})
  const scope=effectScope(),flow=scope.run(()=>useLevelSetImport());t.after(()=>scope.stop())
  const pending=flow.importSource({kind:'file',value:'user.zip'});await tick();flow.discardImport();await pending
  assert.deepEqual(f.plans,[]);assert.deepEqual(f.calls,[['stageMapSource','file','user.zip']])
})
