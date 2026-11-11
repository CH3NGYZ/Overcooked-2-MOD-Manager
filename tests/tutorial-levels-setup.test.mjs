import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, nextTick } from 'vue'

// Mount the production setup with real tutorial data, stores and grouping/tag
// calculations. Replace only IPC, navigation and UI boundaries, not watchers.
const boundary = { dataCalls: [], subscriptions: [], notices: [], dialogs: [] }
globalThis.__tutorialLevelsSetupBoundary = boundary
registerHooks({
  resolve(specifier, context, next) {
    if (['naive-ui', '@vicons/ionicons5', 'vue-router'].includes(specifier)) {
      return { url: `tutorial-levels:${specifier}`, shortCircuit: true }
    }
    if (context.parentURL?.startsWith('tutorial-levels:')) {
      return next(specifier, { ...context, parentURL: import.meta.url })
    }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* try directory */ }
      try { return next(`${specifier}/index.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/views/LevelsView.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'tutorial-levels-setup-test' }).content }
    }
    if (url.endsWith('.vue')) return { format: 'module', shortCircuit: true, source: 'export default {}' }
    const modules = {
      'tutorial-levels:naive-ui': `
        export const NCollapse = {}, NCollapseItem = {}, NEmpty = {}, NFloatButton = {}, NTag = {}
        export const dateEnUS = {}, dateJaJP = {}, dateKoKR = {}, dateZhCN = {}, dateZhTW = {}
        export const enUS = {}, jaJP = {}, koKR = {}, zhCN = {}, zhTW = {}
        export const useDialog = () => ({warning: (...args) => {
          globalThis.__tutorialLevelsSetupBoundary.dialogs.push(args)
          return {}
        }})
      `,
      'tutorial-levels:@vicons/ionicons5': 'export const AlbumsOutline = {}, FolderOpenOutline = {}, RefreshOutline = {}',
      'tutorial-levels:vue-router': `
        import { shallowRef } from 'vue'
        const currentRoute = shallowRef({ name: 'levels', query: {} })
        const router = { currentRoute, push() {}, replace() {} }
        export const useRouter = () => router
      `,
      '/src/services/tauri.ts': `
        const boundary = globalThis.__tutorialLevelsSetupBoundary
        export const api = new Proxy({}, {get: (_, key) => (...args) => {
          boundary.dataCalls.push([key, ...args])
          return Promise.resolve(undefined)
        }})
        const subscribe = name => async () => {
          boundary.subscriptions.push(name)
          return () => {}
        }
        export const watchGameState = subscribe('game-state')
        export const watchLevelSetScan = subscribe('level-set-scan')
        export const watchLevelSetZipProgress = subscribe('level-set-zip-progress')
        export const watchLevelSetsChanged = subscribe('level-sets-changed')
      `,
      '/src/useNotify.ts': `
        export const useNotify = () => Object.fromEntries(
          ['error', 'warning', 'success', 'info'].map(kind => [kind, (...args) =>
            globalThis.__tutorialLevelsSetupBoundary.notices.push([kind, ...args])]))
      `,
      '/src/theme.ts': 'export const tagColorFor = () => ({})',
    }
    for (const [suffix, source] of Object.entries(modules)) {
      if (url.endsWith(suffix)) return { format: 'module', shortCircuit: true, source }
    }
    return next(url, context)
  },
})

const { default: LevelsView } = await import('../src/views/LevelsView.vue')
const demo = await import('../src/tutorial/demo.ts')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }

test('local maps setup evaluates immediate grouping watchers with tutorial dependencies and no data IPC', async () => {
  const originalDocument = Object.getOwnPropertyDescriptor(globalThis, 'document')
  Object.defineProperty(globalThis, 'document', { configurable: true, value: {
    documentElement: { lang: '' }, addEventListener() {}, removeEventListener() {},
  } })
  for (const entries of Object.values(boundary)) entries.length = 0
  const errors = []
  const app = renderer.createApp({ ...LevelsView, render: () => null })
  app.config.errorHandler = error => errors.push(error)
  demo.startTutorialDemo()
  try {
    const expectedSets = demo.withDemoLevelSets([])
    assert.equal(expectedSets.length, 2, 'nonempty tutorial maps must exercise dependency tags during setup')
    assert.equal(demo.withDemoMaps([]).length, 2)
    assert.ok(demo.withDemoMods([]).length > 0)
    assert.ok(demo.withDemoConfig(null).repositories.length > 0)
    assert.doesNotThrow(() => app.mount({}))
    await tick()
    assert.deepEqual(errors, [], 'immediate grouping watchers must not read uninitialized setup bindings')

    const state = app._instance.setupState
    assert.equal(state.loading, false)
    assert.equal(state.diyLevelAvailable, true, 'tutorial maps do not depend on the real DLL scan or selected target')
    assert.equal(state.diyLevelMissing, false)
    assert.equal(state.sets.length, 2)
    assert.equal(state.visibleSets.length, 2)
    assert.deepEqual(state.groups.flatMap(group => group.sets).map(set => set.dir_name).sort(),
      expectedSets.map(set => set.dir_name).sort())
    assert.ok(state.groups.every(group => group.key && group.label && group.sets.length))
    for (const set of state.visibleSets) {
      assert.ok(Array.isArray(state.tagsOf(set)))
      assert.ok(state.boundMapOf(set), 'each tutorial set is bound through the real identity matcher')
    }
    assert.ok(state.statusSummary.length > 0)

    await state.load()
    await state.reloadSets()
    await tick()
    assert.deepEqual(errors, [])
    assert.deepEqual(boundary.dataCalls, [], 'mount and explicit local reloads must stay offline during the tutorial')
    assert.deepEqual(boundary.notices, [])
    assert.deepEqual(boundary.dialogs, [])
  } finally {
    // Stop page watchers before exiting demo mode would resume real loading.
    app.unmount()
    demo.stopTutorialDemo()
    if (originalDocument) Object.defineProperty(globalThis, 'document', originalDocument)
    else delete globalThis.document
  }
})
