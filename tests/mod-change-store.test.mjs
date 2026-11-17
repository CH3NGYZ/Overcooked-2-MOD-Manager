import assert from 'node:assert/strict'
import test from 'node:test'
import {registerHooks} from 'node:module'
registerHooks({resolve(specifier,context,next){if(specifier.startsWith('.')&&!/\.[cm]?[jt]s$/.test(specifier)){try{return next(`${specifier}.ts`,context)}catch{}}return next(specifier,context)},
 load(url,context,next){if(url.endsWith('/src/services/tauri.ts'))return {format:'module',shortCircuit:true,source:'export const api=new Proxy({}, {get:(_,name)=>(...args)=>globalThis.__changeStoreApi(name,args)})'};return next(url,context)}})
const {useModChanges,registerEvaluationSubject}=await import('../src/modChanges/store.ts')
const tick=()=>new Promise(resolve=>setImmediate(resolve)),deferred=()=>{let resolve;const promise=new Promise(yes=>resolve=yes);return{promise,resolve}}
const snapshot=revision=>({revision,nodes:[],evaluations:[]})
test('newer evaluation refresh wins even when an older IPC finishes last',async()=>{
 const first=deferred(),second=deferred();let i=0;globalThis.__changeStoreApi=()=>++i===1?first.promise:second.promise
 const store=useModChanges(),a=store.refresh(),b=store.refresh();second.resolve(snapshot('new'));await b;first.resolve(snapshot('old'));await a
 assert.equal(store.evaluation.value.revision,'new');assert.equal(store.evaluating.value,false)
})
test('source invalidation clears existing state and rejects an older in-flight result',async()=>{
 const pending=deferred();globalThis.__changeStoreApi=()=>pending.promise;const store=useModChanges(),work=store.refresh();store.clearEvaluation();pending.resolve(snapshot('stale'));await work;assert.equal(store.evaluation.value,null)
})
test('visible exact subjects are batched, deduplicated and released on cleanup',async()=>{
 const calls=[];globalThis.__changeStoreApi=async(name,args)=>{calls.push([name,...args]);return snapshot('batch')}
 const subject={repository_id:'owner',tag:'v1'},a=registerEvaluationSubject(subject),b=registerEvaluationSubject({...subject});await tick()
 assert.equal(calls.length,1);assert.deepEqual(calls[0],['evaluateModChanges',[subject]])
 a();b();await useModChanges().refresh();assert.deepEqual(calls.at(-1),['evaluateModChanges',undefined])
})
test('ordinary same-tick invalidations share one request',async()=>{
 let calls=0;globalThis.__changeStoreApi=async()=>{calls++;return snapshot('one')};const store=useModChanges();await Promise.all([store.invalidate(),store.invalidate(),store.invalidate()]);assert.equal(calls,1)
})
test('an invalidation during evaluation schedules a second snapshot before the shared promise settles',async()=>{
 const first=deferred();let calls=0;globalThis.__changeStoreApi=()=>++calls===1?first.promise:Promise.resolve(snapshot('latest'))
 const store=useModChanges(),pending=store.invalidate();await tick();const again=store.invalidate();assert.strictEqual(again,pending)
 first.resolve(snapshot('older'));await pending;assert.equal(calls,2);assert.equal(store.evaluation.value.revision,'latest')
})
