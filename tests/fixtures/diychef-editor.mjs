import { registerHooks } from 'node:module'
import { createRenderer, nextTick, shallowRef } from 'vue'

// Keep the production editor, dashboard and Vue lifecycle. Replace only IPC,
// translation and notices; deferred requests exercise actual publication paths.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* try normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/services/tauri.ts')) return { format: 'module', shortCircuit: true, source: `
      export const api = new Proxy({}, {get: (_, key) => (...args) => globalThis.__chefApi[key](...args)})
      export const watchGameState = async callback => { globalThis.__chefGameEvent = callback; return () => {} }
      export const watchDiyChefFiles = callback => {
        globalThis.__chefFilesEvent = callback
        return globalThis.__chefFilesSubscribe ? globalThis.__chefFilesSubscribe() : Promise.resolve(() => { globalThis.__chefFilesStopped++ })
      }
    ` }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true, source: `
      export const useNotify = () => ({error: text => globalThis.__chefNotices.push(text), info: text => globalThis.__chefNotices.push(text), success: text => globalThis.__chefNotices.push(text)})
    ` }
    if (url.endsWith('/src/locale/backend.ts')) return { format: 'module', shortCircuit: true, source: 'export const backendMessageOf = reason => reason instanceof Error ? reason.message : String(reason)' }
    if (url.endsWith('/src/i18n.ts')) return { format: 'module', shortCircuit: true, source: 'export const t = key => key' }
    return next(url, context)
  },
})
const { useDiyChefEditor } = await import('../../src/composables/useDiyChefEditor.ts')
const { useDiyChefConfirmations } = await import('../../src/composables/useDiyChefConfirmations.ts')
const { useDashboard, startGameWatch } = await import('../../src/composables/useDashboardStore.ts')
const { draftSignature } = await import('../../src/diychef/draft.ts')
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
const deferred = () => { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j }); return { promise, resolve, reject } }
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
function mounted(factory) {
  let result
  const app = renderer.createApp({ setup() { result = factory(); return () => null } })
  app.mount({})
  let stopped = false
  return { value: result, dispose() { if (!stopped) { stopped = true; app.unmount() } } }
}
function snapshot(missing = false) {
  const chef = (id, source) => ({ id, source_kind: source, display_name: id, config_name: id === 'cat' ? 'Chef_Black_cat' : id,
    head_name: id, custom_id: source === 'native' ? null : 1, resource_state: source === 'native' ? 'known_name' : 'valid', preview_state: 'pending' })
  return {
    session_id: 'session', target: { id: 'target', guid: 'dev.gua.overcooked.diychef', version: '2.2.0', display_path: 'fixture', enabled: true, generation: 7, compatibility: 'fastinit_2_2' },
    source: { state: missing ? 'missing' : 'present', filename: 'prefer.txt', revision: missing ? 'missing' : 'initial', encoding: 'utf8', newline: 'crlf' },
    catalog_revision: 'catalog', chefs: [chef('cat', 'native'), chef('fox', 'custom'), chef('owl', 'custom')],
    hats: [{ id: 'hat:', kind: 'auto', config_value: '', display_name: 'auto', resource_state: 'known_name', preview_state: 'pending' },
      { id: 'hat:None', kind: 'none', config_value: 'None', display_name: 'none', resource_state: 'known_name', preview_state: 'pending' }],
    document_rows: [], selection: { entries: [{ entry_id: missing ? 'default' : 'row0', row_id: missing ? null : 'row0', chef_ref: { kind: 'known', chef_id: 'cat' }, hat_ref: { kind: 'known', hat_id: 'hat:' } }], lobby_switch: false }, diagnostics: [],
  }
}
const valid = () => ({ diagnostics: [], changes: [], save_blocking: false })
async function setup(t, { missing = false, running = false, target = {}, api = {}, confirmChanges = async () => true, demoSnapshot, confirmMigration, interactionBusy, filesSubscribe } = {}) {
  const initial = snapshot(missing)
  initial.target = { ...initial.target, ...target }
  let current = initial
  globalThis.__chefNotices = []
  globalThis.__chefFilesStopped = 0
  globalThis.__chefFilesSubscribe = filesSubscribe
  globalThis.__chefApi = {
    dashboard: async () => ({ config: { repositories: [], game: { path: 'fixture', runtime: 'native' } }, game: { path: 'fixture', kitchen_running: running } }),
    localMods: async () => ({ generation: 7, mods: [] }),
    diyChefOpen: async () => ({ status: 'ready', candidates: [initial.target], snapshot: current }),
    diyChefInitialize: async () => ({ ...snapshot(), target: initial.target, source: { ...snapshot().source, revision: 'initialized' } }),
    diyChefValidate: async () => valid(),
    diyChefSave: async (_session, draft) => { current = { ...current, source: { ...current.source, revision: 'saved' }, selection: { entries: structuredClone(draft.entries), hat_bindings: structuredClone(draft.hat_bindings), lobby_switch: draft.lobby_switch } }; return current },
    ...api,
  }
  const dashboard = useDashboard()
  await dashboard.loadBase()
  await dashboard.loadLocalMods()
  startGameWatch()
  const instance = mounted(() => useDiyChefEditor({ confirmChanges, cancelChanges() {}, demoSnapshot, confirmMigration, interactionBusy }))
  t.after(instance.dispose)
  return { editor: instance.value, dispose: instance.dispose, initial }
}


export { useDiyChefConfirmations, useDashboard, draftSignature, shallowRef, mounted, snapshot, tick, deferred, valid, setup }
