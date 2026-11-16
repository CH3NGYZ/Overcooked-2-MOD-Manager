import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h } from 'vue'

// Execute the production view setup; only IPC, dialogs and unrelated services
// are replaced. No cleanup command reaches Tauri or the user's data directory.
const boundary = globalThis.__clearAllConfigBoundary = {
  calls: [], dialogs: [], notices: [],
  directory: async () => 'D:\\Redirected Documents\\OC2ModManager',
  clear: async () => {},
}
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'clear-config-test:naive', shortCircuit: true }
    if (specifier === '@vicons/ionicons5') return { url: 'clear-config-test:icons', shortCircuit: true }
    if (specifier === 'vue-router') return { url: 'clear-config-test:router', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue|jpg)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'clear-config-test:naive') return fake(`
      export const NButton={},NLog={},NSelect={},NTag={}
      export const useDialog=()=>({error:options=>{
        globalThis.__clearAllConfigBoundary.dialogs.push(options);return options
      }})`)
    if (url === 'clear-config-test:icons') return fake(`
      export const BookOutline={},BuildOutline={},ChatbubblesOutline={},CloudOfflineOutline={},CloudUploadOutline={},
        DocumentTextOutline={},MapOutline={},PersonOutline={},RefreshOutline={},ServerOutline={},TerminalOutline={},TrashOutline={}`)
    if (url === 'clear-config-test:router') return fake('export const useRouter=()=>({push(){}})')
    if (url.endsWith('/src/views/ToolsView.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'clear-all-config-test' }).content }
    }
    if (url.endsWith('.vue')) return fake('export default {}')
    if (url.endsWith('.jpg')) return fake('export default "sponsor-test.jpg"')
    const modules = {
      '/src/services/tauri.ts': `
        const b=globalThis.__clearAllConfigBoundary
        export const api={getManagerDataDirectory:async(...args)=>{
          b.calls.push(['directory',...args]);return b.directory()
        },clearAllConfig:async(...args)=>{b.calls.push(['clear',...args]);return b.clear()}}`,
      '/src/avatars.ts': 'export const clearAvatarCache=()=>{}',
      '/src/composables/useDiyChefThumbnails.ts': 'export const clearChefThumbnailCache=async()=>0',
      '/src/useNotify.ts': 'export const useNotify=()=>({error:value=>globalThis.__clearAllConfigBoundary.notices.push(value)})',
      '/src/i18n.ts': 'export const t=key=>key',
      '/src/locale/backend.ts': 'export const backendMessageOf=value=>`translated:${String(value)}`;export const translateBackendMessage=String',
      '/src/tutorial/engine.ts': 'export const replayTutorial=()=>{}',
    }
    for (const [suffix, source] of Object.entries(modules)) if (url.endsWith(suffix)) return fake(source)
    return next(url, context)
  },
})
const { default: ToolsView } = await import('../src/views/ToolsView.vue')
const { toolsUiDataReset } = await import('../src/locale/tools/dataReset.ts')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
function mounted(t) {
  boundary.calls.length = boundary.dialogs.length = boundary.notices.length = 0
  boundary.directory = async () => 'D:\\Redirected Documents\\OC2ModManager'
  boundary.clear = async () => {}
  const app = renderer.createApp({ setup: () => () => h({ ...ToolsView, render: () => null }) })
  app.mount({})
  t.after(() => app.unmount())
  return app._instance.subTree.component.setupState
}

test('configuration is a separate group directly below cache with one accessible action', () => {
  const source = readFileSync(new URL('../src/views/ToolsView.vue', import.meta.url), 'utf8')
  const group = source.match(/<SettingsGroup :title="t\('tools\.configuration'\)"[\s\S]*?<\/SettingsGroup>/)?.[0]
  assert.ok(group)
  assert.equal((group.match(/<SettingsRow\b/g) ?? []).length, 1)
  assert.match(group, /role="button" tabindex="0" aria-haspopup="dialog"/)
  assert.match(group, /@keydown\.enter\.prevent="confirmClearAllConfig"/)
  assert.match(group, /@keydown\.space\.prevent="confirmClearAllConfig"/)
  assert.ok(source.indexOf('data-tutorial="tools-cache"') < source.indexOf(group))
  assert.ok(source.indexOf(group) < source.indexOf('data-tutorial="tools-advanced"'))
  const service = readFileSync(new URL('../src/services/tauri.ts', import.meta.url), 'utf8')
  assert.match(service, /getManagerDataDirectory: \(\) => invoke<string>\('get_manager_data_directory'\)/)
  assert.match(service, /clearAllConfig: \(\) => invoke<void>\('clear_all_config'\)/)
})

test('opening only reads the actual directory once and cancellation does not clear data', async t => {
  const state = mounted(t)
  assert.deepEqual(boundary.calls, [])
  const read = deferred()
  boundary.directory = () => read.promise
  const first = state.confirmClearAllConfig()
  await state.confirmClearAllConfig()
  assert.deepEqual(boundary.calls, [['directory']])
  assert.equal(boundary.dialogs.length, 0)
  read.resolve('Z:\\User Documents\\OC2ModManager')
  await first
  const confirmation = boundary.dialogs[0]
  assert.equal(confirmation.maskClosable, false)
  assert.equal(confirmation.positiveButtonProps.type, 'error')
  const lines = confirmation.content().children.map(child => child.children)
  assert.deepEqual(lines, ['tools.clearAllConfigConfirm', 'tools.managerDataDirectory',
    'Z:\\User Documents\\OC2ModManager', 'tools.clearAllConfigPreservesGame', 'tools.clearAllConfigExit'])
  confirmation.onAfterLeave()
  assert.equal(state.configConfirmOpen, false)
  assert.deepEqual(boundary.calls, [['directory']])
})

test('confirmed clearing has one shared click gate and stays locked while process exit completes', async t => {
  const state = mounted(t)
  const clearing = deferred()
  boundary.clear = () => clearing.promise
  await state.confirmClearAllConfig()
  const confirmation = boundary.dialogs[0]
  const first = confirmation.onPositiveClick()
  await confirmation.onPositiveClick()
  await state.confirmClearAllConfig()
  assert.deepEqual(boundary.calls, [['directory'], ['clear']])
  assert.equal(state.clearingConfig, true)
  assert.equal(confirmation.loading, true)
  assert.equal(confirmation.closable, false)
  assert.equal(confirmation.closeOnEsc, false)
  assert.equal(confirmation.negativeButtonProps.disabled, true)
  clearing.resolve()
  assert.equal(await first, false)
  assert.equal(state.clearingConfig, true)
  assert.deepEqual(boundary.notices, [])
})

test('read failure shows the translated error and never opens destructive confirmation', async t => {
  const state = mounted(t)
  boundary.directory = async () => { throw 'config.directoryUnavailable' }
  await state.confirmClearAllConfig()
  assert.deepEqual(boundary.calls, [['directory']])
  assert.deepEqual(boundary.notices, ['translated:config.directoryUnavailable'])
  assert.equal(boundary.dialogs.length, 0)
  assert.equal(state.configConfirmOpen, false)
})

test('cleanup preparation failure restores cancellation and permits explicit retry', async t => {
  const state = mounted(t)
  boundary.clear = async () => { throw 'tools.clearAllConfigFailed?reason=blocked' }
  await state.confirmClearAllConfig()
  const confirmation = boundary.dialogs[0]
  assert.equal(await confirmation.onPositiveClick(), false)
  assert.equal(state.clearingConfig, false)
  assert.equal(confirmation.loading, false)
  assert.equal(confirmation.closable, true)
  assert.equal(confirmation.closeOnEsc, true)
  assert.equal(confirmation.negativeButtonProps.disabled, false)
  assert.deepEqual(boundary.notices, ['translated:tools.clearAllConfigFailed?reason=blocked'])
  boundary.clear = async () => {}
  await confirmation.onPositiveClick()
  assert.deepEqual(boundary.calls, [['directory'], ['clear'], ['clear']])
})

test('the reset domain has complete five-language irreversible-data and exit wording', () => {
  const locales = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']
  for (const [key, translations] of Object.entries(toolsUiDataReset)) {
    assert.deepEqual(Object.keys(translations).sort(), [...locales].sort(), key)
    for (const text of Object.values(translations)) assert.ok(text.trim(), key)
  }
  for (const text of Object.values(toolsUiDataReset.clearAllConfigFailed)) assert.match(text, /\{reason\}/)
  assert.match(toolsUiDataReset.clearAllConfigConfirm['zh-CN'], /不可撤销.*设置.*安装清单.*缓存.*备份.*回收站/)
  assert.match(toolsUiDataReset.clearAllConfigPreservesGame['zh-CN'], /不会被删除/)
  assert.match(toolsUiDataReset.clearAllConfigExit['zh-CN'], /退出.*下次启动.*默认设置/)
})
