import assert from 'node:assert/strict'
import test from 'node:test'
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
    ` }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true, source: `
      export const useNotify = () => ({error: text => globalThis.__chefNotices.push(text), info: text => globalThis.__chefNotices.push(text), success: text => globalThis.__chefNotices.push(text)})
    ` }
    if (url.endsWith('/src/locale/backend.ts')) return { format: 'module', shortCircuit: true, source: 'export const backendMessageOf = reason => reason instanceof Error ? reason.message : String(reason)' }
    if (url.endsWith('/src/i18n.ts')) return { format: 'module', shortCircuit: true, source: 'export const t = key => key' }
    return next(url, context)
  },
})
const { useDiyChefEditor } = await import('../src/composables/useDiyChefEditor.ts')
const { useDiyChefConfirmations } = await import('../src/composables/useDiyChefConfirmations.ts')
const { useDashboard, startGameWatch } = await import('../src/composables/useDashboardStore.ts')
const { draftSignature } = await import('../src/diychef/draft.ts')
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
async function setup(t, { missing = false, running = false, api = {}, confirmChanges = async () => true, demoSnapshot } = {}) {
  const initial = snapshot(missing)
  globalThis.__chefNotices = []
  globalThis.__chefApi = {
    dashboard: async () => ({ config: { repositories: [], game: { path: 'fixture', runtime: 'native' } }, game: { path: 'fixture', kitchen_running: running } }),
    localMods: async () => ({ generation: 7, mods: [] }),
    diyChefOpen: async () => ({ status: 'ready', candidates: [initial.target], snapshot: initial }),
    diyChefInitialize: async () => ({ ...snapshot(), source: { ...snapshot().source, revision: 'initialized' } }),
    diyChefValidate: async () => valid(),
    diyChefSave: async (_session, draft) => ({ ...snapshot(), source: { ...snapshot().source, revision: 'saved' }, selection: { entries: draft.entries, lobby_switch: draft.lobby_switch } }),
    ...api,
  }
  const dashboard = useDashboard()
  await dashboard.loadBase()
  await dashboard.loadLocalMods()
  startGameWatch()
  const instance = mounted(() => useDiyChefEditor({ confirmChanges, cancelChanges() {}, demoSnapshot }))
  t.after(instance.dispose)
  return { editor: instance.value, dispose: instance.dispose, initial }
}

test('missing config remains editable while running; stopping initializes only the baseline', async t => {
  const create = deferred()
  let initialized = 0
  const { editor } = await setup(t, { missing: true, running: true, api: { diyChefInitialize: () => { initialized++; return create.promise } } })
  await editor.open()
  assert.equal(initialized, 0)
  editor.toggle('fox')
  editor.setDefault('fox')
  editor.changeHat('owl', { kind: 'known', hat_id: 'hat:None' })
  editor.setLobby(true)
  const edited = structuredClone(editor.state.value)
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.canSave.value, false)
  globalThis.__chefGameEvent({ path: 'fixture', kitchen_running: false })
  await tick()
  assert.equal(initialized, 1)
  assert.equal(editor.initializing.value, true)
  create.resolve({ ...snapshot(), source: { ...snapshot().source, revision: 'initialized' } })
  await tick()
  assert.equal(editor.snapshot.value.source.revision, 'initialized')
  assert.equal(editor.state.value.draft.base_revision, 'initialized')
  assert.equal(draftSignature(editor.state.value.draft), draftSignature(edited.draft))
  assert.deepEqual(editor.state.value.disabledHats, edited.disabledHats)
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.canSave.value, true)
  editor.undo()
  assert.equal(editor.state.value.draft.base_revision, 'initialized', 'undo also uses the new disk baseline')
})

test('opening and initializing missing chef config waits for the successful current local MOD scan', async t => {
  const events = [], scan = deferred()
  const current = () => ({ ...snapshot(true), target: { ...snapshot(true).target, generation: 17 } })
  const { editor } = await setup(t, { missing: true, api: {
    diyChefOpen: async generation => {
      events.push(`open:${generation}`)
      return { status: 'ready', candidates: [], snapshot: current() }
    },
    diyChefInitialize: async () => {
      events.push('initialize')
      return { ...current(), source: { ...snapshot().source, revision: 'initialized' } }
    },
  } })
  const d = useDashboard()
  d.applyConfigSnapshot({ ...d.config.value, game: { path: 'chef-scan-barrier', runtime: 'native' } })
  globalThis.__chefApi.localMods = async () => {
    events.push('scan-start')
    const result = await scan.promise
    events.push('scan-complete')
    return result
  }
  const opening = editor.open()
  await tick()
  assert.deepEqual(events, ['scan-start'])
  assert.equal(editor.loading.value, true)
  assert.equal(editor.snapshot.value, null)
  scan.resolve({ generation: 17, mods: [] })
  await opening
  assert.deepEqual(events, ['scan-start', 'scan-complete', 'open:17', 'initialize'])
  assert.equal(editor.snapshot.value.source.revision, 'initialized')
})

test('external creation during missing-file initialization retains edited draft and reports conflict', async t => {
  const { editor } = await setup(t, { missing: true, running: true, api: { diyChefInitialize: async () => { throw new Error('diychef.conflict') } } })
  await editor.open()
  editor.toggle('fox')
  const edited = structuredClone(editor.state.value)
  globalThis.__chefGameEvent({ path: 'fixture', kitchen_running: false })
  await tick()
  assert.equal(editor.error.value, 'diychef.conflict')
  assert.deepEqual(editor.state.value, edited)
  assert.equal(editor.snapshot.value.source.state, 'missing')
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.canSave.value, false)
})

test('placing a disabled custom chef first clamps behind the native and remains one undoable edit', async t => {
  const { editor } = await setup(t)
  await editor.open()
  editor.changeHat('fox', { kind: 'known', hat_id: 'hat:None' })
  editor.changeHat('owl', { kind: 'known', hat_id: 'hat:None' })
  const before = structuredClone(editor.state.value)
  assert.equal(editor.dirty.value, false, 'disabled hat preferences have not changed prefer.txt')
  editor.place('fox', 'cat', false)
  assert.deepEqual(editor.state.value.draft.entries.map(entry => entry.chef_ref.chef_id), ['cat', 'fox'])
  assert.deepEqual(editor.state.value.draft.entries[1].hat_ref, { kind: 'known', hat_id: 'hat:None' })
  assert.deepEqual(editor.state.value.disabledHats, { owl: { kind: 'known', hat_id: 'hat:None' } })
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.canUndo.value, true)
  editor.undo()
  assert.deepEqual(editor.state.value, before, 'one undo restores the full state before enabling and moving Fox')
  assert.equal(editor.dirty.value, false)
  assert.equal(editor.canUndo.value, false)
  editor.undo()
  assert.deepEqual(editor.state.value, before, 'placing Fox did not leave a second undo step for enabling it')
})

test('save freezes all mutations through validation, confirmation and write/read-back', async t => {
  const check = deferred(), confirm = deferred(), write = deferred()
  let writes = 0, confirmationCalls = 0, opens = 0
  const { editor } = await setup(t, { api: {
    diyChefOpen: async () => { opens++; return { status: 'ready', candidates: [], snapshot: snapshot() } },
    diyChefValidate: () => check.promise,
    diyChefSave: () => { writes++; return write.promise },
  }, confirmChanges: () => { confirmationCalls++; return confirm.promise } })
  await editor.open()
  editor.toggle('fox')
  const frozen = structuredClone(editor.state.value)
  const save = editor.save()
  assert.equal(editor.saving.value, true)
  assert.equal(editor.editable.value, false)
  const attemptEdits = () => {
    editor.toggle('owl'); editor.setDefault('fox'); editor.move('fox', 'cat')
    editor.place('owl', 'cat', false); editor.place('fox', 'cat', false)
    editor.changeHat('fox', { kind: 'known', hat_id: 'hat:None' }); editor.setLobby(true)
    editor.removeProblem('row0'); editor.reset('restore_default'); editor.undo()
    assert.deepEqual(editor.state.value, frozen)
  }
  attemptEdits()
  await editor.open('other')
  assert.equal(opens, 1)
  assert.equal(await editor.save(), false)
  check.resolve({ ...valid(), changes: [{ id: 'normalize', before: 'before', after: 'after' }] })
  await tick()
  assert.equal(confirmationCalls, 1)
  assert.equal(writes, 0)
  attemptEdits()
  confirm.resolve(true)
  await tick()
  assert.equal(writes, 1)
  attemptEdits()
  write.resolve({ ...snapshot(), source: { ...snapshot().source, revision: 'saved' }, selection: { entries: frozen.draft.entries, lobby_switch: false } })
  assert.equal(await save, true)
  assert.equal(editor.saving.value, false)
  assert.equal(editor.dirty.value, false)
  assert.equal(editor.snapshot.value.source.revision, 'saved')
})

test('failed write and cancelled normalization keep the draft dirty and editable', async t => {
  const { editor } = await setup(t, { api: { diyChefSave: async () => { throw new Error('diychef.saveFailed') } } })
  await editor.open()
  editor.toggle('fox')
  const draft = structuredClone(editor.state.value)
  assert.equal(await editor.save(), false)
  assert.equal(editor.error.value, 'diychef.saveFailed')
  assert.deepEqual(editor.state.value, draft)
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.editable.value, true)
})

test('cancelling normalization performs no write and changes no confirmation credentials', async t => {
  let writes = 0
  const { editor } = await setup(t, { api: {
    diyChefValidate: async () => ({ ...valid(), changes: [{ id: 'x', before: 'old', after: 'new' }] }),
    diyChefSave: async () => { writes++; return snapshot() },
  }, confirmChanges: async () => false })
  await editor.open()
  editor.toggle('fox')
  const before = structuredClone(editor.state.value)
  assert.equal(await editor.save(), false)
  assert.equal(writes, 0)
  assert.deepEqual(editor.state.value, before)
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.editable.value, true)
})

test('successful save keeps hats tried on disabled chefs for the current editor session', async t => {
  const { editor } = await setup(t)
  await editor.open()
  editor.changeHat('owl', { kind: 'known', hat_id: 'hat:None' })
  editor.toggle('fox')
  assert.equal(await editor.save(), true)
  assert.deepEqual(editor.state.value.disabledHats.owl, { kind: 'known', hat_id: 'hat:None' })
  assert.equal(editor.dirty.value, false)
})

test('revision conflicts and write/read-back failures require reloading, while the draft stays editable', async t => {
  let writes = 0
  const { editor } = await setup(t, { api: { diyChefSave: async () => { writes++; throw new Error('diychef.writtenReadFailed') } } })
  await editor.open()
  editor.toggle('fox')
  assert.equal(await editor.save(), false)
  assert.equal(editor.canSave.value, false)
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.editable.value, true)
  editor.setLobby(true)
  assert.equal(editor.state.value.draft.lobby_switch, true)
  assert.equal(await editor.save(), false)
  assert.equal(writes, 1, 'never blindly retry against the revision that was already written')
  await editor.open()
  editor.toggle('fox')
  assert.equal(editor.canSave.value, true, 'a successful read establishes usable revision credentials again')
  globalThis.__chefApi.diyChefSave = async () => { throw new Error('diychef.conflict?path=changed') }
  assert.equal(await editor.save(), false)
  assert.equal(editor.canSave.value, false)
  assert.equal(editor.editable.value, true)
})

test('late initialization after unmount cannot replace the draft or schedule validation', async t => {
  const create = deferred()
  let validations = 0
  const { editor, dispose } = await setup(t, { missing: true, api: {
    diyChefInitialize: () => create.promise,
    diyChefValidate: async () => { validations++; return valid() },
  } })
  const opened = editor.open()
  await tick()
  const before = editor.state.value
  dispose()
  create.resolve(snapshot())
  await opened
  await new Promise(resolve => setTimeout(resolve, 310))
  assert.strictEqual(editor.state.value, before)
  assert.equal(validations, 0)
})

test('confirmation cancellation, replacement and unmount settle pending promises', async () => {
  const instance = mounted(() => useDiyChefConfirmations(async () => false))
  const c = instance.value
  const first = c.confirmLeave()
  c.cancelLeave()
  assert.equal(await first, false)
  const old = c.confirmLeave()
  const newer = c.confirmLeave()
  assert.equal(await old, false)
  const changes = c.confirmChanges([{ id: 'x', before: 'a', after: 'b' }])
  instance.dispose()
  assert.equal(await newer, false)
  assert.equal(await changes, false)
  assert.equal(c.leaveOpen.value, false)
  assert.equal(c.changesOpen.value, false)
})

test('tutorial example remains editable without open initialize validation or save IPC and restores the exact real draft', async t => {
  const demo = shallowRef(null), calls = { open: 0, initialize: 0, validate: 0, save: 0 }
  const { editor } = await setup(t, { demoSnapshot: () => demo.value, api: {
    diyChefOpen: async () => { calls.open++; return { status: 'ready', candidates: [], snapshot: snapshot() } },
    diyChefInitialize: async () => { calls.initialize++; return snapshot() },
    diyChefValidate: async () => { calls.validate++; return valid() },
    diyChefSave: async () => { calls.save++; return snapshot() },
  } })
  await editor.open()
  editor.toggle('fox')
  editor.changeHat('owl', { kind: 'known', hat_id: 'hat:None' })
  const realState = editor.state.value, realSnapshot = editor.snapshot.value
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  assert.equal(editor.snapshot.value.session_id, 'demo-session')
  editor.toggle('owl'); editor.setLobby(true)
  assert.equal(editor.editable.value, true)
  assert.equal(editor.dirty.value, false)
  assert.equal(editor.canSave.value, false)
  await editor.open(); assert.equal(await editor.save(), false)
  await new Promise(resolve => setTimeout(resolve, 310))
  assert.deepEqual(calls, { open: 1, initialize: 0, validate: 0, save: 0 })
  demo.value = null
  assert.strictEqual(editor.state.value, realState)
  assert.strictEqual(editor.snapshot.value, realSnapshot)
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.canSave.value, true)
})

test('a late real open cannot overwrite a tutorial and ending that tutorial starts a fresh real open', async t => {
  const demo = shallowRef(null), open = deferred()
  let opens = 0
  const { editor } = await setup(t, { demoSnapshot: () => demo.value, api: {
    diyChefOpen: () => { opens++; return opens === 1 ? open.promise : Promise.resolve({ status: 'ready', candidates: [], snapshot: snapshot() }) },
  } })
  const pending = editor.open()
  await tick()
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  open.resolve({ status: 'ready', candidates: [], snapshot: { ...snapshot(), session_id: 'obsolete-session' } })
  await pending
  assert.equal(editor.snapshot.value.session_id, 'demo-session')
  demo.value = null
  await tick()
  assert.equal(opens, 2)
  assert.equal(editor.snapshot.value.session_id, 'session')
})

test('tutorial interrupts save validation before it can authorize a disk write', async t => {
  const demo = shallowRef(null), checking = deferred()
  let saves = 0
  const { editor } = await setup(t, { demoSnapshot: () => demo.value, api: {
    diyChefValidate: () => checking.promise,
    diyChefSave: async () => { saves++; return snapshot() },
  } })
  await editor.open(); editor.toggle('fox')
  const saving = editor.save()
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  checking.resolve(valid())
  assert.equal(await saving, false)
  assert.equal(saves, 0)
  demo.value = null
  assert.equal(editor.dirty.value, true)
  assert.equal(editor.canSave.value, true)
})

test('a write already in flight before the tutorial cannot overwrite the demo and restored state requires rereading', async t => {
  const demo = shallowRef(null), write = deferred()
  let saves = 0
  const { editor } = await setup(t, { demoSnapshot: () => demo.value, api: {
    diyChefSave: () => { saves++; return write.promise },
  } })
  await editor.open(); editor.toggle('fox')
  const realState = editor.state.value
  const saving = editor.save()
  await tick(); assert.equal(saves, 1)
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  write.resolve({ ...snapshot(), session_id: 'saved-session' })
  assert.equal(await saving, false)
  assert.equal(editor.snapshot.value.session_id, 'demo-session')
  demo.value = null
  assert.strictEqual(editor.state.value, realState)
  assert.equal(editor.error.value, 'diychef.conflict')
  assert.equal(editor.canSave.value, false)
  assert.equal(editor.dirty.value, true)
})
