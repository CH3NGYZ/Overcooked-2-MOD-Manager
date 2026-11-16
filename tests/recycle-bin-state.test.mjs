import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'

// Run the real shared store and action coordinator. Only native IPC, directory
// events and provider-dependent UI services are replaced; no user files change.
const boundary = globalThis.__recycleBinBoundary = {
  calls: [], notices: [], dialogs: [], listener: null, subscriptions: 0,
  snapshot: [], running: false, tauri: true,
  read: async () => boundary.snapshot,
  remove: async () => ({ ok: true, message: 'recycle.modPermanentlyDeleted' }),
  restore: async () => ({ ok: true, message: 'recycle.modRestored' }),
  clear: async () => ({ ok: true, message: 'recycle.emptied' }),
}

registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'recycle-test:dialog', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'recycle-test:dialog') return fake(`
      export const useDialog=()=>({error:options=>globalThis.__recycleBinBoundary.dialogs.push(options)})`)
    if (url.endsWith('/src/services/tauri.ts')) return fake(`
      const b=globalThis.__recycleBinBoundary
      export const watchRecycleBin=async listener=>{
        b.subscriptions++; b.listener=listener; b.calls.push(['listen']); return ()=>{}
      }
      export const api={
        getModRecycleBin:async()=>{b.calls.push(['read']);return b.read()},
        deleteRecycledMod:async id=>{b.calls.push(['delete',id]);return b.remove(id)},
        restoreRecycledMod:async id=>{b.calls.push(['restore',id]);return b.restore(id)},
        deleteAllRecycledMods:async()=>{b.calls.push(['clear']);return b.clear()},
      }`)
    if (url.endsWith('/src/runtime.ts')) return fake('export const isTauri=()=>globalThis.__recycleBinBoundary.tauri')
    if (url.endsWith('/src/locale/backend.ts')) return fake('export const backendMessageOf=String; export const translateBackendMessage=String')
    if (url.endsWith('/src/i18n.ts')) return fake('export const t=(key,params)=>params?`${key}:${JSON.stringify(params)}`:key')
    if (url.endsWith('/src/useNotify.ts')) return fake(`
      export const useNotify=()=>({
        success:text=>globalThis.__recycleBinBoundary.notices.push(['success',text]),
        error:text=>globalThis.__recycleBinBoundary.notices.push(['error',text]),
      })`)
    if (url.endsWith('/src/composables/useGameOperationGate.ts')) return fake(`
      export const useGameOperationGate=()=>({blockModOperation:()=>globalThis.__recycleBinBoundary.running})`)
    return next(url, context)
  },
})

const { useRecycleBin } = await import('../src/composables/useRecycleBin.ts')
const { useRecycleBinActions } = await import('../src/composables/useRecycleBinActions.ts')
const bin = useRecycleBin()
const tick = () => new Promise(resolve => setImmediate(resolve))
function deferred() {
  let resolve
  const promise = new Promise(done => { resolve = done })
  return { promise, resolve }
}
const entry = (id, size_bytes) => ({
  id, size_bytes, name: id, deleted_at: '2026-11-16T00:00:00Z',
  file_count: 1, files: [`${id}.dll`], game_path: 'C:\\game',
})

async function setup(snapshot) {
  boundary.snapshot = snapshot
  boundary.read = async () => boundary.snapshot
  boundary.remove = async () => ({ ok: true, message: 'recycle.modPermanentlyDeleted' })
  boundary.restore = async () => ({ ok: true, message: 'recycle.modRestored' })
  boundary.clear = async () => ({ ok: true, message: 'recycle.emptied' })
  boundary.running = false
  boundary.tauri = true
  await bin.reload()
  boundary.calls = []
  boundary.notices = []
  boundary.dialogs = []
}

test('two consumers share one subscribed snapshot and one concurrent initial read', async () => {
  const menu = useRecycleBin(), fullDialog = useRecycleBin(), reading = deferred()
  boundary.read = () => reading.promise

  const first = menu.ensureLoaded(), second = fullDialog.ensureLoaded()
  assert.equal(first, second)
  await tick()
  assert.deepEqual(boundary.calls, [['listen'], ['read']], 'subscription precedes the first snapshot')
  assert.equal(menu.loading.value, true)
  assert.equal(fullDialog.loaded.value, false)
  reading.resolve([entry('mod', 4), entry('map', 9)])
  await Promise.all([first, second])

  assert.equal(menu.entries, fullDialog.entries)
  assert.equal(menu.count, fullDialog.count)
  assert.equal(menu.totalBytes, fullDialog.totalBytes)
  assert.equal(menu.acting, fullDialog.acting)
  assert.equal(menu.count.value, 2)
  assert.equal(menu.totalBytes.value, 13)
  assert.equal(fullDialog.loaded.value, true)
  assert.equal(fullDialog.loading.value, false)
  await fullDialog.ensureLoaded()
  assert.equal(boundary.calls.filter(([kind]) => kind === 'read').length, 1)
  assert.equal(boundary.subscriptions, 1)
})

test('the high-usage indicator starts strictly above 1 GiB and clears for an empty bin', async () => {
  await setup([entry('first', 1024 ** 3 - 1), entry('second', 1)])
  assert.equal(bin.totalBytes.value, 1024 ** 3)
  assert.equal(bin.overLimit.value, false)

  boundary.snapshot = [entry('first', 1024 ** 3), entry('second', 1)]
  await bin.reload()
  assert.equal(bin.totalBytes.value, 1024 ** 3 + 1)
  assert.equal(bin.overLimit.value, true)

  boundary.snapshot = []
  await bin.reload()
  assert.equal(bin.totalBytes.value, 0)
  assert.equal(bin.count.value, 0)
  assert.equal(bin.overLimit.value, false)
})

test('a failed refresh keeps the last trustworthy list and byte count until a successful retry', async () => {
  const trusted = [entry('retained', 77)]
  await setup(trusted)
  boundary.read = async () => { throw new Error('disk unavailable') }

  await bin.reload()
  assert.equal(bin.entries.value, trusted)
  assert.equal(bin.totalBytes.value, 77)
  assert.equal(bin.loaded.value, true)
  assert.equal(bin.loading.value, false)
  assert.match(bin.error.value, /disk unavailable/)

  boundary.read = async () => [entry('replacement', 88)]
  await bin.reload()
  assert.equal(bin.totalBytes.value, 88)
  assert.equal(bin.error.value, '')
})

test('directory changes during a read discard its stale snapshot and coalesce into one follow-up', async () => {
  const trusted = [entry('trusted', 30)]
  await setup(trusted)
  const stale = deferred(), fresh = deferred()
  let reads = 0
  boundary.read = () => ++reads === 1 ? stale.promise : fresh.promise

  const pending = bin.reload()
  await tick()
  boundary.listener()
  boundary.listener()
  boundary.listener()
  assert.equal(reads, 1, 'a burst does not start parallel native scans')
  stale.resolve([entry('stale', 1)])
  await tick()
  assert.equal(reads, 2)
  assert.equal(bin.entries.value, trusted, 'a stale result must never flash on either surface')
  assert.equal(bin.loading.value, true)
  fresh.resolve([entry('current', 60)])
  await pending

  assert.deepEqual(bin.entries.value.map(item => item.id), ['current'])
  assert.equal(bin.totalBytes.value, 60)
  assert.equal(bin.loading.value, false)
  assert.equal(boundary.subscriptions, 1, 'all consumers reuse one process subscription')
})

test('operations share one busy gate across consumers and remain locked through authoritative read-back', async () => {
  const firstEntry = entry('first', 10), secondEntry = entry('second', 20)
  await setup([firstEntry, secondEntry])
  const menu = useRecycleBinActions(), fullDialog = useRecycleBinActions()
  const deleting = deferred(), rereading = deferred()
  boundary.remove = () => deleting.promise
  boundary.read = () => rereading.promise

  const pending = menu.remove(firstEntry)
  assert.equal(bin.busy.value, true)
  await fullDialog.remove(secondEntry)
  await fullDialog.restore(secondEntry)
  fullDialog.confirmClearAll()
  assert.deepEqual(boundary.calls, [['delete', 'first']])
  assert.deepEqual(boundary.dialogs, [])
  deleting.resolve({ ok: true, message: 'deleted' })
  await tick()
  assert.equal(bin.busy.value, true, 'the old totals remain gated until the new snapshot arrives')
  assert.deepEqual(boundary.calls, [['delete', 'first'], ['read']])
  rereading.resolve([secondEntry])
  await pending

  assert.equal(bin.busy.value, false)
  assert.equal(bin.count.value, 1)
  assert.equal(bin.totalBytes.value, 20)
  assert.deepEqual(boundary.notices, [['success', 'deleted']])
})

test('emptying the bin requires confirmation and updates both consumers from the resulting snapshot', async () => {
  await setup([entry('mod', 15), entry('map', 25)])
  const menu = useRecycleBinActions(), fullDialog = useRecycleBin()

  menu.confirmClearAll()
  assert.deepEqual(boundary.calls, [], 'opening confirmation performs no destructive native call')
  assert.equal(boundary.dialogs.length, 1)
  assert.match(boundary.dialogs[0].content, /"count":2/)
  assert.equal(fullDialog.totalBytes.value, 40)
  boundary.snapshot = []
  await boundary.dialogs[0].onPositiveClick()

  assert.deepEqual(boundary.calls, [['clear'], ['read']])
  assert.equal(fullDialog.count.value, 0)
  assert.equal(fullDialog.totalBytes.value, 0)
  assert.equal(fullDialog.busy.value, false)
})

test('partially failed clearing re-reads remaining records instead of setting totals to zero', async () => {
  const remaining = entry('remaining', 19)
  await setup([entry('removed', 11), remaining])
  boundary.clear = async () => {
    boundary.snapshot = [remaining]
    return { ok: false, message: 'partial failure' }
  }
  const actions = useRecycleBinActions()

  actions.confirmClearAll()
  await boundary.dialogs[0].onPositiveClick()

  assert.deepEqual(boundary.calls, [['clear'], ['read']])
  assert.deepEqual(bin.entries.value, [remaining])
  assert.equal(bin.totalBytes.value, 19)
  assert.deepEqual(boundary.notices, [['error', 'partial failure']])
  assert.equal(bin.busy.value, false)
})

test('an IPC failure still reads back the bin and releases the operation gate', async () => {
  const remaining = entry('remaining', 55)
  await setup([remaining])
  boundary.remove = async () => { throw new Error('permission denied') }

  await useRecycleBinActions().remove(remaining)

  assert.deepEqual(boundary.calls, [['delete', 'remaining'], ['read']])
  assert.equal(bin.totalBytes.value, 55)
  assert.match(boundary.notices[0][1], /permission denied/)
  assert.equal(bin.busy.value, false)
})

test('a running game prevents restore IPC but does not block deleting recycled files', async () => {
  const recycled = entry('chef', 90)
  await setup([recycled])
  boundary.running = true
  const actions = useRecycleBinActions()

  await actions.restore(recycled)
  assert.deepEqual(boundary.calls, [])
  assert.equal(bin.busy.value, false)
  boundary.snapshot = []
  await actions.remove(recycled)
  assert.deepEqual(boundary.calls, [['delete', 'chef'], ['read']])
  assert.equal(bin.count.value, 0)
})

test('titlebar recycle menu delegates hover visibility to Naive and closes before opening dialogs', () => {
  const status = readFileSync(new URL('../src/components/RecycleBinStatus.vue', import.meta.url), 'utf8')
  const popover = status.match(/<n-popover\b[^>]*>/)?.[0]
  assert.ok(popover)
  assert.match(popover, /trigger="hover"/)
  assert.match(popover, /:show="menuOpen"/)
  assert.match(popover, /@update:show="showMenu"/)
  assert.match(status, /function showMenu\(show: boolean\)\s*\{\s*menuOpen\.value = show\s+if \(show\) void reload\(\)/)
  assert.match(status, /@click="openBin"/)
  assert.match(status, /function openBin\(\)\s*\{\s*menuOpen\.value = false\s+dialogOpen\.value = true/)
  assert.match(status, /@click="clearBin"/)
  assert.match(status, /async function clearBin\(\)\s*\{\s*menuOpen\.value = false[\s\S]*?await nextTick\(\)\s+confirmClearAll\(\)/)
})
