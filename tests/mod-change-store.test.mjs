import assert from 'node:assert/strict'
import test from 'node:test'
import {registerHooks} from 'node:module'
registerHooks({resolve(specifier,context,next){if(specifier.startsWith('.')&&!/\.[cm]?[jt]s$/.test(specifier)){try{return next(`${specifier}.ts`,context)}catch{}}return next(specifier,context)},
 load(url,context,next){if(url.endsWith('/src/services/tauri.ts'))return {format:'module',shortCircuit:true,source:'export const api=new Proxy({}, {get:(_,name)=>(...args)=>globalThis.__changeStoreApi(name,args)})'};return next(url,context)}})
const {useModChanges,registerEvaluationSubject}=await import('../src/modChanges/store.ts')
const {changeStatusKey}=await import('../src/modChanges/presentation.ts')
const {downloadTaskState,updateDownloadProgress}=await import('../src/downloadTask.ts')
const {backendMessageOf}=await import('../src/locale/backend.ts')
const {i18n}=await import('../src/locale/index.ts')
const tick=()=>new Promise(resolve=>setImmediate(resolve)),deferred=()=>{let resolve;const promise=new Promise(yes=>resolve=yes);return{promise,resolve}}
const snapshot=revision=>({revision,nodes:[],evaluations:[]})
test('newer evaluation refresh wins even when an older IPC finishes last',async()=>{
 const first=deferred(),second=deferred();let i=0;globalThis.__changeStoreApi=()=>++i===1?first.promise:second.promise
 const store=useModChanges(),a=store.refresh(),b=store.refresh();second.resolve(snapshot('new'));await b;first.resolve(snapshot('old'))
 assert.equal((await a).revision,'old','request callers receive their own inventory, even if no longer displayed')
 assert.equal(store.evaluation.value.revision,'new');assert.equal(store.evaluating.value,false)
})
test('source invalidation clears existing state and rejects an older in-flight result',async()=>{
 const pending=deferred();globalThis.__changeStoreApi=()=>pending.promise;const store=useModChanges(),work=store.refresh();store.clearEvaluation();pending.resolve(snapshot('stale'));await work;assert.equal(store.evaluation.value,null)
})
test('coordinated reads retain the last evaluation and include new detail subjects when both scans settle',async t=>{
 const store=useModChanges(),overtaken=deferred(),calls=[]
 globalThis.__changeStoreApi=async(_,args)=>{calls.push(args);return calls.length===2?overtaken.promise:snapshot(calls.length===1?'visible':'settled')}
 await store.refresh()
 const work=store.refresh()
 store.setEvaluationPaused(true);t.after(()=>store.setEvaluationPaused(false))
 const subject={map_id:'map',tag:'v2'},unregister=registerEvaluationSubject(subject);t.after(unregister)
 await tick();assert.equal(calls.length,2);assert.equal(store.evaluating.value,false)
 overtaken.resolve(snapshot('overtaken'));await work;assert.equal(store.evaluation.value.revision,'visible')
 store.setEvaluationPaused(false);await store.invalidate()
 assert.deepEqual(calls,[ [undefined], [undefined], [[subject]] ])
 assert.equal(store.evaluation.value.revision,'settled')
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
test('a map scan publishing just after the evaluation loop exits cannot lose its refresh',async()=>{
 const first=deferred();let calls=0;globalThis.__changeStoreApi=()=>++calls===1?first.promise:Promise.resolve(snapshot('map-scanned'))
 const store=useModChanges(),pending=store.invalidate();await tick()
 const mapPublished=first.promise.then(async()=>{await Promise.resolve();store.clearEvaluation();await store.invalidate()})
 first.resolve(snapshot('before-map-scan'));await Promise.all([pending,mapPublished])
 assert.equal(calls,2);assert.equal(store.evaluation.value.revision,'map-scanned')
})

const plan=(id='initial',prepared=false,install_notes=[])=>({plan_id:id,prepared,status:'ready',nodes:[],roots:[],actions:[],evaluations:[],blockers:[],impacts:[],install_notes})
const note={id:'github-beta',name:'Arcade Beta',name_i18n:{},note:'Read the beta instructions.',note_i18n:{}}

test('pending plan and its first-install note both block preparation until explicit approval',async()=>{
 const planned=deferred(),calls=[]
 globalThis.__changeStoreApi=async(name,args)=>{calls.push([name,...args]);if(name==='planModChanges')return planned.promise}
 const store=useModChanges(),request={roots:[{node_id:'mod:root',enabled:true}]},pending=store.run(request)
 await store.confirm();assert.deepEqual(calls,[['planModChanges',request]])
 planned.resolve(plan('initial',false,[note]));await tick()
 assert.deepEqual(store.installNotes.value,[{id:note.id,name:note.name,text:note.note}])
 store.acceptInstallNotes([{id:note.id,name:note.name,text:'Stale text'}]);await store.confirm()
 assert.deepEqual(calls,[['planModChanges',request]])
 store.acceptInstallNotes(store.installNotes.value);assert.deepEqual(store.installNotes.value,[])
 assert.deepEqual(calls,[['planModChanges',request]],'continuing opens review; it never prepares or applies')
 await store.cancel();assert.equal(await pending,null);assert.deepEqual(calls.at(-1),['cancelModChanges','initial'])
 const again=store.run(request);await tick();assert.equal(store.installNotes.value.length,1,'cancelled plan does not mark future installs as read')
 await store.cancel();assert.equal(await again,null)
})

for(const revised of [note,{...note,note:'Instructions changed.'}])test(`reconfirmation ${revised===note?'skips read':'gates revised'} notes`,async()=>{
 const calls=[]
 globalThis.__changeStoreApi=async(name)=>{
   calls.push(name)
   if(name==='planModChanges')return plan('initial',false,[note])
   if(name==='prepareModChanges')return plan('verified',true,[revised])
   if(name==='executeModChanges')return {ok:true,message:''}
   return snapshot('done')
 }
 const store=useModChanges(),pending=store.run({roots:[{node_id:'mod:root',enabled:true}]});await tick()
 store.acceptInstallNotes(store.installNotes.value);await store.confirm()
 assert.equal(store.state.value.phase,'reconfirming');assert.deepEqual(calls,['planModChanges','prepareModChanges'])
 if(revised!==note){
   assert.equal(store.installNotes.value[0].text,revised.note)
   await store.confirm();assert.deepEqual(calls,['planModChanges','prepareModChanges'])
   store.acceptInstallNotes(store.installNotes.value)
 }else assert.deepEqual(store.installNotes.value,[])
 await store.confirm();assert.equal((await pending).ok,true)
 assert.equal(calls.filter(name=>name==='executeModChanges').length,1)
})

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
 emit({...progress,items:[]});assert.equal(store.preparation.value,null,'closed IPC cannot reopen preparation during consent to apply')
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

test('a map ZIP rejected as game.running during preparation never applies and can be replanned',async t=>{
 const store=useModChanges(),calls=[]
 const request={roots:[],map_import:{zip_path:'map.zip',selected:['Map-A']}}
 const mapPlan={...plan(),actions:[{node_id:'map:root',kind:'install'}]}
 const locale=i18n.global.locale
 const previousLocale=locale.value
 t.after(()=>{locale.value=previousLocale})
 locale.value='zh-CN'
 globalThis.__changeStoreApi=async(name,args)=>{
   calls.push([name,args[0]])
   if(name==='planModChanges')return mapPlan
   if(name==='prepareModChanges')throw new Error('game.running')
   if(name==='cancelModChanges')return
   assert.fail(`Unexpected IPC: ${name}`)
 }
 const revision=store.appliedRevision.value
 const pending=store.run(request);await tick()
 assert.equal(store.state.value.phase,'confirming')
 await store.confirm()
 assert.equal(store.state.value.phase,'failed')
 assert.equal(backendMessageOf(store.state.value.error),'《胡闹厨房2》正在运行，请先关闭后重试')
 await store.confirm()
 assert.deepEqual(calls,[['planModChanges',request],['prepareModChanges','initial']])
 assert.equal(store.appliedRevision.value,revision)
 assert.equal(downloadTaskState.failure,null)
 await store.retry()
 assert.equal(store.state.value.phase,'confirming')
 assert.equal(store.state.value.error,null)
 assert.deepEqual(calls.slice(2),[['cancelModChanges','initial'],['planModChanges',request]])
 await store.cancel();assert.equal(await pending,null)
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

test('apply progress ignores foreign and closed Channels and clears provisional success after rollback',async()=>{
 const first=deferred(),second=deferred(),channels=[]
 globalThis.__changeStoreApi=async(name,args)=>{
   if(name==='planModChanges')return plan('same',true)
   if(name==='executeModChanges'){
     channels.push(args[1])
     return (channels.length===1?first:second).promise
   }
 }
 const store=useModChanges(),request={roots:[{node_id:'mod:root',enabled:true}]}
 const root={node_id:'mod:root',member_key:'root.dll'},sibling={...root,member_key:'sibling.dll'}
 const rootKey=changeStatusKey(root.node_id,root.member_key),siblingKey=changeStatusKey(sibling.node_id,sibling.member_key)
 const progress=(item,stage,plan_id='same')=>({plan_id,items:[{...item,stage}]})

 const pending=store.run(request);await tick()
 const confirm=store.confirm()
 assert.equal(store.state.value.phase,'executing')
 channels[0](progress(root,'processed','foreign'));assert.equal(store.execution.size,0)
 channels[0](progress(root,'applying'));assert.equal(store.execution.get(rootKey),'applying')
 channels[0](progress(root,'processed'));assert.equal(store.execution.get(rootKey),'processed')
 assert.equal(store.execution.has(siblingKey),false)
 // The command can report rollback before its final Channel event is delivered.
 first.resolve({ok:false,message:'recycle.partialFailure'});await confirm
 assert.equal(store.state.value.phase,'failed');assert.equal(store.execution.get(rootKey),'not_applied')
 channels[0](progress(root,'processed'));assert.equal(store.execution.get(rootKey),'not_applied')
 await store.cancel();assert.equal(await pending,null);assert.equal(store.execution.size,0)

 const retry=store.run(request);await tick()
 const confirmRetry=store.confirm()
 assert.equal(store.state.value.phase,'executing')
 channels[1](progress(root,'applying'));assert.equal(store.execution.get(rootKey),'applying')
 channels[0](progress(root,'processed'));channels[0](progress(sibling,'applying'))
 assert.equal(store.execution.get(rootKey),'applying','a stale Channel with the same plan ID cannot advance the new run')
 assert.equal(store.execution.has(siblingKey),false)
 channels[1](progress(root,'applied'));assert.equal(store.execution.get(rootKey),'applied')
 second.resolve({ok:true,message:''});await confirmRetry
 assert.equal((await retry).ok,true);assert.equal(store.execution.size,0)
})
