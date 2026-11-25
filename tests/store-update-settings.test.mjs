import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, ref, toRaw } from 'vue'

// Real SettingsView setup and shared update state; replace native/unrelated services only.
const boundary = globalThis.__storeUpdateSettings = {
  calls: [], notices: [], result: null, opening: async () => ({ ok: true }), demoActive: ref(true), dialogs: ref([]),
}
registerHooks({
  resolve(specifier, context, next) {
    if (['naive-ui', '@vicons/ionicons5', 'vue-router', '@tauri-apps/api/app'].includes(specifier)) {
      return { url: `store-update-test:${specifier}`, shortCircuit: true }
    }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'store-update-test:naive-ui') return fake(`
      export const NButton={},NDrawer={},NDrawerContent={},NInput={},NRadioButton={},NRadioGroup={},NSelect={},NSwitch={},NTag={}
      export const useDialogReactiveList = () => globalThis.__storeUpdateSettings.dialogs
    `)
    if (url === 'store-update-test:@vicons/ionicons5') return fake('export const ArrowUndoOutline={},DownloadOutline={},FolderOpenOutline={},InformationCircleOutline={}')
    if (url === 'store-update-test:vue-router') return fake('export const useRoute=()=>({query:{}}),useRouter=()=>({replace(){}})')
    if (url === 'store-update-test:@tauri-apps/api/app') return fake('export const getVersion=async()=>"2.3.2"')
    if (url.endsWith('/src/views/SettingsView.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'store-update-settings-test' }).content }
    }
    if (url.endsWith('.vue')) return fake('export default {}')
    const modules = {
      '/src/services/tauri.ts': `const b=globalThis.__storeUpdateSettings;export const api={
        checkAppUpdate:async(...args)=>{b.calls.push(['check',...args]);return b.result},
        openUpdateRepository:async()=>{b.calls.push(['open']);return b.opening()}}`,
      '/src/composables/useDashboardStore.ts': `import {ref} from 'vue';export const useDashboard=()=>({
        config:ref(null),game:ref(null),mods:ref([]),diyLevelTarget:ref(null),load:async()=>{},ensureLoaded:async()=>{}})`,
      '/src/composables/useDataProviderSwitch.ts': `import {ref} from 'vue';export const useDataProviderSwitch=()=>({switching:ref(false),switchSelection:async()=>{}})`,
      '/src/modChanges/store.ts': `import {ref} from 'vue';export const useModChanges=()=>({state:ref(null)})`,
      '/src/tutorial/demo.ts': `export const demoActive=globalThis.__storeUpdateSettings.demoActive;
        export const withDemoConfig=v=>v,withDemoDiyLevelTarget=v=>v,withDemoGame=v=>v,withDemoMods=v=>v`,
      '/src/useNotify.ts': `export const useNotify=()=>({success:v=>globalThis.__storeUpdateSettings.notices.push(['success',v]),error:v=>globalThis.__storeUpdateSettings.notices.push(['error',v])})`,
      '/src/i18n.ts': 'export const t=key=>key',
      '/src/theme.ts': 'export const tagColorFor=()=>({})',
      '/src/crossLink.ts': 'export const FOCUS_QUERY="focus"',
      '/src/gameRuntime.ts': 'export const displayPath=v=>v',
      '/src/locale/backend.ts': 'export const backendMessageOf=v=>`translated:${String(v)}`;export const translateBackendMessage=String',
    }
    for (const [suffix, source] of Object.entries(modules)) if (url.endsWith(suffix)) return fake(source)
    return next(url, context)
  },
})
const { default: SettingsView } = await import('../src/views/SettingsView.vue')
const shared = await import('../src/appUpdateState.ts')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const result = (patch = {}) => ({ channels: [], all_failed: false, region: 'cn', ...patch })
const info = { version: '9.9.9', current_version: '2.3.2', install_mode: 'manual' }
function mounted(t) {
  Object.assign(toRaw(shared.appUpdateState), { distribution: null, update: null, stage: 'idle', error: '', checkingRecovery: false })
  boundary.calls.length = boundary.notices.length = 0
  boundary.result = result()
  boundary.opening = async () => ({ ok: true })
  const app = renderer.createApp({ setup: () => () => h({ ...SettingsView, render: () => null }) })
  app.mount({})
  t.after(() => app.unmount())
  return app._instance.subTree.component.setupState
}

test('startup Store identity is visible before manual checking and opens Store without a new check', async t => {
  const view = mounted(t)
  assert.equal(view.distributionLabel, '', 'do not show the wrong channel while native identity is pending')
  shared.setAppDistribution('microsoft_store')
  await nextTick()
  assert.equal(view.storeManaged, true)
  assert.equal(view.distributionLabel, 'Microsoft Store')
  await view.checkUpdate()
  assert.deepEqual(boundary.calls, [['open']])
  assert.deepEqual(boundary.notices, [])
  assert.equal(shared.appUpdateState.stage, 'idle')
})

test('first manual check discovering Store never says latest or opens independent prompt', async t => {
  const view = mounted(t)
  boundary.result = result({ distribution: 'microsoft_store', update: info })
  await view.checkUpdate()
  assert.deepEqual(boundary.calls, [['check', true], ['open']])
  assert.deepEqual(boundary.notices, [])
  assert.equal(view.storeManaged, true)
  assert.equal(shared.appUpdateState.stage, 'idle')
})

test('Store launch errors translate normally; duplicate clicks share checking gate', async t => {
  const view = mounted(t)
  shared.setAppDistribution('microsoft_store')
  let reject
  boundary.opening = () => new Promise((_, no) => { reject = no })
  const opening = view.checkUpdate()
  assert.equal(view.checking, true)
  await view.checkUpdate()
  assert.deepEqual(boundary.calls, [['open']])
  reject('store-unavailable')
  await opening
  assert.equal(view.checking, false)
  assert.deepEqual(boundary.notices, [['error', 'translated:store-unavailable']])
})

test('ordinary checks preserve current, update and all-channel failure behavior', async t => {
  const view = mounted(t)
  shared.setAppDistribution('standalone')
  assert.equal(view.distributionLabel, 'update.standaloneChannel')
  await view.checkUpdate()
  assert.deepEqual(boundary.notices, [['success', 'dialogs.youAreUpToDate']])
  assert.equal(view.storeManaged, false)
  boundary.result = result({ all_failed: true, update: info })
  await view.checkUpdate()
  assert.equal(shared.appUpdateState.stage, 'checkFailed')
  assert.deepEqual(boundary.notices.at(-1), ['error', 'dialogs.updateCheckFailed'])
  boundary.result = result({ distribution: 'standalone', update: info })
  await view.checkUpdate()
  assert.equal(shared.appUpdateState.stage, 'stageFailed')
  assert.equal(shared.appUpdateState.error, 'update.manualUpdateRequired')
  assert.equal(boundary.calls.filter(([kind]) => kind === 'open').length, 0)
})

test('About shows the channel, version and a unified update control for both channels', () => {
  const source = readFileSync(new URL('../src/views/SettingsView.vue', import.meta.url), 'utf8')
  assert.deepEqual(parse(source).errors, [])
  assert.doesNotMatch(source, /settings-update|checkForUpdatesOnStartup|store-update-row/)
  const about = source.match(/<SettingsRow[^>]+data-testid="app-about"[^]*?<\/SettingsRow>/)?.[0]
  assert.ok(about)
  assert.match(about, /data-testid="app-distribution"[^]*?data-testid="app-version"[^]*?data-testid="app-check-update"/)
  assert.match(about, /<n-button(?![^>]*\bv-if=)[^>]+@click\.stop="checkUpdate"/)
  assert.equal([...source.matchAll(/@click\.stop="checkUpdate"/g)].length, 1)
})
