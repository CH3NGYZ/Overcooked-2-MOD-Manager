import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { effectScope, nextTick } from 'vue'
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try {return next(`${specifier}.ts`,context)} catch { /* normal resolution */ }
    }
    return next(specifier,context)
  },
  load(url,context,next) {
    if (url.endsWith('/src/services/tauri.ts')) return {format:'module',shortCircuit:true,source:'export const api = new Proxy({}, {get:(_,key)=>(...args)=>globalThis.__previewApi[key](...args)})'}
    if (url.endsWith('/src/locale/backend.ts')) return {format:'module',shortCircuit:true,source:'export const backendMessageOf=String'}
    return next(url,context)
  },
})
const {useLevelPreview}=await import('../src/composables/useLevelPreview.ts')
const deferred=()=>{let resolve;const promise=new Promise(r=>{resolve=r});return {promise,resolve}}
const tick=async()=>{await nextTick();await new Promise(r=>setTimeout(r,0))}
const level=name=>({name,name_zh:'',scene_name:name,min_order_count:2,max_order_count:5,dependencies:[],scene_exists:true,recipe_count:1,active_recipe_count:1})
const picture=name=>({data_url:name,width:524,height:308,state:'ready'})
const set=name=>({dir_name:name,enabled:true})
function setup(t,api) {
  globalThis.__previewApi=api
  const scope=effectScope();const p=scope.run(useLevelPreview)
  t.after(()=>scope.stop());return p
}
test('catalog and screenshot publish before slow recipes; switching uses completed per-level results',async t=>{
  const recipes=deferred();let requests=0
  const p=setup(t,{levelSetPreview:async()=>[level('A'),level('B')],levelPreviewDetail:async(_,__,index,part)=>{
    requests++;return part==='picture'?{picture:picture(String(index)),recipes:null}:recipes.promise
  }})
  await p.open(set('set'));await tick()
  assert.equal(p.loading.value,false);assert.equal(p.picture.value.data_url,'0');assert.equal(p.recipeBusy.value,true)
  p.selected.value=1;await tick()
  assert.equal(p.picture.value.data_url,'1')
  recipes.resolve({picture:null,recipes:[]});await tick()
  assert.deepEqual(p.recipes.value,[])
  p.selected.value=0;await tick()
  assert.equal(p.picture.value.data_url,'0');assert.equal(requests,4)
})
test('late catalog from an older dialog session cannot replace a newly opened map',async t=>{
  const first=deferred()
  const p=setup(t,{levelSetPreview:async name=>name==='old'?first.promise:[level('new')],levelPreviewDetail:async()=>({picture:picture('new'),recipes:[]})})
  const old=p.open(set('old'));await p.open(set('new'));first.resolve([level('old')]);await old;await tick()
  assert.equal(p.current.value.name,'new');assert.equal(p.target.value.dir_name,'new')
})
test('closing keeps displayed content through transition and late requests cannot repaint it',async t=>{
  const delayed=deferred()
  const p=setup(t,{levelSetPreview:async()=>[level('A')],levelPreviewDetail:async()=>delayed.promise})
  await p.open(set('set'));await tick();p.show.value=false;await nextTick()
  assert.equal(p.current.value.name,'A');assert.equal(p.target.value.dir_name,'set')
  delayed.resolve({picture:picture('late'),recipes:[]});await tick()
  assert.equal(p.picture.value,null);p.afterLeave();assert.equal(p.target.value,null);assert.deepEqual(p.levels.value,[])
})
test('failed detail retains catalog and retry requests the failed detail again',async t=>{
  let fail=true
  const p=setup(t,{levelSetPreview:async()=>[level('A')],levelPreviewDetail:async()=>{if(fail)throw Error('broken');return {picture:picture('ok'),recipes:[]}}})
  await p.open(set('set'));await tick();assert.ok(p.pictureError.value);assert.equal(p.levels.value.length,1)
  fail=false;p.retry();await tick();assert.equal(p.picture.value.data_url,'ok');assert.equal(p.pictureError.value,'')
})
