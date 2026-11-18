import assert from 'node:assert/strict'
import test from 'node:test'
import {registerHooks} from 'node:module'
registerHooks({resolve(specifier,context,next){if(specifier.startsWith('.')&&!/\.[cm]?[jt]s$/.test(specifier)){try{return next(`${specifier}.ts`,context)}catch{}}return next(specifier,context)},
 load(url,context,next){if(url.endsWith('/src/services/tauri.ts'))return {format:'module',shortCircuit:true,source:'export const api=new Proxy({}, {get:(_,name)=>(...args)=>globalThis.__changeStoreApi(name,args)})'};return next(url,context)}})
const {useModChanges,registerEvaluationSubject}=await import('../src/modChanges/store.ts')
const {downloadTaskState,updateDownloadProgress}=await import('../src/downloadTask.ts')
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

const plan=(id='initial',prepared=false)=>({plan_id:id,prepared,status:'ready',nodes:[],roots:[],actions:[],evaluations:[],blockers:[],impacts:[]})
test('chain preparation owns inline download only until validation; revised plan still needs confirmation',async()=>{
 const prepared=deferred(),calls=[]
 globalThis.__changeStoreApi=async(name,args)=>{calls.push([name,...args]);if(name==='planModChanges')return plan();if(name==='prepareModChanges')return prepared.promise;if(name==='executeModChanges')return{ok:true,message:''};return snapshot('done')}
 const store=useModChanges(),pending=store.run({roots:[{node_id:'mod:root',enabled:true}]});await tick()
 assert.equal(store.state.value.phase,'confirming');assert.equal(downloadTaskState.running,null)
 const confirm=store.confirm();assert.equal(store.state.value.phase,'preparing');assert.equal(downloadTaskState.running.presentation,'mod-changes')
 updateDownloadProgress({label:'actual asset',downloaded:6,total:10,speed_bytes:50,finished:false});assert.equal(downloadTaskState.progress.downloaded,6)
 prepared.resolve(plan('revised',true));await confirm
 assert.equal(store.state.value.phase,'reconfirming');assert.equal(downloadTaskState.running,null);assert.equal(downloadTaskState.progress,null)
 updateDownloadProgress({label:'late asset',downloaded:9,finished:false});assert.equal(downloadTaskState.progress,null)
 assert.equal(calls.some(call=>call[0]==='executeModChanges'),false)
 await store.confirm();await pending;assert.equal(store.state.value,null);assert.equal(downloadTaskState.failure,null)
})
test('preparation failure remains workflow-owned with no second download failure card',async()=>{
 globalThis.__changeStoreApi=async(name)=>{if(name==='planModChanges')return plan();if(name==='prepareModChanges')throw new Error('download.cancelled')}
 const store=useModChanges(),pending=store.run({roots:[{node_id:'mod:root',enabled:true}]});await tick();await store.confirm()
 assert.equal(store.state.value.phase,'failed');assert.equal(downloadTaskState.running,null);assert.equal(downloadTaskState.progress,null);assert.equal(downloadTaskState.failure,null)
 await store.cancel();assert.equal(await pending,null)
})
