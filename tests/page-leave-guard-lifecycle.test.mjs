import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { createRenderer, h, nextTick, shallowRef, toRaw } from 'vue'
import { createMemoryHistory, createRouter, isNavigationFailure, RouterView } from 'vue-router'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'oc2-test:dialog-list', shortCircuit: true }
    if (specifier === '@tauri-apps/api/window') return { url: 'oc2-test:native-window', shortCircuit: true }
    if (specifier === '@tauri-apps/api/event') return { url: 'oc2-test:native-event', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* standard resolution below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url === 'oc2-test:dialog-list') return { format: 'module', shortCircuit: true, source: 'export const useDialogReactiveList = () => ({value:[]})' }
    if (url === 'oc2-test:native-window') return { format: 'module', shortCircuit: true, source: 'export const getCurrentWindow = () => globalThis.__guardNative' }
    if (url === 'oc2-test:native-event') return { format: 'module', shortCircuit: true, source: 'export const listen = async (event, handler) => { const events = globalThis.__guardEvents; events.set(event, handler); return () => events.delete(event) }' }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true, source: 'export const useNotify = () => ({error: text => globalThis.__guardNotices.push(text)})' }
    if (url.endsWith('/src/locale/backend.ts')) return { format: 'module', shortCircuit: true, source: `
      export const backendMessageOf = reason => reason instanceof Error ? reason.message : String(reason)
      export const backendPayloadOf = reason => ({tag: backendMessageOf(reason).split('?')[0]})
    ` }
    if (url.endsWith('/src/modChanges/store.ts')) return {format:'module',shortCircuit:true,source:'export const useModChanges=()=>({state:globalThis.__guardChangeState})'}
    if (url.endsWith('/src/services/tauri.ts')) return { format: 'module', shortCircuit: true, source: `
      export const api = {
        exitApp: () => globalThis.__guardExit(),
        applyStagedAppUpdate: () => globalThis.__guardApplyUpdate(),
        openAppUpdateReleaseAndExit: (version, destination) => globalThis.__guardDownloadUpdate(version, destination),
        restartUpdateAsUser: () => globalThis.__guardRestartUpdate(),
      }
    ` }
    return next(url, context)
  },
})
const { usePageLeaveGuard } = await import('../src/composables/usePageLeaveGuard.ts')
const { useAppCloseGuard, registerPageLeaveGuard } = await import('../src/composables/useAppCloseGuard.ts')
const { useAppUpdatePromptOpen } = await import('../src/composables/useAppUpdates.ts')
const { appUpdateState, appUpdateBlocksOperations, beginAppUpdateCheck, beginDeferringAppUpdate, finishAppUpdateCheck,
  finishBackgroundAppUpdate, finishDeferringAppUpdate } = await import('../src/appUpdateState.ts')
const offer = { version: 'test', install_mode: 'deferred', source_channel: 'fixture' }
const offerResult = { update: offer, all_failed: false }
const resetUpdate = () => Object.assign(toRaw(appUpdateState), { stage: 'idle', error: '', update: null, deferred: false, notesRequested: false, resolvingExitBlock: false })
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
const deferred = () => { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j }); return { promise, resolve, reject } }
const host = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
globalThis.window = new EventTarget()
window.__TAURI_INTERNALS__ = { metadata: {} }

async function setup(t, { dirty = true, busy = false, confirm = async () => false, registration, exit, apply, download, restart, guarded = true } = {}) {
  const changeState = globalThis.__guardChangeState = shallowRef(null)
  const state = { dirty: shallowRef(dirty), busy: shallowRef(busy), confirms: 0, cancels: 0, stops: 0, closes: 0, exits: 0, applies: 0, subscriptions: 0 }
  const listeners = new Set()
  globalThis.__guardNotices = []
  const events = globalThis.__guardEvents = new Map()
  globalThis.__guardDownloadUpdate = download
  globalThis.__guardRestartUpdate = restart
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
  let updatePromptOpen
  const app = host.createApp({ setup() {
    appGuard = useAppCloseGuard()
    updatePromptOpen = useAppUpdatePromptOpen()
    return () => h(RouterView)
  } }).use(router)
  app.mount({})
  let disposed = false
  const dispose = () => { if (!disposed) { disposed = true; app.unmount() } }
  t.after(dispose)
  await tick()
  return { get guard() { return guard }, ...appGuard, router, state, dispose, listeners, events, changeState, updatePromptOpen,
    emitClose() {
      let prevented = 0
      const pending = Promise.all([...listeners].map(callback => callback({ preventDefault() { prevented++ } })))
      return { pending, get prevented() { return prevented } }
    },
  }
}

test('a required update waits for locked MOD preparation or execution before offering an exit', async t => {
  resetUpdate()
  t.after(resetUpdate)
  const downloads = []
  const fixture = await setup(t, { dirty: false, download: async version => { downloads.push(version) } })
  fixture.changeState.value = { phase: 'preparing' }
  beginAppUpdateCheck()
  finishAppUpdateCheck(offerResult)
  finishBackgroundAppUpdate({ error: 'update.verifyFailed' })
  for (const phase of ['preparing', 'prepared', 'executing']) {
    fixture.changeState.value = { phase }
    assert.equal(fixture.updatePromptOpen.value, false, phase)
    assert.equal(appUpdateBlocksOperations(), true, phase)
    await fixture.updateExit('download')
  }
  assert.deepEqual(downloads, [])
  fixture.changeState.value = { phase: 'failed' }
  assert.equal(fixture.updatePromptOpen.value, true)
  await fixture.updateExit('download')
  assert.deepEqual(downloads, [offer.version])
})

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
 for(const phase of ['preparing','prepared','executing']){fixture.changeState.value={phase};await fixture.emitClose().pending;assert.equal(fixture.state.exits,0,phase)}
 fixture.changeState.value={phase:'reconfirming'};await fixture.emitClose().pending;assert.equal(fixture.state.exits,1)
 fixture.changeState.value=null
})

test('downloads allow ordinary close; ready apply shares dirty confirmation and busy gates', async t => {
  t.after(resetUpdate)
  const choice = deferred()
  const handoff = deferred()
  const fixture = await setup(t, { dirty: false, apply: () => handoff.promise })
  assert.equal(beginAppUpdateCheck(), true)
  assert.equal(finishAppUpdateCheck(offerResult), 'fixture')
  await fixture.emitClose().pending
  await fixture.updateExit('apply')
  assert.equal(fixture.state.exits, 1)
  assert.equal(fixture.state.applies, 0)
  fixture.dispose()
  finishBackgroundAppUpdate({ update: offer })
  assert.equal(appUpdateState.stage, 'ready')
  const guarded = await setup(t, { confirm: () => choice.promise, apply: () => handoff.promise })
  guarded.changeState.value = { phase: 'executing' }
  await guarded.updateExit('apply')
  assert.equal(guarded.state.confirms, 0)
  guarded.changeState.value = null
  const cancelled = guarded.updateExit('apply')
  await tick()
  assert.equal(guarded.confirmingClose.value, true)
  assert.equal(appUpdateBlocksOperations(), false)
  await guarded.emitClose().pending
  assert.equal(guarded.state.confirms, 1)
  choice.resolve(false)
  await cancelled
  assert.equal(guarded.confirmingClose.value, false)
  assert.equal(appUpdateState.stage, 'ready', 'cancelling a draft confirmation keeps the ready update')
  assert.equal(appUpdateState.notesRequested, true)
  assert.equal(guarded.state.exits, 0)
  assert.equal(guarded.state.applies, 0)
  guarded.dispose()
  const ready = await setup(t, { confirm: async () => true, apply: () => handoff.promise })
  const applying = ready.updateExit('apply')
  await tick()
  assert.equal(appUpdateState.stage, 'applying')
  await ready.emitClose().pending
  await ready.updateExit('apply')
  assert.equal(ready.state.exits, 0)
  assert.equal(ready.state.applies, 1)
  handoff.reject('update.stagedUnavailable')
  await applying
  assert.equal(appUpdateState.stage, 'stageFailed')
  ready.dispose()

  const ordinary = await setup(t, { guarded: false })
  const closing = ordinary.emitClose()
  assert.equal(closing.prevented, 1)
  await closing.pending
  assert.equal(ordinary.state.exits, 0, 'the update gate applies even without a page guard')
})

test('system quit shares the draft guard, and its event listener is disposed', async t => {
  const fixture = await setup(t, { confirm: async () => true })
  fixture.events.get('app-quit-requested')()
  await tick()
  assert.equal(fixture.state.confirms, 1)
  assert.equal(fixture.state.exits, 1)
  fixture.dispose()
  assert.equal(fixture.events.size, 0)
})

test('only explicit elevated-update actions can exit; retries retain page confirmation and the native listener', async t => {
  t.after(resetUpdate)
  const browser = deferred()
  const downloads = []
  let restarts = 0
  let allowLeave = false
  const fixture = await setup(t, {
    confirm: async () => allowLeave,
    restart: async () => { restarts++; throw 'update.restartAsUserUnavailable?reason=Access%20denied' },
    download: version => { downloads.push(version); return downloads.length === 1 ? browser.promise : undefined },
  })
  assert.equal(beginAppUpdateCheck(), true)
  assert.equal(finishAppUpdateCheck({ ...offerResult, update: { ...offer, install_mode: 'restartAsUser' } }), undefined)
  await fixture.emitClose().pending
  fixture.events.get('app-quit-requested')()
  await tick()
  assert.equal(fixture.state.exits, 0)
  assert.ok(isNavigationFailure(await fixture.router.push('/away')))

  await fixture.updateExit('apply')
  assert.equal(restarts, 0, 'declining draft confirmation does not dispatch relaunch')
  assert.equal(appUpdateState.stage, 'ready')
  allowLeave = true
  await fixture.updateExit('apply')
  assert.equal(restarts, 1)
  assert.equal(appUpdateState.stage, 'stageFailed')
  assert.equal(appUpdateState.error, 'update.restartAsUserUnavailable?reason=Access%20denied')
  assert.deepEqual(globalThis.__guardNotices, [], 'action errors appear in the modal')
  await fixture.updateExit('apply')
  assert.equal(restarts, 1, 'unsafe relaunch is no longer available')
  assert.equal(fixture.state.applies, 0, 'no installer is invoked in the elevated window')

  const opening = fixture.updateExit('download')
  await tick()
  assert.equal(appUpdateState.stage, 'openingRelease')
  await fixture.updateExit('download')
  await fixture.emitClose().pending
  assert.deepEqual(downloads, [offer.version], 'one browser IPC for concurrent clicks')
  browser.reject(new Error('no browser'))
  await opening
  assert.equal(appUpdateState.stage, 'stageFailed')
  assert.equal(appUpdateState.error, 'no browser')
  assert.equal(fixture.listeners.size, 1)
  assert.equal(fixture.state.exits, 0)
  await fixture.updateExit('download')
  assert.equal(appUpdateState.stage, 'handoff')
  assert.deepEqual(downloads, [offer.version, offer.version])
})

test('local application failure leaves the version-bound manual destinations and permits retry through either', async t => {
  t.after(resetUpdate)
  const downloads = []
  const fixture = await setup(t, { dirty: false, apply: async () => { throw 'update.installFailedManual' },
    download: async (version, destination) => {
      downloads.push([version, destination])
      if (downloads.length === 1) throw 'no browser'
    },
  })
  beginAppUpdateCheck()
  finishAppUpdateCheck(offerResult)
  finishBackgroundAppUpdate({ update: offer })
  await fixture.updateExit('apply')
  assert.equal(appUpdateState.stage, 'stageFailed')
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 0)
  await fixture.updateExit('website')
  assert.equal(appUpdateState.stage, 'stageFailed')
  assert.equal(appUpdateState.error, 'no browser')
  await fixture.updateExit('download')
  assert.deepEqual(downloads, [[offer.version, 'website'], [offer.version, undefined]])
  assert.equal(appUpdateState.stage, 'handoff')
})

test('scheduled installation follows normal quit and can retry after a recovery block', async t => {
  t.after(resetUpdate)
  let failures = 0
  const fixture = await setup(t, { dirty: false, exit: async () => {
    assert.equal(appUpdateState.stage, 'applying')
    if (failures++ === 0) throw 'modChanges.recoveryRequired'
  } })
  beginAppUpdateCheck()
  finishAppUpdateCheck(offerResult)
  finishBackgroundAppUpdate({ update: offer })
  assert.equal(beginDeferringAppUpdate(), offer.version)
  finishDeferringAppUpdate()
  await fixture.emitClose().pending
  assert.equal(appUpdateState.stage, 'ready')
  assert.equal(appUpdateState.deferred, true)
  assert.equal(appUpdateState.resolvingExitBlock, true)
  assert.equal(appUpdateBlocksOperations(), false)
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 2)
  assert.equal(fixture.state.applies, 0)
  assert.equal(appUpdateState.stage, 'handoff')
})

test('a failed scheduled installation requires manual update and cannot close through the recovery detour', async t => {
  t.after(resetUpdate)
  const downloads = []
  const fixture = await setup(t, { dirty: false, exit: async () => { throw 'update.installFailedManual' },
    download: async version => {
      downloads.push(version)
      if (downloads.length === 1) throw 'assetBundle.recoveryPending'
    },
  })
  beginAppUpdateCheck()
  finishAppUpdateCheck(offerResult)
  finishBackgroundAppUpdate({ update: offer })
  beginDeferringAppUpdate()
  finishDeferringAppUpdate()
  await fixture.emitClose().pending
  assert.equal(appUpdateState.stage, 'stageFailed')
  assert.equal(appUpdateState.deferred, false)
  await fixture.updateExit('download')
  assert.equal(appUpdateState.resolvingExitBlock, true)
  assert.equal(appUpdateBlocksOperations(), false)
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 1)
  assert.equal(appUpdateState.resolvingExitBlock, false)
  await fixture.updateExit('download')
  assert.deepEqual(downloads, [offer.version, offer.version])
  assert.equal(appUpdateState.stage, 'handoff')
})

test('the only non-installer exit of a mandatory update is explicit quit for OS restart', async t => {
  t.after(resetUpdate)
  const fixture = await setup(t, { dirty: false })
  beginAppUpdateCheck()
  finishAppUpdateCheck({ ...offerResult, installation_status: 'applying' })
  await fixture.updateExit('reboot')
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 0)
  beginAppUpdateCheck()
  finishAppUpdateCheck({ ...offerResult, installation_status: 'reboot_required' })
  await fixture.emitClose().pending
  assert.equal(fixture.state.exits, 0)
  await fixture.updateExit('reboot')
  assert.equal(fixture.state.exits, 1)
  assert.equal(fixture.state.applies, 0)
})
