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
  choose: async () => null,
  migrate: async () => {},
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
      const show=options=>{
        globalThis.__clearAllConfigBoundary.dialogs.push(options);return options
      }
      export const useDialog=()=>({error:show,warning:show})`)
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
        },clearAllConfig:async(...args)=>{b.calls.push(['clear',...args]);return b.clear()},
        chooseManagerDataParentDirectory:async(...args)=>{b.calls.push(['choose',...args]);return b.choose()},
        migrateManagerData:async(...args)=>{b.calls.push(['migrate',...args]);return b.migrate(...args)}}`,
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
function mounted(t, overrides = {}, lifecycle = {}) {
  boundary.calls.length = boundary.dialogs.length = boundary.notices.length = 0
  boundary.directory = async () => 'D:\\Redirected Documents\\OC2ModManager'
  boundary.clear = async () => {}
  boundary.choose = async () => null
  boundary.migrate = async () => {}
  Object.assign(boundary, overrides)
  const app = renderer.createApp({ setup: () => () => h({ ...ToolsView, render: () => null }) })
  app.mount({})
  t.after(() => app.unmount())
  lifecycle.unmount = () => app.unmount()
  return app._instance.subTree.component.setupState
}

test('configuration keeps clear action and adds a non-clickable path row with a migration button', () => {
  const source = readFileSync(new URL('../src/views/ToolsView.vue', import.meta.url), 'utf8')
  assert.deepEqual(parse(source).errors, [], 'the real SFC template must remain valid')
  const group = source.match(/<SettingsGroup :title="t\('tools\.configuration'\)"[\s\S]*?<\/SettingsGroup>/)?.[0]
  assert.ok(group)
  assert.equal((group.match(/<SettingsRow\b/g) ?? []).length, 2)
  const pathRow = group.match(/<SettingsRow[^>]+data-manager-data-path>[\s\S]*?<\/SettingsRow>/)?.[0]
  assert.ok(pathRow)
  assert.doesNotMatch(pathRow.split('<template')[0], /\bclickable\b|@click|role="button"/)
  assert.match(pathRow, /:description="managerDataDescription" :description-title="managerDataDescription"/)
  assert.match(pathRow, /<n-button[^>]+:disabled="managerDataBusy"[\s\S]*?@click="confirmMigrateManagerData"/)
  assert.match(group, /role="button" tabindex="0" aria-haspopup="dialog"/)
  assert.match(group, /@keydown\.enter\.prevent="confirmClearAllConfig"/)
  assert.match(group, /@keydown\.space\.prevent="confirmClearAllConfig"/)
  assert.ok(source.indexOf('data-tutorial="tools-cache"') < source.indexOf(group))
  assert.ok(source.indexOf(group) < source.indexOf('data-tutorial="tools-advanced"'))
  const service = readFileSync(new URL('../src/services/tauri.ts', import.meta.url), 'utf8')
  assert.match(service, /getManagerDataDirectory: \(\) => invoke<string>\('get_manager_data_directory'\)/)
  assert.match(service, /clearAllConfig: \(\) => invoke<void>\('clear_all_config'\)/)
  assert.match(service, /migrateManagerData: \(parentDirectory: string\) => invoke<void>\('migrate_manager_data', \{ parentDirectory \}\)/)
  assert.match(service, /chooseManagerDataParentDirectory: \(\) => open\(\{ multiple: false, directory: true \}\)/)
})

test('opening only reads the actual directory once and cancellation does not clear data', async t => {
  const read = deferred()
  const state = mounted(t, { directory: () => read.promise })
  assert.deepEqual(boundary.calls, [['directory']])
  assert.equal(state.managerDataDescription, 'tools.managerDataDirectoryLoading')
  const first = state.confirmClearAllConfig()
  await state.confirmClearAllConfig()
  assert.deepEqual(boundary.calls, [['directory']])
  assert.equal(boundary.dialogs.length, 0)
  read.resolve('Z:\\User Documents\\OC2ModManager')
  await first
  assert.equal(state.managerDataDescription, 'Z:\\User Documents\\OC2ModManager')
  const confirmation = boundary.dialogs[0]
  assert.equal(confirmation.maskClosable, false)
  assert.equal(confirmation.positiveButtonProps.type, 'error')
  assert.deepEqual(confirmation.content().props.style, { maxHeight: 'calc(100vh - 250px)', overflowY: 'auto' })
  const lines = confirmation.content().children.map(child => child.children)
  assert.deepEqual(lines, ['tools.clearAllConfigConfirm', 'tools.managerDataDirectory',
    'Z:\\User Documents\\OC2ModManager', 'tools.clearAllConfigPreservesGame', 'tools.clearAllConfigExit'])
  confirmation.onAfterLeave()
  assert.equal(state.managerDataBusy, false)
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
  const state = mounted(t, { directory: async () => { throw 'config.directoryUnavailable' } })
  await state.confirmClearAllConfig()
  assert.deepEqual(boundary.calls, [['directory']])
  assert.deepEqual(boundary.notices, ['translated:config.directoryUnavailable'])
  assert.equal(boundary.dialogs.length, 0)
  assert.equal(state.managerDataBusy, false)
  assert.equal(state.managerDataDescription, 'translated:config.directoryUnavailable')
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

test('mount displays the actual data root without opening dialogs or writing configuration', async t => {
  const state = mounted(t)
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(state.managerDataDescription, 'D:\\Redirected Documents\\OC2ModManager')
  assert.equal(state.managerDataBusy, false)
  assert.deepEqual(boundary.calls, [['directory']])
  assert.deepEqual(boundary.dialogs, [])
  assert.deepEqual(boundary.notices, [])
})

test('cancelling native directory selection does not open confirmation or mutate data', async t => {
  const state = mounted(t)
  await state.confirmMigrateManagerData()
  assert.equal(state.managerDataBusy, false)
  assert.deepEqual(boundary.calls, [['directory'], ['choose']])
  assert.deepEqual(boundary.dialogs, [])
})

test('directory selection, confirmation and execution share a gate with clear configuration', async t => {
  const selection = deferred(), migration = deferred()
  const state = mounted(t, { choose: () => selection.promise, migrate: () => migration.promise })
  const opening = state.confirmMigrateManagerData()
  await state.confirmMigrateManagerData()
  await state.confirmClearAllConfig()
  assert.equal(state.selectingDataDirectory, true)
  assert.deepEqual(boundary.calls, [['directory'], ['choose']])
  selection.resolve('E:\\Manager Data\\')
  await opening
  const confirmation = boundary.dialogs[0]
  assert.equal(confirmation.title, 'tools.migrateData')
  assert.equal(confirmation.positiveText, 'tools.migrateDataAndExit')
  assert.equal(confirmation.positiveButtonProps.type, 'warning')
  assert.deepEqual(confirmation.content().props.style, { maxHeight: 'calc(100vh - 250px)', overflowY: 'auto' })
  assert.equal(confirmation.maskClosable, false)
  assert.deepEqual(confirmation.content().children.map(child => child.children), [
    'tools.migrateDataConfirm', 'tools.migrateDataSource', 'D:\\Redirected Documents\\OC2ModManager',
    'tools.migrateDataTarget', 'E:\\Manager Data\\OC2ModManager',
    'tools.migrateDataPreservesGame', 'tools.migrateDataExit',
  ])
  await state.confirmClearAllConfig()
  await state.confirmMigrateManagerData()
  const first = confirmation.onPositiveClick()
  await confirmation.onPositiveClick()
  await state.confirmClearAllConfig()
  assert.deepEqual(boundary.calls, [['directory'], ['choose'], ['migrate', 'E:\\Manager Data\\']])
  assert.equal(state.migratingData, true)
  assert.equal(confirmation.loading, true)
  assert.equal(confirmation.closable, false)
  assert.equal(confirmation.closeOnEsc, false)
  assert.equal(confirmation.negativeButtonProps.disabled, true)
  migration.resolve()
  assert.equal(await first, false)
  assert.equal(state.managerDataBusy, true)
  assert.equal(state.migratingData, true)
  assert.deepEqual(boundary.notices, [])
})

test('clear confirmation and execution prevent migration from opening the native picker', async t => {
  const state = mounted(t)
  await state.confirmClearAllConfig()
  await state.confirmMigrateManagerData()
  const clearing = deferred()
  boundary.clear = () => clearing.promise
  const first = boundary.dialogs[0].onPositiveClick()
  await state.confirmMigrateManagerData()
  assert.deepEqual(boundary.calls, [['directory'], ['clear']])
  clearing.resolve()
  await first
  await state.confirmMigrateManagerData()
  assert.deepEqual(boundary.calls, [['directory'], ['clear']])
})

test('cancelling migration confirmation releases the gate and invalidates its old callback', async t => {
  const state = mounted(t, { choose: async () => '/Users/example/Data/' })
  await state.confirmMigrateManagerData()
  const confirmation = boundary.dialogs[0]
  assert.equal(confirmation.content().children[4].children, '/Users/example/Data/OC2ModManager')
  confirmation.onAfterLeave()
  assert.equal(state.managerDataBusy, false)
  assert.equal(await confirmation.onPositiveClick(), false)
  assert.deepEqual(boundary.calls, [['directory'], ['choose']])
  await state.confirmClearAllConfig()
  assert.equal(boundary.dialogs.length, 2)
})

test('filesystem root selection and forward-slash Windows paths show the fixed child folder', async t => {
  const state = mounted(t, { choose: async () => '/' })
  await state.confirmMigrateManagerData()
  assert.equal(boundary.dialogs[0].content().children[4].children, '/OC2ModManager')
  boundary.dialogs[0].onAfterLeave()
  boundary.choose = async () => 'E:/'
  await state.confirmMigrateManagerData()
  assert.equal(boundary.dialogs[1].content().children[4].children, 'E:/OC2ModManager')
})

test('native picker errors are translated and release both operation gates', async t => {
  const state = mounted(t, { choose: async () => { throw 'tools.migrateDataFailed?reason=picker' } })
  await state.confirmMigrateManagerData()
  assert.equal(state.managerDataBusy, false)
  assert.deepEqual(boundary.dialogs, [])
  assert.deepEqual(boundary.notices, ['translated:tools.migrateDataFailed?reason=picker'])
  await state.confirmClearAllConfig()
  assert.equal(boundary.dialogs.length, 1)
})

test('unknown source path prevents migration confirmation and supports an explicit retry', async t => {
  const state = mounted(t, { directory: async () => { throw 'config.directoryUnavailable' },
    choose: async () => '/Volumes/External' })
  await state.confirmMigrateManagerData()
  assert.deepEqual(boundary.dialogs, [])
  assert.deepEqual(boundary.notices, ['translated:config.directoryUnavailable'])
  assert.equal(state.managerDataBusy, false)
  boundary.directory = async () => '/Users/example/Library/Application Support/OC2ModManager'
  await state.confirmMigrateManagerData()
  assert.equal(boundary.dialogs.length, 1)
  assert.equal(state.managerDataDescription, '/Users/example/Library/Application Support/OC2ModManager')
})

test('unsafe, busy and preparation errors keep migration confirmation cancellable and retryable', async t => {
  const state = mounted(t, { choose: async () => 'E:\\Data' })
  await state.confirmMigrateManagerData()
  const confirmation = boundary.dialogs[0]
  for (const error of ['tools.migrateDataUnsafe', 'tools.clearAllConfigBusy', 'tools.migrateDataFailed?reason=exists']) {
    boundary.migrate = async () => { throw error }
    assert.equal(await confirmation.onPositiveClick(), false)
    assert.equal(state.migratingData, false)
    assert.equal(state.managerDataBusy, true)
    assert.equal(confirmation.loading, false)
    assert.equal(confirmation.closable, true)
    assert.equal(confirmation.closeOnEsc, true)
    assert.equal(confirmation.negativeButtonProps.disabled, false)
    assert.equal(boundary.notices.at(-1), `translated:${error}`)
    await state.confirmClearAllConfig()
  }
  boundary.migrate = async () => {}
  await confirmation.onPositiveClick()
  assert.equal(state.migratingData, true)
  assert.equal(boundary.calls.filter(([name]) => name === 'migrate').length, 4)
  assert.equal(boundary.calls.filter(([name]) => name === 'clear').length, 0)
})

test('unmount while native selection waits cannot open a late migration confirmation', async t => {
  const selection = deferred(), lifecycle = {}
  const state = mounted(t, { choose: () => selection.promise }, lifecycle)
  const opening = state.confirmMigrateManagerData()
  lifecycle.unmount()
  selection.resolve('E:\\Data')
  await opening
  assert.deepEqual(boundary.dialogs, [])
  assert.equal(state.managerDataBusy, false)
  assert.equal(boundary.calls.filter(([name]) => name === 'migrate').length, 0)
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
  for (const text of Object.values(toolsUiDataReset.migrateDataFailed)) assert.match(text, /\{reason\}/)
  assert.match(toolsUiDataReset.migrateDataConfirm['zh-CN'], /整个.*设置.*安装清单.*缓存.*备份.*回收站.*OC2ModManager.*子目录已存在.*取消迁移/)
  assert.match(toolsUiDataReset.migrateDataPreservesGame['zh-CN'], /游戏目录.*不会被移动/)
  assert.match(toolsUiDataReset.migrateDataExit['zh-CN'], /退出.*迁移.*手动启动.*新路径/)
})
