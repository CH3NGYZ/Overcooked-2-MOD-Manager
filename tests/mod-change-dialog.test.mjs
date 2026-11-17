import assert from 'node:assert/strict'
import test from 'node:test'
import {readFileSync,existsSync} from 'node:fs'
const read=path=>readFileSync(new URL(`../${path}`,import.meta.url),'utf8')
const dialog=read('src/modChanges/ChangePlanDialog.vue'),app=read('src/App.vue')
test('App owns the only shared plan modal and all pages use its store',()=>{
 assert.equal((app.match(/<ChangePlanDialog\s*\/>/g)||[]).length,1)
 for(const name of ['ModsLocalView','ModsOnlineView','LevelsView','MapsOnlineView'])assert.doesNotMatch(read(`src/views/${name}.vue`),/<(?:ChangePlanDialog|InstallCheckDialog|InstallPlanDialog|ModInstallNoteDialog)/)
 for(const path of ['components/InstallCheckDialog.vue','components/InstallPlanDialog.vue','components/ModInstallDialog.vue','components/ModInstallNoteDialog.vue','composables/useInstallPlanGate.ts'])assert.equal(existsSync(new URL(`../src/${path}`,import.meta.url)),false,path)
})
test('confirmation shows root tag, all actions, reasons, blockers and package member states',()=>{
 for(const pattern of [/root\.tag/,/in plan\.actions/,/action\.from_tag/,/action\.to_tag/,/action\.caused_by/,/in plan\.blockers/,/blocker\.owner_id/,/in node\.members/,/member\.enabled/,/member\.dll_file/])assert.match(dialog,pattern)
 assert.doesNotMatch(dialog,/api\.(?:install|toggle|uninstall)|execute\(action|done\.value|failures\.value/)
})
test('blocked, failed and busy plans cannot confirm; preparation and execution cannot cancel',()=>{
 assert.match(dialog,/state\?\.phase === 'failed'/);assert.match(dialog,/plan\?\.status !== 'ready'/)
 assert.match(dialog,/\['preparing', 'executing'\]\.includes/);assert.match(dialog,/:disabled="!canCancel"/)
 assert.match(dialog,/@click="changes\.confirm"/)
})
test('first install notes share the complete plan and reset reading time with note identity',()=>{
 assert.match(dialog,/changePlanInstallNotes/);assert.match(dialog,/notes\.value\.map\(note => `\$\{note\.id\}:\$\{note\.text\}`\)/)
 assert.match(dialog,/notes\.value\.length \* 5000/);assert.match(dialog,/readingSeconds > 0/);assert.match(dialog,/onUnmounted\(stopReadingTimer\)/)
 assert.doesNotMatch(read('src/modInstallNotes.ts'),/confirmModInstallNotes|modInstallNote =|setInterval/)
})
test('map selection and manual preview never pre-uninstall or execute individual rows',()=>{
 const map=read('src/composables/useLevelSetImport.ts'),manual=read('src/composables/useModInstallFlow.ts')
 assert.ok(map.indexOf('await askApproval(staged.path, sets)')<map.indexOf('await changes.run('))
 assert.match(map,/map_import:/);assert.match(manual,/node_id: `preview:/)
 for(const source of [map,manual])assert.doesNotMatch(source,/api\.(?:deleteLevelSet|uninstallLocalMod|confirmModInstall|importLevelSets)\(/)
})
test('online selection freezes the exact release or RAW candidate before shared confirmation',()=>{
 const source=read('src/composables/useModRepositoryPreview.ts')
 assert.match(source,/candidate_id: raw \? `raw:/);assert.match(source,/`release:\$\{repo\.id\}:\$\{release!\.tag_name\}`/)
 assert.match(source,/asset_name: assetName/);assert.doesNotMatch(source,/prepareInstall|dialog\.warning|api\.installModRepository/)
})
