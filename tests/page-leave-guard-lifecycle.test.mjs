import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { createRenderer, h, nextTick, shallowRef } from 'vue'
import { createMemoryHistory, createRouter, isNavigationFailure, RouterView } from 'vue-router'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier === '@tauri-apps/api/window') return { url: 'oc2-test:native-window', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* standard resolution below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url === 'oc2-test:native-window') return { format: 'module', shortCircuit: true, source: 'export const getCurrentWindow = () => globalThis.__guardNative' }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true, source: 'export const useNotify = () => ({error: text => globalThis.__guardNotices.push(text)})' }
    if (url.endsWith('/src/locale/backend.ts')) return { format: 'module', shortCircuit: true, source: 'export const backendMessageOf = reason => reason instanceof Error ? reason.message : String(reason)' }
    if (url.endsWith('/src/modChanges/store.ts')) return {format:'module',shortCircuit:true,source:'export const useModChanges=()=>({state:{get value(){return globalThis.__guardChangeState}}})'}
    if (url.endsWith('/src/services/tauri.ts')) return { format: 'module', shortCircuit: true, source: 'export const api = {exitApp: () => globalThis.__guardExit(), applyStagedAppUpdate: () => globalThis.__guardApplyUpdate()}' }
    return next(url, context)
  },
})
const { usePageLeaveGuard } = await import('../src/composables/usePageLeaveGuard.ts')
const { useAppCloseGuard, registerPageLeaveGuard } = await import('../src/composables/useAppCloseGuard.ts')
const { appUpdateState, requireAppUpdate, beginBackgroundAppUpdate, finishBackgroundAppUpdate, beginStagedAppUpdate, finishStagedAppUpdate } = await import('../src/appUpdateState.ts')
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
const deferred = () => { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j }); return { promise, resolve, reject } }
const host = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
globalThis.window = new EventTarget()
window.__TAURI_INTERNALS__ = { metadata: {} }

async function setup(t, { dirty = true, busy = false, confirm = async () => false, registration, exit, apply, guarded = true } = {}) {
  globalThis.__guardChangeState = null
  const state = { dirty: shallowRef(dirty), busy: shallowRef(busy), confirms: 0, cancels: 0, stops: 0, closes: 0, exits: 0, applies: 0, subscriptions: 0 }
  const listeners = new Set()
  globalThis.__guardNotices = []
  const native = {
    async onCloseRequested(callback) {
      state.subscriptions++
      if (registration) await registration.promise
      listeners.add(callback)
      return () => { state.stops++; listeners.delete(callback) }
    },
    async close() {
      state.closes++
      assert.fail('native close must remain prevented until exitApp completes map recovery')
    },
  }
  globalThis.__guardNative = native
  globalThis.__guardExit = async () => {
    state.exits++
    assert.equal(listeners.size, 1, 'the native listener must remain registered during recovery and on failure')
    if (exit) await exit()
  }
  globalThis.__guardApplyUpdate = async () => {
    state.applies++
    await apply?.()
  }
  let guard
  const page = { setup() {
    if (guarded) guard = usePageLeaveGuard({ busy: state.busy, dirty: state.dirty,
      confirm: () => { state.confirms++; return confirm() }, cancelConfirm: () => { state.cancels++ } })
    return () => null
  } }
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: page }, { path: '/away', component: { render: () => null } }] })
  await router.push('/')
  await router.isReady()
  let appGuard
  const app = host.createApp({ setup() { appGuard = useAppCloseGuard(); return () => h(RouterView) } }).use(router)
  app.mount({})
  let disposed = false
  const dispose = () => { if (!disposed) { disposed = true; app.unmount() } }
  t.after(dispose)
  await tick()
  return { get guard() { return guard }, applyUpdate: appGuard.applyUpdate, router, state, dispose, listeners,
    emitClose() {
      let prevented = 0
      const pending = Promise.all([...listeners].map(callback => callback({ preventDefault() { prevented++ } })))
      return { pending, get prevented() { return prevented } }
    },
  }
}

test('the App listener survives route changes and ordinary pages also recover before exit', async t => {
  const fixture = await setup(t, { dirty: false })
  const { guard, router, state, listeners } = fixture
  assert.equal(await guard.requestLeave(), true)
  await router.push('/away')
  await tick()
  assert.equal(router.currentRoute.value.path, '/away')
  assert.equal(state.confirms, 0)
  assert.equal(listeners.size, 1)
  assert.equal(state.stops, 0)
  assert.equal(await guard.requestLeave(), false, 'the former page guard was disposed')
  await fixture.emitClose().pending
  assert.equal(state.exits, 1, 'routes without a page guard must still use exitApp')
  await router.push('/')
  await router.push('/away')
  assert.equal(state.subscriptions, 1, 'route changes never register another native listener')
  fixture.dispose()
  assert.equal(state.stops, 1)
  assert.equal(listeners.size, 0)
})

test('busy pages reject direct, route and native-close requests, including a save starting during confirmation', async t => {
  const choice = deferred()
  const fixture = await setup(t, { dirty: false, busy: true, confirm: () => choice.promise })
  const { guard, router, state } = fixture
  assert.equal(await guard.requestLeave(), false)
  assert.ok(isNavigationFailure(await router.push('/away')))
  const closing = fixture.emitClose()
  assert.equal(closing.prevented, 1)
  await closing.pending
  assert.equal(state.closes, 0)
  assert.equal(state.exits, 0)
  assert.equal(state.confirms, 0)
  assert.equal(router.currentRoute.value.path, '/')
  const reload = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(reload)
  assert.equal(reload.defaultPrevented, true)
  state.busy.value = false
  state.dirty.value = true
  const pending = fixture.emitClose()
  await tick()
  state.busy.value = true
  choice.resolve(true)
  await pending.pending
  assert.equal(state.confirms, 1)
  assert.equal(state.exits, 0, 'a newly busy page invalidates an accepted confirmation')
})

test('route, repeated direct requests and native close share one dirty confirmation; cancellation keeps the page', async t => {
  const choice = deferred()
  const fixture = await setup(t, { confirm: () => choice.promise })
  const navigation = fixture.router.push('/away')
  await tick()
  const first = fixture.guard.requestLeave(), second = fixture.guard.requestLeave()
  assert.strictEqual(first, second)
  const closing = fixture.emitClose()
  assert.equal(closing.prevented, 1)
  assert.equal(fixture.state.confirms, 1)
  choice.resolve(false)
  assert.equal(await first, false)
  assert.ok(isNavigationFailure(await navigation))
  await closing.pending
  assert.equal(fixture.guard.confirming.value, false)
  assert.equal(fixture.router.currentRoute.value.path, '/')
  assert.equal(fixture.state.closes, 0)
  assert.equal(fixture.state.exits, 0)
})

test('an accepted route confirmation navigates and cancels page-owned dialog state on disposal', async t => {
  const fixture = await setup(t, { confirm: async () => true })
  await fixture.router.push('/away')
  await tick()
  assert.equal(fixture.router.currentRoute.value.path, '/away')
  assert.equal(fixture.state.confirms, 1)
  assert.equal(fixture.state.cancels, 1)
  assert.equal(await fixture.guard.requestLeave(), false)
  assert.equal(fixture.listeners.size, 1)
  assert.equal(fixture.state.exits, 0, 'route confirmation never exits the application')
  const reload = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(reload)
  assert.equal(reload.defaultPrevented, false, 'reload protection belongs to the page lifetime')
})

test('accepted native close shares confirmation and a single exitApp while retaining its listener', async t => {
  const recovery = deferred()
  const fixture = await setup(t, { confirm: async () => true, exit: () => recovery.promise })
  const first = fixture.emitClose(), duplicate = fixture.emitClose()
  await tick()
  assert.equal(first.prevented, 1)
  assert.equal(duplicate.prevented, 1)
  assert.equal(fixture.state.confirms, 1)
  assert.equal(fixture.state.exits, 1)
  assert.equal(fixture.state.closes, 0)
  assert.equal(fixture.listeners.size, 1)
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 1, 'repeated close while backend recovery runs is ignored')
  recovery.resolve()
  await Promise.all([first.pending, duplicate.pending])
  assert.equal(fixture.state.subscriptions, 1)
})

test('failed map recovery preserves the window and listener; the next close can retry', async t => {
  let attempts = 0
  const fixture = await setup(t, { dirty: false, exit: async () => { if (++attempts === 1) throw new Error('assetBundle.recoveryPending') } })
  await fixture.emitClose().pending
  assert.equal(fixture.state.closes, 0)
  assert.equal(fixture.state.exits, 1)
  assert.equal(fixture.state.subscriptions, 1)
  assert.equal(fixture.listeners.size, 1)
  assert.deepEqual(globalThis.__guardNotices, ['assetBundle.recoveryPending'])
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 2)
  assert.equal(fixture.state.subscriptions, 1)
  assert.equal(fixture.listeners.size, 1)
  assert.deepEqual(globalThis.__guardNotices, ['assetBundle.recoveryPending'])
})

test('unmount while listener registration is pending cleans the late native subscription', async t => {
  const registration = deferred()
  const fixture = await setup(t, { registration })
  assert.equal(fixture.listeners.size, 0)
  fixture.dispose()
  registration.resolve()
  await tick()
  assert.equal(fixture.listeners.size, 0)
  assert.equal(fixture.state.stops, 1)
  assert.equal(fixture.state.cancels, 1)
  assert.equal(fixture.state.exits, 0)
})

test('unmount before the confirmation microtask prevents any late page-owned confirmation', async t => {
  const fixture = await setup(t, { confirm: async () => true })
  const pending = fixture.guard.requestLeave()
  fixture.dispose()
  assert.equal(await pending, false)
  await tick()
  assert.equal(fixture.state.confirms, 0, 'disposed pages must not create a new dialog or run cleanup policy')
  assert.equal(fixture.guard.confirming.value, false)
})

test('page replacement invalidates native confirmation; App disposal settles it and ignores late failure', async t => {
  let choice = deferred()
  const fixture = await setup(t, { confirm: () => choice.promise })
  const closing = fixture.emitClose()
  await tick()
  assert.equal(fixture.state.confirms, 1)
  const removeOlder = registerPageLeaveGuard({ requestLeave: async () => true, blocked: () => false })
  let newerRequests = 0
  const removeNewer = registerPageLeaveGuard({ requestLeave: async () => { newerRequests++; return false }, blocked: () => false })
  removeOlder()
  choice.resolve(true)
  await closing.pending
  assert.equal(fixture.state.exits, 0, 'old confirmation must not authorize closing a newly active page')
  await fixture.emitClose().pending
  assert.equal(newerRequests, 1, 'removing an older registration preserves the newest page guard')
  removeNewer()
  choice = deferred()
  const pending = fixture.guard.requestLeave()
  const lateClose = fixture.emitClose()
  await tick()
  fixture.dispose()
  assert.equal(await pending, false)
  choice.reject(new Error('late-confirmation-failure'))
  await lateClose.pending
  await tick()
  assert.deepEqual(globalThis.__guardNotices, [])
  assert.equal(fixture.state.cancels, 1)
  assert.equal(fixture.listeners.size, 0)
})

test('batch preparation, displaying completion and execution block native close until the apply review',async t=>{
 const fixture=await setup(t,{dirty:false})
 for(const phase of ['preparing','prepared','executing']){globalThis.__guardChangeState={phase};await fixture.emitClose().pending;assert.equal(fixture.state.exits,0,phase)}
 globalThis.__guardChangeState={phase:'reconfirming'};await fixture.emitClose().pending;assert.equal(fixture.state.exits,1)
 globalThis.__guardChangeState=null
})

test('background downloads prevent close and ready apply shares dirty confirmation and busy gates', async t => {
  const choice = deferred()
  const handoff = deferred()
  const fixture = await setup(t, { confirm: () => choice.promise, apply: () => handoff.promise })
  requireAppUpdate({ version: 'test', background_install: true, source_channel: 'fixture' })
  assert.equal(beginBackgroundAppUpdate(), 'fixture')
  await fixture.emitClose().pending
  await fixture.applyUpdate()
  assert.equal(fixture.state.confirms, 0)
  finishBackgroundAppUpdate()
  assert.equal(appUpdateState.stage, 'ready')
  globalThis.__guardChangeState = { phase: 'executing' }
  await fixture.applyUpdate()
  assert.equal(fixture.state.confirms, 0)
  globalThis.__guardChangeState = null
  const cancelled = fixture.applyUpdate()
  await tick()
  await fixture.emitClose().pending
  assert.equal(fixture.state.confirms, 1)
  choice.resolve(false)
  await cancelled
  assert.equal(fixture.state.exits, 0)
  assert.equal(fixture.state.applies, 0)
  fixture.dispose()
  const ready = await setup(t, { confirm: async () => true, apply: () => handoff.promise })
  const applying = ready.applyUpdate()
  await tick()
  assert.equal(appUpdateState.stage, 'applying')
  await ready.emitClose().pending
  await ready.applyUpdate()
  assert.equal(ready.state.exits, 0)
  assert.equal(ready.state.applies, 1)
  handoff.reject('update.stagedUnavailable')
  await applying
  assert.equal(appUpdateState.stage, 'stageFailed')
  ready.dispose()

  const closeResult = deferred()
  const closingReady = await setup(t, { dirty: false, exit: () => closeResult.promise })
  assert.equal(beginBackgroundAppUpdate(), 'fixture')
  finishBackgroundAppUpdate()
  const closingWithUpdate = closingReady.emitClose()
  await tick()
  assert.equal(appUpdateState.stage, 'applying')
  await closingReady.applyUpdate()
  await closingReady.emitClose().pending
  assert.equal(closingReady.state.exits, 1)
  assert.equal(closingReady.state.applies, 0, 'ordinary close does not request a restart')
  closeResult.reject(new Error('update.stagedUnavailable'))
  await closingWithUpdate.pending
  assert.equal(appUpdateState.stage, 'stageFailed')
  assert.equal(appUpdateState.error, 'update.stagedUnavailable')
  assert.equal(closingReady.listeners.size, 1)
  closingReady.dispose()

  const closeBlocked = await setup(t, { dirty: false, exit: async () => { throw 'assetBundle.recoveryPending' } })
  assert.equal(beginBackgroundAppUpdate(), 'fixture')
  finishBackgroundAppUpdate()
  await closeBlocked.emitClose().pending
  assert.equal(appUpdateState.stage, 'ready', 'other failure keeps the verified package')
  assert.deepEqual(globalThis.__guardNotices, ['assetBundle.recoveryPending'])
  closeBlocked.dispose()

  const pendingChoice = deferred()
  const duringConfirmation = await setup(t, { confirm: () => pendingChoice.promise })
  const pending = duringConfirmation.guard.requestLeave()
  await tick()
  assert.equal(duringConfirmation.state.confirms, 1)
  assert.equal(beginStagedAppUpdate(), true)
  assert.equal(await duringConfirmation.guard.requestLeave(), false)
  assert.ok(isNavigationFailure(await duringConfirmation.router.push('/away')))
  pendingChoice.resolve(true)
  assert.equal(await pending, false)
  await duringConfirmation.emitClose().pending
  assert.equal(duringConfirmation.state.exits, 0)
  assert.equal(duringConfirmation.router.currentRoute.value.path, '/')
  finishStagedAppUpdate('update.inProgress')
  duringConfirmation.dispose()

  const normalHandoff = await setup(t, { dirty: false })
  await normalHandoff.emitClose().pending
  assert.equal(normalHandoff.state.exits, 1)
  assert.equal(normalHandoff.state.applies, 0)
  assert.equal(appUpdateState.stage, 'handoff')
  normalHandoff.dispose()

  const ordinary = await setup(t, { guarded: false })
  const closing = ordinary.emitClose()
  assert.equal(closing.prevented, 1)
  await closing.pending
  assert.equal(ordinary.state.exits, 0, 'the update gate applies even without a page guard')
})
