import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'

const source = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const info = { version: '9.9.9', current_version: '2.0.2', manifest_url: 'https://example.com/latest.json', installer_signature: 'fixture', installer_name: 'fixture.exe' }
let instance = 0
const fresh = () => import(`../src/appUpdateState.ts?test=${++instance}`)

test('known update is sticky; only required can start; failures require manual release download', async () => {
  const s = await fresh()
  assert.equal(s.appUpdateState.stage, 'idle')
  assert.equal(s.beginAppUpdate(), false)
  s.requireAppUpdate(info)
  assert.equal(s.appUpdateState.stage, 'required')
  assert.equal(s.beginAppUpdate(), true)
  assert.equal(s.beginAppUpdate(), false)
  s.requireAppUpdate({ ...info, version: '10.0.0' })
  assert.equal(s.appUpdateState.update.version, '9.9.9')
  s.finishAppUpdate(false, 'update.installFailedManual')
  assert.equal(s.appUpdateState.stage, 'manual')
  assert.equal(s.appUpdateState.error, 'update.installFailedManual')
  s.requireAppUpdate(info)
  assert.equal(s.beginAppUpdate(), false)
  s.finishAppUpdate(true)
  assert.equal(s.appUpdateState.stage, 'manual', 'late success must not dismiss manual fallback')
})

test('successful handoff remains locked pending updater exit, not idle', async () => {
  const s = await fresh()
  s.requireAppUpdate(info)
  s.beginAppUpdate()
  s.finishAppUpdate(true)
  assert.equal(s.appUpdateState.stage, 'handoff')
  assert.equal(s.appUpdateState.error, '')
  assert.equal(s.beginAppUpdate(), false)
  s.finishAppUpdate(false, 'late error')
  assert.equal(s.appUpdateState.stage, 'handoff')
})

test('startup and settings use one App-level prompt; route and drop cannot dismiss it', () => {
  const app = source('src/App.vue')
  const settings = source('src/views/SettingsView.vue')
  assert.match(app, /requireAppUpdate\(result.update\)/)
  assert.match(settings, /requireAppUpdate\(updateResult.value.update\)/)
  assert.equal((app.match(/<UpdatePrompt\b/g) ?? []).length, 1)
  assert.doesNotMatch(settings, /<UpdatePrompt|showUpdate|updateInfo/)
  assert.match(app, /:inert="appUpdateState.stage !== 'idle'"/)
  assert.match(app, /TutorialOverlay v-if="appUpdateState.stage === 'idle'"/)
  assert.match(source('src/composables/useFileDrop.ts'), /appUpdateState.stage !== 'idle'[\s\S]*?return/)
})

test('prompt has no dismiss/retry path; all final failure forms lead to Release only', () => {
  const prompt = source('src/components/UpdatePrompt.vue')
  assert.match(prompt, /:closable="false"/)
  assert.match(prompt, /:mask-closable="false"/)
  assert.doesNotMatch(prompt, /update:show|failDownloadTask|@click="close"|onCloseRequested/)
  assert.match(prompt, /if \(!beginAppUpdate\(\)\) return/)
  assert.match(prompt, /finishAppUpdate\(result.ok, result.message\)/)
  assert.match(prompt, /catch \(reason\)[\s\S]*?finishAppUpdate\(false/)
  assert.match(prompt, /finally \{\s*endDownloadTask\(token\)/)
  assert.match(prompt, /api.openUpdateRepository\(\)/)
  assert.match(prompt, /if \(!manual.value \|\| openingRelease.value\) return/)
  assert.match(prompt, /v-if="manual"[\s\S]*?@click="openRelease"/)
  assert.match(prompt, /v-else[\s\S]*?:disabled="busy"/)
})

test('UI hides cancellation; native cancel, exit and close share the atomic update guard', () => {
  const strip = source('src/components/DownloadStrip.vue')
  assert.match(strip, /if \(!canCancel.value\) return/)
  assert.match(strip, /v-if="canCancel" class="dl-actions"/)
  const title = source('src/components/AppTitleBar.vue')
  assert.match(title, /:disabled="action === 'close' && updateBusy"/)
  const commands = source('src-tauri/src/commands.rs')
  assert.match(commands, /if !archive::cancel_download\(\)[\s\S]*?update.inProgress/)
  assert.match(commands, /install_app_update[\s\S]*?archive::begin_app_update\(\)\?/)
  for (const name of ['exit_app', 'relaunch_as_admin']) {
    assert.match(commands, new RegExp(`pub fn ${name}[^]*?archive::app_update_in_progress\\(\\)[^]*?update.inProgress`))
  }
  const lib = source('src-tauri/src/lib.rs')
  assert.match(lib, /app_update_in_progress\(\)[\s\S]*?CloseRequested[\s\S]*?api.prevent_close\(\)/)
  assert.match(lib, /ExitRequested[\s\S]*?code: None[\s\S]*?app_update_in_progress\(\)[\s\S]*?api.prevent_exit\(\)/)
  assert.match(source('src/composables/usePublishLeaveGuard.ts'), /appUpdateState.stage === 'installing' \|\| appUpdateState.stage === 'handoff'/)
})
