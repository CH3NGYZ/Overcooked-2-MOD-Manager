import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
registerHooks({resolve(specifier,context,next){if(specifier==='./store')return next('./store.ts',context);return next(specifier,context)},
  load(url,context,next){if(url.endsWith('/src/modChanges/store.ts'))return {format:'module',shortCircuit:true,source:'export const useModChanges=()=>globalThis.__changeRequestStore'};return next(url,context)}})
const {requestToggleChanges,requestAllToggleChanges}=await import('../src/modChanges/requests.ts')
const node=(node_id,extra={})=>({node_id,kind:'mod',enabled:true,dll_files:['a.dll'],candidates:[],...extra})
function fixture(nodes){const calls=[];globalThis.__changeRequestStore={evaluation:{value:null},refresh:async()=>{calls.push('refresh');globalThis.__changeRequestStore.evaluation.value={nodes}},run:async request=>{calls.push(request);return {ok:true,message:''}}};return calls}
test('single member toggles use fresh backend inventory and stable member keys',async()=>{
 const calls=fixture([node('package:actual',{dll_files:['a.dll','b.dll'],members:[{key:'stable-a',dll_file:'a.dll',enabled:true}]})])
 await requestToggleChanges({dllFiles:['a.dll'],enabled:false})
 assert.deepEqual(calls,['refresh',{roots:[{node_id:'package:actual',member_key:'stable-a',enabled:false}]}])
})
test('whole package toggles deduplicate DLL selections without member keys',async()=>{
 const calls=fixture([node('package:actual',{dll_files:['a.dll','b.dll']})])
 await requestToggleChanges({dllFiles:['a.dll','b.dll'],wholeNodes:true,enabled:true})
 assert.deepEqual(calls[1],{roots:[{node_id:'package:actual',enabled:true}]})
})
for(const nodes of [[],[node('one'),node('two')],[node('one',{dll_files:['a.dll','b.dll']})]])test(`missing, ambiguous or missing-member inventory cannot create a plan: ${nodes.length}`,async()=>{
 const calls=fixture(nodes);await assert.rejects(()=>requestToggleChanges({dllFiles:['a.dll'],enabled:false}),/modChanges.targetChanged/);assert.deepEqual(calls,['refresh'])
})
test('map toggle resolves backend directory identity in the same batch',async()=>{
 const calls=fixture([node('map:actual',{kind:'map',dll_files:[],map_dir_name:'Map-A'})])
 await requestToggleChanges({mapDirectories:['Map-A'],enabled:false});assert.deepEqual(calls[1],{roots:[{node_id:'map:actual',enabled:false}]})
})
for(const kind of ['mod','map'])test(`bulk ${kind} includes all physical installed nodes and excludes display/remote subjects`,async()=>{
 const physical=node('physical',{kind,dll_files:kind==='mod'?['a.dll']:[],map_dir_name:kind==='map'?'Map-A':undefined})
 const calls=fixture([physical,node('other',{kind:kind==='mod'?'map':'mod'}),node('remote',{kind,dll_files:[]}),{...physical,node_id:'subject:version',subject_tag:'v2'}])
 await requestAllToggleChanges(kind,false);assert.deepEqual(calls[1],{roots:[{node_id:'physical',enabled:false}]})
})
test('empty physical inventory makes no bulk request',async()=>{const calls=fixture([]);assert.equal(await requestAllToggleChanges('mod',true),null);assert.deepEqual(calls,['refresh'])})
