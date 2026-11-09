import assert from 'node:assert/strict'
import test from 'node:test'
import vm from 'node:vm'
import { readFileSync } from 'node:fs'
import ts from 'typescript'
import * as vue from 'vue'
const code = ts.transpileModule(readFileSync('src/composables/useMapPublish.ts', 'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText
const deferred = () => { let resolve; const promise = new Promise(r => {resolve=r}); return {resolve,promise} }
function load(api) {
  const exports = {}, notices = [], unmount = []
  const deps = {
    vue: {...vue,onUnmounted: fn=>unmount.push(fn)},
    '../services/tauri':{api:{cancelPublishAuthorization:async()=>{},...api}},
    '../i18n':{t:key=>key}, '../mapPublishFlow':{parsePublishPermissionFailure:()=>null},
    '../locale/backend':{backendMessageOf:String,translateBackendMessage:String},
    '../useNotify':{useNotify:()=>({error:x=>notices.push(x),warning:x=>notices.push(x),success:()=>{}})},
    '../mapPublishRelease':{upsertPublishRelease:(list,value)=>[...list.filter(x=>x.id!==value.id),value]},
  }
  vm.runInNewContext(code,{exports,require:name=>{assert.ok(name in deps,name);return deps[name]},setTimeout,clearTimeout})
  return {state:exports.useMapPublish(),notices,unmount}
}
const auth = {authenticated:true,login:'fixture',installed:false,secret_configured:true,client_id:'fixture',install_url:''}
const repo = {owner:'u',name:'maps',full_name:'u/maps',push:true}
const check = {outcome:{status:'valid'},auth}
test('切换发布平台丢弃旧平台迟到的会话与仓库结果，GitCode不走App安装门',async()=>{
  const stale=deferred(), calls=[]
  const {state}=load({checkPublishSession:async p=>{calls.push(p);return p==='github'?stale.promise:check},listPublishRepositories:async()=>({installations:0,all_repositories:false,repositories:[repo]}),listPublishReleases:async()=>[]})
  const old=state.loadState()
  await state.selectProvider('gitcode')
  stale.resolve({outcome:{status:'signed_out'},auth:{...auth,authenticated:false,login:'stale'}})
  await old
  assert.equal(state.provider.value,'gitcode');assert.equal(state.auth.value.login,'fixture');assert.equal(state.needsInstall.value,false)
  assert.equal(state.selectedRepository.value.full_name,'u/maps')
  assert.deepEqual(calls,['github','gitcode'])
})
test('旧仓库列表失败或成功均不能污染切换后的平台',async()=>{
  const stale=deferred()
  const {state,notices}=load({checkPublishSession:async()=>check,listPublishRepositories:async p=>p==='github'?stale.promise:{installations:0,all_repositories:false,repositories:[]}})
  const old=state.loadState();await new Promise(r=>setImmediate(r))
  await state.selectProvider('gitcode');stale.resolve({installations:2,all_repositories:true,repositories:[repo]});await old
  assert.equal(state.repositories.value.length,0);assert.equal(state.installationCount.value,0);assert.equal(notices.length,0)
})
test('待清理附件固定使用上传时的平台、tag与仓库，写操作期间不能切渠道',async()=>{
  const calls=[]
  const {state}=load({checkPublishSession:async()=>({outcome:{status:'signed_out'},auth:{...auth,authenticated:false}}),deletePublishAsset:async(...args)=>{calls.push(args);return {ok:true,message:'ok'}}})
  state.uploading.value=true;await state.selectProvider('gitcode');assert.equal(state.provider.value,'github');state.uploading.value=false
  const result=await state.deleteAsset('opaque-id','tag/地图',{provider:'gitcode',owner:'original',repository:'repo'})
  assert.equal(result,true);assert.deepEqual(calls,[['original','repo','opaque-id','tag/地图','gitcode']])
})
test('退出登录后迟到的仓库响应不能恢复旧列表',async()=>{
  const stale=deferred()
  const {state}=load({checkPublishSession:async()=>check,listPublishRepositories:async()=>stale.promise,signOutMapPublish:async()=>({...auth,authenticated:false,login:''})})
  const loading=state.loadState();await new Promise(r=>setImmediate(r))
  await state.signOut();stale.resolve({installations:1,all_repositories:true,repositories:[repo]});await loading
  assert.equal(state.authenticated.value,false);assert.equal(state.repositories.value.length,0);assert.equal(state.repositoriesLoading.value,false)
})
