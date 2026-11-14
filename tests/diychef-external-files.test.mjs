import assert from 'node:assert/strict'
import test from 'node:test'
import { setup, snapshot, tick, deferred, shallowRef } from './fixtures/diychef-editor.mjs'

const change = (resources_changed = false, target_id = 'target', generation = 7) => ({ target_id, generation, resources_changed })
const opened = value => ({ status: 'ready', candidates: [value.target], snapshot: value })
const revised = (revision = 'external') => ({ ...snapshot(), session_id: `session-${revision}`, source: { ...snapshot().source, revision } })

test('external TXT imports through the real editor without DLL scan, initialization or whole page loading', async t => {
  let opens = 0, scans = 0, initializes = 0
  const reading = deferred()
  const { editor } = await setup(t, { api: {
    localMods: async () => { scans++; return { generation: 7, mods: [] } },
    diyChefInitialize: async () => { initializes++; return snapshot() },
    diyChefOpen: async (generation, target) => {
      if (++opens === 1) return opened(snapshot())
      assert.equal(generation, 7); assert.equal(target, 'target')
      return reading.promise
    },
  } })
  await editor.open()
  const beforeScans = scans
  globalThis.__chefFilesEvent(change())
  await tick()
  assert.equal(opens, 2); assert.equal(scans, beforeScans); assert.equal(initializes, 0)
  assert.equal(editor.loading.value, false); assert.equal(editor.editable.value, true)
  const external = revised()
  external.selection.entries.push({ ...external.selection.entries[0], entry_id: 'fox', chef_ref: { kind: 'known', chef_id: 'fox' } })
  reading.resolve(opened(external)); await tick()
  assert.equal(editor.snapshot.value.session_id, external.session_id)
  assert.deepEqual(editor.state.value.draft.entries.map(entry => entry.chef_ref.chef_id), ['cat', 'fox'])
  assert.equal(editor.dirty.value, false)
})

test('external edits preserve a dirty draft and undo, report conflict, and block stale saves', async t => {
  const { editor } = await setup(t)
  await editor.open()
  editor.changeHat('owl', { kind: 'known', hat_id: 'hat:None' })
  const oldSnapshot = editor.snapshot.value, oldState = editor.state.value
  globalThis.__chefApi.diyChefOpen = async () => opened(revised())
  globalThis.__chefFilesEvent(change()); assert.equal(editor.canSave.value, false)
  await tick()
  assert.equal(editor.error.value, 'diychef.conflict')
  assert.strictEqual(editor.snapshot.value, oldSnapshot); assert.strictEqual(editor.state.value, oldState)
  assert.equal(editor.canSave.value, false); assert.equal(editor.canUndo.value, true)
  editor.undo(); assert.equal(editor.dirty.value, false)
})

test('an edit created while external read is in flight is preserved too', async t => {
  const reading = deferred()
  const { editor } = await setup(t)
  await editor.open()
  globalThis.__chefApi.diyChefOpen = () => reading.promise
  globalThis.__chefFilesEvent(change()); await tick()
  editor.toggle('fox')
  const edited = editor.state.value
  assert.equal(editor.canSave.value, false)
  reading.resolve(opened(revised())); await tick()
  assert.strictEqual(editor.state.value, edited)
  assert.equal(editor.error.value, 'diychef.conflict')
})

test('save echoes wait for write completion then compare revisions without replacing the editor session', async t => {
  const writing = deferred()
  const { editor } = await setup(t, { api: { diyChefSave: () => writing.promise } })
  await editor.open(); editor.toggle('fox')
  const saved = revised('saved')
  saved.selection.entries = structuredClone(editor.state.value.draft.entries)
  let reads = 0
  globalThis.__chefApi.diyChefOpen = async () => { reads++; return opened({ ...saved, session_id: 'echo-read' }) }
  const saving = editor.save(); await tick()
  globalThis.__chefFilesEvent(change()); await tick()
  assert.equal(reads, 0)
  writing.resolve(saved); assert.equal(await saving, true); await tick()
  assert.equal(reads, 1); assert.equal(editor.error.value, '')
  assert.equal(editor.snapshot.value.session_id, 'session-saved')
  assert.equal(editor.dirty.value, false)
})

test('dragging, marquee and dialogs defer and merge notifications, including resources-only changes', async t => {
  const interacting = shallowRef(false)
  const { editor } = await setup(t, { interactionBusy: () => interacting.value })
  await editor.open()
  let reads = 0
  globalThis.__chefApi.diyChefOpen = async () => { reads++; return opened({ ...snapshot(), session_id: 'resources-reloaded' }) }
  interacting.value = true
  globalThis.__chefFilesEvent(change(true)); globalThis.__chefFilesEvent(change(false)); await tick()
  assert.equal(reads, 0)
  interacting.value = false; await tick()
  assert.equal(reads, 1)
  assert.equal(editor.snapshot.value.session_id, 'resources-reloaded', 'equal semantic revisions still refresh resource previews')
})

test('an interaction starting during a read postpones snapshot publication until it finishes', async t => {
  const interacting = shallowRef(false), reading = deferred()
  const { editor } = await setup(t, { interactionBusy: () => interacting.value })
  await editor.open()
  const original = editor.snapshot.value
  let reads = 0
  globalThis.__chefApi.diyChefOpen = () => ++reads === 1 ? reading.promise : Promise.resolve(opened(revised()))
  globalThis.__chefFilesEvent(change()); await tick()
  interacting.value = true; reading.resolve(opened(revised())); await tick()
  assert.strictEqual(editor.snapshot.value, original); assert.equal(reads, 1)
  interacting.value = false; await tick()
  assert.equal(reads, 2); assert.equal(editor.snapshot.value.source.revision, 'external')
})

test('events for old target or scan generations are ignored', async t => {
  const { editor } = await setup(t)
  await editor.open()
  let reads = 0
  globalThis.__chefApi.diyChefOpen = async () => { reads++; return opened(revised()) }
  globalThis.__chefFilesEvent(change(true, 'other'))
  globalThis.__chefFilesEvent(change(true, 'target', 6)); await tick()
  assert.equal(reads, 0); assert.equal(editor.snapshot.value.source.revision, 'initial')
})

test('switching target waits for the old backend read then discards its result before opening the new target', async t => {
  const reading = deferred()
  const { editor } = await setup(t)
  await editor.open()
  const next = revised('next-target')
  next.target = { ...next.target, id: 'other-target' }
  let reads = 0
  globalThis.__chefApi.diyChefOpen = (_generation, target) => {
    reads++
    return target === 'other-target' ? Promise.resolve(opened(next)) : reading.promise
  }
  globalThis.__chefFilesEvent(change()); await tick()
  const switching = editor.open('other-target'); await tick()
  assert.equal(reads, 1, 'new open cannot register its watcher ahead of an old in-flight open')
  reading.resolve(opened(revised('old-response'))); await switching; await tick()
  assert.equal(reads, 2)
  assert.strictEqual(editor.snapshot.value, next)
  assert.equal(editor.error.value, '')
})

test('tutorial defers real filesystem events and restores then reads the real session', async t => {
  const demo = shallowRef(null)
  const { editor } = await setup(t, { demoSnapshot: () => demo.value })
  await editor.open()
  let reads = 0
  globalThis.__chefApi.diyChefOpen = async () => { reads++; return opened(revised()) }
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  globalThis.__chefFilesEvent(change(true)); await tick()
  assert.equal(reads, 0); assert.equal(editor.snapshot.value.session_id, 'demo-session')
  demo.value = null; await tick()
  assert.equal(reads, 1); assert.equal(editor.snapshot.value.session_id, 'session-external')
})

test('an external read spanning a complete tutorial is retried once for the restored real target', async t => {
  const demo = shallowRef(null), reading = deferred()
  const { editor } = await setup(t, { demoSnapshot: () => demo.value })
  await editor.open()
  let reads = 0
  globalThis.__chefApi.diyChefOpen = () => ++reads === 1 ? reading.promise : Promise.resolve(opened(revised('latest')))
  globalThis.__chefFilesEvent(change(true)); await tick()
  assert.equal(reads, 1)
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  demo.value = null; await tick()
  assert.equal(editor.snapshot.value.session_id, 'session'); assert.equal(reads, 1)
  reading.resolve(opened(revised('obsolete'))); await tick()
  assert.equal(reads, 2)
  assert.equal(editor.snapshot.value.session_id, 'session-latest')
  assert.equal(editor.error.value, ''); assert.equal(editor.dirty.value, false)
  await tick(); assert.equal(reads, 2, 'one obsolete response cannot keep scheduling reads')
})

test('a tutorial-spanning read retries without replacing the restored dirty draft or undo', async t => {
  const demo = shallowRef(null), reading = deferred()
  const { editor } = await setup(t, { demoSnapshot: () => demo.value })
  await editor.open(); editor.toggle('fox')
  const dirtyState = editor.state.value
  let reads = 0
  globalThis.__chefApi.diyChefOpen = () => ++reads === 1 ? reading.promise : Promise.resolve(opened(revised('latest')))
  globalThis.__chefFilesEvent(change()); await tick()
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  demo.value = null; await tick()
  reading.resolve(opened(revised('obsolete'))); await tick()
  assert.equal(reads, 2)
  assert.strictEqual(editor.state.value, dirtyState)
  assert.equal(editor.snapshot.value.session_id, 'session')
  assert.equal(editor.error.value, 'diychef.conflict'); assert.equal(editor.canSave.value, false)
  assert.equal(editor.canUndo.value, true)
  editor.undo(); assert.equal(editor.dirty.value, false)
})

test('a tutorial-spanning old read cannot queue another read for a newly selected target', async t => {
  const demo = shallowRef(null), reading = deferred()
  const { editor } = await setup(t, { demoSnapshot: () => demo.value })
  await editor.open()
  const next = revised('next-target')
  next.target = { ...next.target, id: 'other-target' }
  const targets = []
  globalThis.__chefApi.diyChefOpen = (_generation, target) => {
    targets.push(target)
    return target === 'other-target' ? Promise.resolve(opened(next)) : reading.promise
  }
  globalThis.__chefFilesEvent(change(true)); await tick()
  demo.value = { ...snapshot(), session_id: 'demo-session' }
  demo.value = null; await tick()
  const switching = editor.open('other-target'); await tick()
  reading.resolve(opened(revised('obsolete'))); await switching; await tick()
  assert.deepEqual(targets, ['target', 'other-target'])
  assert.strictEqual(editor.snapshot.value, next); assert.equal(editor.error.value, '')
  await tick(); assert.deepEqual(targets, ['target', 'other-target'])
})

test('unmount releases the event listener and discards an in-flight external response', async t => {
  const reading = deferred()
  const { editor, dispose } = await setup(t)
  await editor.open()
  const original = editor.snapshot.value
  globalThis.__chefApi.diyChefOpen = () => reading.promise
  globalThis.__chefFilesEvent(change()); await tick(); dispose()
  assert.equal(globalThis.__chefFilesStopped, 1)
  reading.resolve(opened(revised())); await tick()
  assert.strictEqual(editor.snapshot.value, original)
})

test('open waits for event subscription and late registration is released after unmount', async t => {
  const listening = deferred()
  let reads = 0, stopped = 0
  const { editor, dispose } = await setup(t, { filesSubscribe: () => listening.promise,
    api: { diyChefOpen: async () => { reads++; return opened(snapshot()) } } })
  const opening = editor.open(); await tick()
  assert.equal(reads, 0)
  dispose(); listening.resolve(() => { stopped++ }); await opening
  assert.equal(reads, 0); assert.equal(stopped, 1)
})

test('a notification between backend watch registration and initial read completion is retained', async t => {
  const initial = deferred()
  let reads = 0
  const { editor } = await setup(t, { api: { diyChefOpen: () => ++reads === 1 ? initial.promise : Promise.resolve(opened(revised())) } })
  const opening = editor.open(); await tick()
  globalThis.__chefFilesEvent(change()); await tick(); assert.equal(reads, 1)
  initial.resolve(opened(snapshot())); await opening; await tick()
  assert.equal(reads, 2); assert.equal(editor.snapshot.value.source.revision, 'external')
})

test('initial missing initialization resumes once after a pending resource read still finds no TXT', async t => {
  const initial = deferred(), reading = deferred(), initializations = []
  let reads = 0
  const { editor } = await setup(t, { missing: true, api: {
    diyChefOpen: () => ++reads === 1 ? initial.promise : reading.promise,
    diyChefInitialize: async (...args) => { initializations.push(args); return revised('initialized') },
  } })
  const opening = editor.open(); await tick()
  globalThis.__chefFilesEvent(change(true))
  initial.resolve(opened(snapshot(true))); await opening; await tick()
  assert.equal(reads, 2); assert.equal(initializations.length, 0)
  assert.equal(editor.loading.value, false)
  reading.resolve(opened({ ...snapshot(true), session_id: 'latest-missing' })); await tick()
  assert.deepEqual(initializations, [['latest-missing', 'missing']])
  assert.equal(editor.snapshot.value.source.state, 'present')
  assert.equal(editor.snapshot.value.source.revision, 'initialized')
  assert.equal(editor.dirty.value, false)
  await tick(); assert.equal(initializations.length, 1, 'only the deferred existing initialization is resumed')
})

test('deferred missing initialization still waits if the game starts during the resource read', async t => {
  const initial = deferred(), reading = deferred()
  let reads = 0, initializes = 0
  const { editor } = await setup(t, { missing: true, api: {
    diyChefOpen: () => ++reads === 1 ? initial.promise : reading.promise,
    diyChefInitialize: async () => { initializes++; return revised('initialized') },
  } })
  const opening = editor.open(); await tick()
  globalThis.__chefFilesEvent(change(true))
  initial.resolve(opened(snapshot(true))); await opening; await tick()
  globalThis.__chefGameEvent({ path: 'fixture', kitchen_running: true }); await tick()
  reading.resolve(opened(snapshot(true))); await tick()
  assert.equal(initializes, 0); assert.equal(editor.snapshot.value.source.state, 'missing')
  globalThis.__chefGameEvent({ path: 'fixture', kitchen_running: false }); await tick()
  assert.equal(initializes, 1); assert.equal(editor.snapshot.value.source.state, 'present')
})

test('a dirty draft created during the deferred initial resource read is never initialized or replaced', async t => {
  const initial = deferred(), reading = deferred()
  let reads = 0, initializes = 0
  const { editor } = await setup(t, { missing: true, api: {
    diyChefOpen: () => ++reads === 1 ? initial.promise : reading.promise,
    diyChefInitialize: async () => { initializes++; return revised('initialized') },
  } })
  const opening = editor.open(); await tick()
  globalThis.__chefFilesEvent(change(true))
  initial.resolve(opened(snapshot(true))); await opening; await tick()
  editor.toggle('fox'); const draft = editor.state.value
  reading.resolve(opened({ ...snapshot(true), session_id: 'latest-missing' })); await tick()
  assert.equal(initializes, 0); assert.strictEqual(editor.state.value, draft)
  assert.equal(editor.snapshot.value.source.state, 'missing')
  assert.equal(editor.error.value, 'diychef.conflict'); assert.equal(editor.canUndo.value, true)
  editor.undo(); await tick()
  assert.equal(editor.dirty.value, false); assert.equal(initializes, 0, 'undo cannot bypass the stale baseline conflict')
})

test('a present initial resource read cancels deferred initialization before a later external deletion', async t => {
  const initial = deferred()
  let reads = 0, initializes = 0
  const { editor } = await setup(t, { missing: true, api: {
    diyChefOpen: () => ++reads === 1 ? initial.promise : Promise.resolve(opened(revised('created-externally'))),
    diyChefInitialize: async () => { initializes++; return revised('initialized') },
  } })
  const opening = editor.open(); await tick()
  globalThis.__chefFilesEvent(change(true))
  initial.resolve(opened(snapshot(true))); await opening; await tick()
  assert.equal(initializes, 0); assert.equal(editor.snapshot.value.source.state, 'present')
  globalThis.__chefApi.diyChefOpen = async () => opened(snapshot(true))
  globalThis.__chefFilesEvent(change()); await tick()
  assert.equal(editor.snapshot.value.source.state, 'missing')
  assert.equal(initializes, 0, 'a later external deletion is a read-only event, not another initial open')
})

test('deferred initialization cannot write if the reread target is read-only', async t => {
  const initial = deferred()
  let reads = 0, initializes = 0
  const blocked = snapshot(true)
  blocked.target.compatibility = 'read_only'
  const { editor } = await setup(t, { missing: true, api: {
    diyChefOpen: () => ++reads === 1 ? initial.promise : Promise.resolve(opened(blocked)),
    diyChefInitialize: async () => { initializes++; return revised('initialized') },
  } })
  const opening = editor.open(); await tick()
  globalThis.__chefFilesEvent(change(true))
  initial.resolve(opened(snapshot(true))); await opening; await tick()
  assert.equal(editor.readOnly.value, true); assert.equal(initializes, 0)
  assert.equal(editor.snapshot.value.source.state, 'missing')
})

for (const external of [false, true]) test(`a new editor waits for an unmounted editor's ${external ? 'external' : 'initial'} read before registering its target`, async t => {
  const reading = deferred(), calls = []
  t.after(() => reading.resolve(opened(snapshot())))
  const previous = await setup(t)
  if (external) await previous.editor.open()
  globalThis.__chefApi.diyChefOpen = async (_generation, target) => {
    calls.push(`start:${target}`)
    const result = await reading.promise
    calls.push(`watch:${target}`)
    return result
  }
  let openingPrevious
  if (external) globalThis.__chefFilesEvent(change(true))
  else openingPrevious = previous.editor.open('target')
  await tick(); assert.deepEqual(calls, ['start:target'])
  const previousSnapshot = previous.editor.snapshot.value
  previous.dispose()
  const next = revised('new-editor'); next.target = { ...next.target, id: 'other-target' }
  const current = await setup(t, { api: { diyChefOpen: async (_generation, target) => {
    calls.push(`start:${target}`, `watch:${target}`)
    return opened(next)
  } } })
  const openingCurrent = current.editor.open('other-target'); await tick()
  assert.deepEqual(calls, ['start:target'], 'the old backend request still owns registration order after unmount')
  reading.resolve(opened(revised('obsolete'))); await openingPrevious; await openingCurrent; await tick()
  assert.deepEqual(calls, ['start:target', 'watch:target', 'start:other-target', 'watch:other-target'])
  assert.strictEqual(current.editor.snapshot.value, next)
  assert.strictEqual(previous.editor.snapshot.value, previousSnapshot)
})

test('a disposed queued editor sends no IPC and an old failed read still releases the next editor', async t => {
  const reading = deferred(), calls = []
  t.after(() => reading.resolve(opened(snapshot())))
  const first = await setup(t, { api: { diyChefOpen: (_generation, target) => {
    calls.push(target); return reading.promise
  } } })
  const openingFirst = first.editor.open('first-target'); await tick(); first.dispose()
  const second = await setup(t, { api: { diyChefOpen: async (_generation, target) => {
    calls.push(target); return opened(snapshot())
  } } })
  const openingSecond = second.editor.open('cancelled-target'); await tick(); second.dispose()
  const latest = revised('latest-editor'); latest.target = { ...latest.target, id: 'latest-target' }
  const third = await setup(t, { api: { diyChefOpen: async (_generation, target) => {
    calls.push(target); return opened(latest)
  } } })
  const openingThird = third.editor.open('latest-target'); await tick()
  assert.deepEqual(calls, ['first-target'])
  reading.reject(new Error('old backend read failed'))
  await Promise.all([openingFirst, openingSecond, openingThird]); await tick()
  assert.deepEqual(calls, ['first-target', 'latest-target'])
  assert.strictEqual(third.editor.snapshot.value, latest)
  assert.equal(second.editor.snapshot.value, null); assert.equal(third.editor.error.value, '')
})
