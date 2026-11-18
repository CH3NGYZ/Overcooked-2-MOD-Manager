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
test('chain preparation feeds the shared race overlay from isolated IPC and a revised plan still needs confirmation',async()=>{
 const prepared=deferred(),calls=[];let emit
 globalThis.__changeStoreApi=async(name,args)=>{calls.push([name,...args]);if(name==='planModChanges')return plan();if(name==='prepareModChanges'){emit=args[1];return prepared.promise}if(name==='executeModChanges')return{ok:true,message:''};return snapshot('done')}
 const store=useModChanges(),pending=store.run({roots:[{node_id:'mod:root',enabled:true}]});await tick()
 assert.equal(store.state.value.phase,'confirming');assert.equal(downloadTaskState.running,null)
 const confirm=store.confirm();assert.equal(store.state.value.phase,'preparing');assert.equal(downloadTaskState.running.progressSource,'caller')
 const progress={plan_id:'initial',stage:'downloading',actions:[],active_node_id:'mod:root',items:[{node_id:'mod:root',candidate_id:'v1',label:'Root',stage:'downloading',download:{label:'asset',downloaded:6,total:10,speed_bytes:50,finished:false}}]}
 emit({...progress,plan_id:'unrelated'});assert.equal(store.preparation.value,null)
 emit(progress);assert.equal(store.preparation.value.items[0].download.downloaded,6)
 assert.equal(downloadTaskState.progress.downloaded,6)
 updateDownloadProgress({label:'global unrelated',downloaded:2,total:10,speed_bytes:50,finished:false});assert.equal(store.preparation.value.items[0].download.downloaded,6);assert.equal(downloadTaskState.progress.downloaded,6)
 emit({...progress,stage:'verifying',items:[{...progress.items[0],stage:'verifying'}]});assert.equal(downloadTaskState.progress,null)
 emit({...progress,items:[{...progress.items[0],download:{...progress.items[0].download,downloaded:8}}]});assert.equal(downloadTaskState.progress.downloaded,8)
 prepared.resolve(plan('revised',true));await confirm
 assert.equal(store.state.value.phase,'reconfirming');assert.equal(downloadTaskState.running,null);assert.equal(downloadTaskState.progress,null)
 emit({...progress,items:[]});assert.equal(store.preparation.value.items.length,1,'closed IPC cannot replace retained progress')
 assert.equal(calls.some(call=>call[0]==='executeModChanges'),false)
 await store.confirm();await pending;assert.equal(store.state.value,null);assert.equal(store.preparation.value,null);assert.equal(downloadTaskState.failure,null)
})
test('preparation failure retains item context and never shows a second download failure card',async()=>{
 let emit;globalThis.__changeStoreApi=async(name,args)=>{if(name==='planModChanges')return plan();if(name==='prepareModChanges'){emit=args[1];emit({plan_id:'initial',stage:'failed',actions:[],items:[{node_id:'mod:root',candidate_id:'v1',label:'Root',stage:'failed'}]});throw new Error('download.cancelled')}}
 const store=useModChanges(),pending=store.run({roots:[{node_id:'mod:root',enabled:true}]});await tick();await store.confirm()
 assert.equal(store.state.value.phase,'failed');assert.equal(downloadTaskState.running,null);assert.equal(downloadTaskState.progress,null);assert.equal(downloadTaskState.failure,null)
 assert.equal(store.preparation.value.items[0].stage,'failed')
 await store.cancel();assert.equal(await pending,null);assert.equal(store.preparation.value,null)
 emit({plan_id:'initial',stage:'ready',actions:[],items:[]});assert.equal(store.preparation.value,null)
})

test('preparation cancellation hides progress only after the downloader accepts it',async()=>{
 const prepared=deferred(),calls=[];let emit,cancels=0
 globalThis.__changeStoreApi=async(name,args)=>{calls.push(name);if(name==='planModChanges')return plan();if(name==='prepareModChanges'){emit=args[1];return prepared.promise}if(name==='cancelDownload'&&++cancels===1)throw new Error('update.inProgress')}
 const store=useModChanges(),pending=store.run({roots:[{node_id:'mod:root',enabled:true}]});await tick()
 const confirm=store.confirm();assert.equal(store.canCancelDownload.value,false)
 const item={node_id:'mod:root',candidate_id:'v1',label:'Root',stage:'downloading',download:{label:'asset',downloaded:6,total:10,speed_bytes:50,finished:false}}
 const downloading={plan_id:'initial',stage:'downloading',actions:[],active_node_id:item.node_id,items:[item]}
 emit(downloading);assert.equal(store.canCancelDownload.value,true)
 await store.cancelPreparationDownload()
 assert.equal(store.cancellingDownload.value,false);assert.equal(store.canCancelDownload.value,true);assert.equal(downloadTaskState.progress.downloaded,6)
 await store.cancelPreparationDownload();await store.cancelPreparationDownload();await store.cancel()
 assert.equal(cancels,2)
 emit(downloading);assert.equal(downloadTaskState.progress,null,'late private bytes cannot reopen a cancelled task')
 assert.equal(calls.includes('cancelModChanges'),false);assert.equal(store.state.value.phase,'preparing')
 emit({plan_id:'initial',stage:'finalizing',actions:[],items:[]});assert.equal(store.canCancelDownload.value,false)
 prepared.resolve(plan('revised',true));await confirm
 assert.equal(store.cancellingDownload.value,false)
 await store.cancelPreparationDownload();assert.equal(cancels,2)
 await store.cancel();await pending
})
