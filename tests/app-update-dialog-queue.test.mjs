import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef, toRaw } from 'vue'

const dialogs = globalThis.__updateDialogQueue = shallowRef([])
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'update-dialog-test:naive', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const stub = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'update-dialog-test:naive') return stub(`
      export const useDialogReactiveList = () => globalThis.__updateDialogQueue
      export const NModal = { emits: ['after-leave'], setup(_, { emit }) {
        globalThis.__finishUpdateFixtureLeave = () => emit('after-leave')
        return () => null
      } }
    `)
    if (url.endsWith('/src/components/AppModal.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'update-dialog-queue', inlineTemplate: true }).content }
    }
    if (url.endsWith('/src/modChanges/store.ts')) return stub('export const useModChanges = () => ({ state: { value: null } })')
    if (url.endsWith('/src/services/tauri.ts')) return stub('export const api = {}')
    return next(url, context)
  },
})

const { default: AppModal } = await import('../src/components/AppModal.vue')
const { useAppUpdatePromptMode } = await import('../src/composables/useAppUpdates.ts')
const state = await import('../src/appUpdateState.ts')
const { appModalOpen } = await import('../src/dialogBusy.ts')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})

function setup(t) {
  const show = shallowRef(false)
  const include = shallowRef(true)
  const appUpdateState = toRaw(state.appUpdateState)
  const originalState = { ...appUpdateState }
  const mode = useAppUpdatePromptMode()
  const app = renderer.createApp({ render: () => include.value ? h(AppModal, { show: show.value }) : null })
  const offer = { version: 'fixture', current_version: 'current', background_install: true, source_channel: 'fixture' }
  const offerResult = { update: offer, all_failed: false }
  app.mount({})
  t.after(() => {
    app.unmount()
    dialogs.value = []
    Object.assign(appUpdateState, originalState)
  })
  return { show, include, mode, offer, offerResult, finishLeave: () => globalThis.__finishUpdateFixtureLeave() }
}

test('release notes remain queued until both existing modal surfaces finish leaving', async t => {
  const { show, mode, offer, offerResult, finishLeave } = setup(t)
  show.value = true
  dialogs.value = [{ key: 'confirmation' }]
  await nextTick()
  assert.equal(state.beginAppUpdateCheck(), true)
  assert.equal(state.finishAppUpdateCheck(offerResult), 'fixture')
  state.finishBackgroundAppUpdate({ update: offer })
  assert.equal(mode.value, null)
  assert.equal(state.appUpdateState.notesPending, true)

  show.value = false
  await nextTick()
  assert.equal(appModalOpen.value, true)
  finishLeave()
  assert.equal(mode.value, null, 'the programmatic confirmation is still present')
  dialogs.value = []
  assert.equal(mode.value, 'notes')
  state.dismissAppUpdateNotes()
  assert.equal(mode.value, null)
})

test('unmount clears an open modal, and required recovery takes precedence over both surfaces', async t => {
  const { show, include, mode, offerResult, finishLeave } = setup(t)
  show.value = true
  await nextTick()
  finishLeave()
  assert.equal(appModalOpen.value, true, 'stale leave completion must not unregister a visible modal')
  dialogs.value = [{ key: 'confirmation' }]
  assert.equal(state.beginAppUpdateCheck(), true)
  state.finishAppUpdateCheck({ ...offerResult, recovery_required: true })
  assert.equal(mode.value, 'recovery')
  include.value = false
  await nextTick()
  assert.equal(appModalOpen.value, false)
  assert.equal(mode.value, 'recovery')
})
