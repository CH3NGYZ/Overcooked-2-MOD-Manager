import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

// Exercise the real request, queue and cache together. Only IPC and the GPU
// boundary are replaced, so leases and cancellation retain their real lifetime.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal module resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/services/tauri.ts')) return { format: 'module', shortCircuit: true, source: `
      export const api = new Proxy({}, {get: (_, key) => (...args) => globalThis.__chefThumbnailBoundary.api[key](...args)})
    ` }
    if (url.endsWith('/src/diychef/preview/scene.ts')) return { format: 'module', shortCircuit: true, source: `
      export const prepareChef = (...args) => globalThis.__chefThumbnailBoundary.prepareChef(...args)
    ` }
    if (url.endsWith('/src/diychef/preview/runtime.ts')) return { format: 'module', shortCircuit: true, source: `
      export const chefRenderer = () => globalThis.__chefThumbnailBoundary.renderer
      export const releaseChefRenderer = () => { globalThis.__chefThumbnailBoundary.releases++ }
    ` }
    return next(url, context)
  },
})
const { clearChefThumbnailCache, requestChefThumbnail } = await import('../src/composables/useDiyChefThumbnails.ts')
const { thumbnailCacheStats } = await import('../src/diychef/preview/cache.ts')
const { configurePreviewConcurrency, reservePreviewRenderer } = await import('../src/diychef/preview/queue.ts')
const tick = () => new Promise(resolve => setImmediate(resolve))
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
let sequence = 0
function setup(t, status = 'partial') {
  configurePreviewConcurrency(0)
  t.after(() => configurePreviewConcurrency(0))
  const identity = `thumbnail-case-${++sequence}`
  const boundary = {
    status, fingerprint: identity, captures: 0, previews: 0, metadata: 0, reads: 0,
    writes: [], created: [], revoked: [], releases: 0, disposals: 0,
    disk: new ArrayBuffer(0), captureGate: null, prepareGate: null, onClear() {},
    prepares: 0, preparing: 0, preparationPeak: 0, livePrepared: 0, preparedPeak: 0,
    activeCaptures: 0, capturePeak: 0, capturedFingerprints: [], current: null, clears: 0, clearGate: null,
    fingerprintFor: () => boundary.fingerprint,
    async prepareChef(_session, scene, signal) {
      boundary.prepares++; boundary.preparing++
      boundary.preparationPeak = Math.max(boundary.preparationPeak, boundary.preparing)
      try {
        if (boundary.prepareGate) await boundary.prepareGate.promise
        signal?.throwIfAborted()
        boundary.livePrepared++
        boundary.preparedPeak = Math.max(boundary.preparedPeak, boundary.livePrepared)
        let disposed = false
        return { description: scene, dispose() {
          if (disposed) return
          disposed = true
          boundary.disposals++; boundary.livePrepared--
        } }
      } finally { boundary.preparing-- }
    },
    renderer: {
      setChef(prepared) { boundary.current = prepared },
      async capture() {
        boundary.captures++; boundary.activeCaptures++
        boundary.capturePeak = Math.max(boundary.capturePeak, boundary.activeCaptures)
        boundary.capturedFingerprints.push(boundary.current.description.fingerprint)
        try {
          if (boundary.captureGate) await boundary.captureGate.promise
          return new Blob(['rendered chef'], { type: 'image/png' })
        } finally { boundary.activeCaptures-- }
      },
      clear() { boundary.current?.dispose(); boundary.current = null; boundary.onClear() },
    },
    api: {
      async clearDiyChefCache() {
        boundary.clears++
        if (boundary.clearGate) await boundary.clearGate.promise
        boundary.disk = new ArrayBuffer(0)
        return 7
      },
      async diyChefFingerprint(_session, chef) { boundary.metadata++; return boundary.fingerprintFor(chef) },
      async diyChefThumbnail(key, bytes, partial) {
        if (bytes) { boundary.writes.push({ key, bytes, partial }); return }
        boundary.reads++
        return boundary.disk
      },
      async diyChefPreview(_session, chef) {
        boundary.previews++
        return { status: boundary.status, fingerprint: boundary.fingerprintFor(chef),
          meshes: [{}], diagnostics: boundary.status === 'partial' ? ['temporary texture error'] : [] }
      },
    },
    request: signal => requestChefThumbnail(identity, 'fox', 'hat', signal),
  }
  globalThis.__chefThumbnailBoundary = boundary
  t.mock.method(URL, 'createObjectURL', () => {
    const url = `blob:${identity}-${boundary.created.length}`
    boundary.created.push(url)
    return url
  })
  t.mock.method(URL, 'revokeObjectURL', url => { boundary.revoked.push(url) })
  return boundary
}

test('partial thumbnails retry unchanged metadata without revoking the still-visible older image', async t => {
  const boundary = setup(t)
  const partial = await boundary.request()
  assert.equal(partial.state, 'partial')
  assert.deepEqual(partial.diagnostics, ['temporary texture error'])
  boundary.status = 'complete'
  const complete = await boundary.request()
  assert.equal(complete.state, 'ready')
  assert.notEqual(complete.url, partial.url)
  assert.equal(boundary.previews, 2)
  assert.equal(boundary.captures, 2)
  assert.equal(boundary.writes.length, 1)
  assert.equal(boundary.writes[0].partial, false)
  assert.equal(boundary.revoked.includes(partial.url), false)
  partial.release(); partial.release()
  assert.deepEqual(boundary.revoked, [partial.url])
  assert.equal(boundary.revoked.includes(complete.url), false)
  complete.release()
  const reused = await boundary.request()
  assert.equal(reused.url, complete.url)
  assert.equal(boundary.captures, 2)
  reused.release()
})

test('concurrent partial consumers share one capture and keep it alive until the final lease releases', async t => {
  const boundary = setup(t)
  boundary.captureGate = deferred()
  const first = boundary.request(), second = boundary.request()
  await tick()
  assert.equal(boundary.captures, 1)
  boundary.captureGate.resolve()
  const [a, b] = await Promise.all([first, second])
  assert.equal(a.state, 'partial')
  assert.equal(a.url, b.url)
  assert.equal(boundary.created.length, 1)
  assert.equal(boundary.writes.length, 0)
  a.release()
  assert.deepEqual(boundary.revoked, [])
  b.release()
  assert.deepEqual(boundary.revoked, [a.url])
  assert.equal(thumbnailCacheStats().referenced, 0)
})

test('all consumers cancelling after capture discard an unleased partial and permit another request', async t => {
  const boundary = setup(t)
  const before = thumbnailCacheStats()
  const a = new AbortController(), b = new AbortController()
  boundary.onClear = () => { a.abort(); b.abort() }
  const results = await Promise.all([boundary.request(a.signal), boundary.request(b.signal)])
  assert.deepEqual(results.map(value => value.state), ['error', 'error'])
  assert.equal(boundary.captures, 1)
  assert.equal(boundary.created.length, 1)
  assert.deepEqual(boundary.revoked, boundary.created)
  assert.deepEqual(thumbnailCacheStats(), before)
  boundary.onClear = () => {}
  const next = await boundary.request()
  assert.equal(next.state, 'partial')
  assert.equal(boundary.captures, 2)
  assert.notEqual(next.url, boundary.created[0])
  next.release()
  assert.deepEqual(thumbnailCacheStats(), before)
})

test('complete fingerprint thumbnails survive session replacement without another disk read or capture', async t => {
  const boundary = setup(t, 'complete')
  const first = await boundary.request()
  first.release()
  const second = await requestChefThumbnail('another-session', 'fox', 'hat')
  assert.equal(second.state, 'ready')
  assert.equal(first.url, second.url)
  assert.equal(boundary.metadata, 3, 'only the cold render adds a final metadata recheck')
  assert.equal(boundary.reads, 1)
  assert.equal(boundary.captures, 1)
  assert.equal(boundary.writes.length, 1)
  second.release()
})

test('an older partial disk record cannot suppress a fresh scene conversion', async t => {
  const boundary = setup(t, 'complete')
  boundary.disk = new Uint8Array([1, 137, 80, 78, 71]).buffer
  const result = await boundary.request()
  assert.equal(result.state, 'ready')
  assert.equal(boundary.previews, 1)
  assert.equal(boundary.captures, 1)
  assert.equal(boundary.writes.length, 1)
  result.release()
})

test('a complete disk record continues to bypass scene conversion and GPU capture', async t => {
  const boundary = setup(t, 'complete')
  boundary.disk = new Uint8Array([0, 137, 80, 78, 71]).buffer
  const result = await boundary.request()
  assert.equal(result.state, 'ready')
  assert.equal(boundary.previews, 0)
  assert.equal(boundary.captures, 0)
  assert.equal(boundary.writes.length, 0)
  result.release()
})

test('an unchanged warm thumbnail revalidates and returns while an unrelated cold capture is pending', async t => {
  const boundary = setup(t, 'complete')
  const fingerprint = boundary.fingerprint
  const warm = await boundary.request()
  warm.release()
  boundary.api.diyChefFingerprint = async (_session, chef) => {
    boundary.metadata++
    return chef === 'cold' ? `${fingerprint}-cold` : fingerprint
  }
  boundary.api.diyChefPreview = async () => {
    boundary.previews++
    return { status: 'complete', fingerprint: `${fingerprint}-cold`, meshes: [{}], diagnostics: [] }
  }
  boundary.captureGate = deferred()
  const cold = requestChefThumbnail('cold-session', 'cold', 'hat')
  let reused, settled = false
  try {
    await tick()
    assert.equal(boundary.captures, 2)
    reused = requestChefThumbnail('refreshed-session', 'fox', 'hat').then(result => {
      settled = true
      return result
    })
    await tick()
    assert.equal(settled, true, 'unchanged cached images must not wait behind another chef GPU capture')
    const result = await reused
    assert.equal(result.url, warm.url)
    assert.equal(boundary.metadata, 5, 'both cold captures recheck metadata; the warm hit only checks once')
    assert.equal(boundary.reads, 2, 'only the original warm and unrelated cold requests read disk')
    result.release()
  } finally {
    boundary.captureGate.resolve()
    ;(await cold).release()
    if (reused) (await reused).release()
  }
})

test('disk hits and warm hits remain available while a hat dialog reserves the renderer', async t => {
  const boundary = setup(t, 'complete')
  boundary.disk = new Uint8Array([0, 137, 80, 78, 71]).buffer
  const release = await reservePreviewRenderer()
  let pending, settled = false
  try {
    pending = boundary.request().then(result => { settled = true; return result })
    await tick()
    assert.equal(settled, true, 'a cached PNG does not need the reserved WebGL canvas')
    const disk = await pending
    const warm = await boundary.request()
    assert.equal(warm.url, disk.url)
    assert.equal(boundary.metadata, 2)
    assert.equal(boundary.reads, 1)
    assert.equal(boundary.captures, 0)
    assert.equal(boundary.previews, 0)
    warm.release(); disk.release()
  } finally {
    release()
    if (pending) (await pending).release()
  }
})

test('refreshing an unchanged resource keeps its URL, but a changed fingerprint renders a new image', async t => {
  const boundary = setup(t, 'complete')
  const original = await boundary.request()
  for (let i = 0; i < 3; i++) {
    const warm = await requestChefThumbnail(`refresh-${i}`, 'fox', 'hat')
    assert.equal(warm.url, original.url)
    warm.release()
  }
  assert.equal(boundary.metadata, 5)
  assert.equal(boundary.captures, 1)
  assert.equal(boundary.reads, 1)
  boundary.fingerprint += '-changed-file-list-size-or-mtime'
  const changed = await requestChefThumbnail('changed-resource-session', 'fox', 'hat')
  assert.equal(changed.state, 'ready')
  assert.notEqual(changed.url, original.url)
  assert.equal(boundary.metadata, 7)
  assert.equal(boundary.captures, 2)
  assert.equal(boundary.reads, 2)
  assert.equal(boundary.revoked.includes(original.url), false)
  original.release(); changed.release()
})

test('an expired or invalid session cannot reuse an earlier session thumbnail', async t => {
  const boundary = setup(t, 'complete')
  const original = await boundary.request()
  boundary.api.diyChefFingerprint = async () => { throw new Error('expired session') }
  const expired = await requestChefThumbnail('expired-session', 'fox', 'hat')
  assert.equal(expired.state, 'error')
  assert.equal(expired.url, '')
  assert.equal(boundary.reads, 1)
  assert.equal(boundary.captures, 1)
  original.release()
})

test('a concurrent disk-hit batch keeps every PNG alive until its caller acquires a lease', async t => {
  const boundary = setup(t, 'complete')
  boundary.disk = new Uint8Array([0, 137, 80, 78, 71]).buffer
  boundary.api.diyChefFingerprint = async (_session, chef) => {
    boundary.metadata++
    return `${boundary.fingerprint}-${chef}`
  }
  const batch = await Promise.all(Array.from({ length: 80 }, (_, i) => requestChefThumbnail('batch-session', `chef-${i}`, 'hat')))
  assert.equal(batch.filter(result => result.state === 'ready').length, 80)
  assert.equal(new Set(batch.map(result => result.url)).size, 80)
  assert.equal(boundary.metadata, 80)
  assert.equal(boundary.reads, 80)
  assert.equal(boundary.previews, 0)
  assert.equal(boundary.captures, 0)
  assert.equal(thumbnailCacheStats().referenced, 80)
  assert.equal(batch.some(result => boundary.revoked.includes(result.url)), false)
  batch.forEach(result => result.release())
  assert.equal(thumbnailCacheStats().referenced, 0)
  assert.ok(thumbnailCacheStats().bytes <= 32 * 1024 * 1024)
})

test('matching resources from concurrent sessions still capture only once', async t => {
  const boundary = setup(t, 'complete')
  boundary.captureGate = deferred()
  const first = boundary.request()
  const second = requestChefThumbnail('concurrent-session', 'fox', 'hat')
  await tick()
  assert.equal(boundary.captures, 1)
  boundary.captureGate.resolve()
  const [a, b] = await Promise.all([first, second])
  assert.equal(a.state, 'ready')
  assert.equal(b.state, 'ready')
  assert.equal(a.url, b.url)
  assert.equal(boundary.captures, 1)
  a.release(); b.release()
})

test('a resource changed while queued is published only under its freshly checked scene fingerprint', async t => {
  const boundary = setup(t, 'complete')
  const before = boundary.fingerprint
  const release = await reservePreviewRenderer()
  const pending = boundary.request()
  try {
    await tick()
    assert.equal(boundary.metadata, 1)
    assert.equal(boundary.reads, 1)
    assert.equal(boundary.previews, 0)
    boundary.fingerprint += '-changed-while-queued'
  } finally { release() }
  const result = await pending
  assert.equal(result.state, 'ready')
  assert.equal(boundary.captures, 1)
  assert.equal(boundary.writes.length, 1)
  assert.ok(boundary.writes[0].key.endsWith(boundary.fingerprint))
  assert.equal(boundary.writes[0].key.endsWith(before), false)
  result.release()
})

for (const [setting, limit] of [[1, 1], [0, 3]]) {
  test(`cold thumbnail preparation setting ${setting} admits ${limit} jobs and bounds prepared scenes behind serial GPU`, async t => {
    const boundary = setup(t, 'complete')
    t.mock.getter(navigator, 'hardwareConcurrency', () => 3)
    configurePreviewConcurrency(setting)
    boundary.fingerprintFor = chef => `${boundary.fingerprint}-${chef}`
    boundary.prepareGate = deferred()
    boundary.captureGate = deferred()
    const pending = Array.from({ length: 6 }, (_, i) => requestChefThumbnail('bounded-session', `chef-${i}`, 'hat'))
    try {
      await tick()
      assert.equal(boundary.previews, limit)
      assert.equal(boundary.preparing, limit)
      assert.equal(boundary.preparationPeak, limit)
      boundary.prepareGate.resolve()
      await tick()
      assert.equal(boundary.livePrepared, limit, 'waiting for the GPU retains each cold-pipeline permit')
      assert.equal(boundary.captures, 1)
      assert.equal(boundary.previews, limit, 'GPU backlog must not admit additional scene reads')
    } finally {
      boundary.prepareGate.resolve(); boundary.captureGate.resolve()
      const results = await Promise.all(pending)
      assert.equal(results.filter(result => result.state === 'ready').length, 6)
      results.forEach(result => result.release())
    }
    assert.equal(boundary.capturePeak, 1)
    assert.equal(boundary.preparedPeak, limit)
    assert.equal(boundary.livePrepared, 0)
    assert.equal(boundary.disposals, 6)
  })
}

test('cancelling prepared thumbnails behind a hat dialog disposes them before the dialog releases', async t => {
  const boundary = setup(t, 'complete')
  configurePreviewConcurrency(2)
  boundary.fingerprintFor = chef => `${boundary.fingerprint}-${chef}`
  boundary.prepareGate = deferred()
  const abort = new AbortController()
  const cancelled = requestChefThumbnail('cancelled-session', 'cancelled', 'hat', abort.signal)
  const retained = requestChefThumbnail('retained-session', 'retained', 'hat')
  await tick()
  const release = await reservePreviewRenderer()
  let next
  try {
    boundary.prepareGate.resolve()
    await tick()
    assert.equal(boundary.livePrepared, 2)
    assert.equal(boundary.captures, 0)
    abort.abort()
    assert.equal((await cancelled).state, 'error')
    assert.equal(boundary.livePrepared, 1)
    assert.equal(boundary.disposals, 1)
    assert.equal(boundary.releases, 0, 'discarding CPU resources must not touch the dialog renderer')
    next = requestChefThumbnail('next-session', 'next', 'hat')
    await tick()
    assert.equal(boundary.previews, 2, 'new scene preparation pauses while the dialog owns GPU')
  } finally {
    release(); boundary.prepareGate.resolve()
    ;(await retained).release()
    if (next) (await next).release()
    ;(await cancelled).release()
  }
  assert.equal(boundary.captures, 2)
  assert.equal(boundary.livePrepared, 0)
  assert.equal(boundary.disposals, 3)
})

test('a prepared scene changed while awaiting GPU is disposed and retried under the new fingerprint', async t => {
  const boundary = setup(t, 'complete')
  configurePreviewConcurrency(2)
  let version = 'before'
  boundary.fingerprintFor = chef => `${boundary.fingerprint}-${chef}-${chef === 'target' ? version : 'fixed'}`
  boundary.captureGate = deferred()
  const blocker = requestChefThumbnail('blocker-session', 'blocker', 'hat')
  const target = requestChefThumbnail('target-session', 'target', 'hat')
  try {
    await tick()
    assert.equal(boundary.captures, 1)
    assert.equal(boundary.previews, 2)
    assert.equal(boundary.livePrepared, 2)
    version = 'after'
  } finally { boundary.captureGate.resolve() }
  const results = await Promise.all([blocker, target])
  assert.equal(results.filter(result => result.state === 'ready').length, 2)
  assert.equal(boundary.captures, 2)
  assert.equal(boundary.previews, 3)
  assert.equal(boundary.disposals, 3)
  assert.equal(boundary.livePrepared, 0)
  assert.equal(boundary.capturedFingerprints.some(value => value.endsWith('target-before')), false)
  assert.equal(boundary.writes.some(value => value.key.endsWith('target-before')), false)
  assert.equal(boundary.writes.some(value => value.key.endsWith('target-after')), true)
  results.forEach(result => result.release())
})

test('a failed scene preparation releases its permit for later thumbnails', async t => {
  const boundary = setup(t, 'complete')
  configurePreviewConcurrency(1)
  boundary.fingerprintFor = chef => `${boundary.fingerprint}-${chef}`
  const prepare = boundary.prepareChef
  boundary.prepareChef = async (...args) => {
    if (args[1].fingerprint.endsWith('-bad')) throw new Error('invalid-geometry')
    return prepare(...args)
  }
  const [failed, next] = await Promise.all([
    requestChefThumbnail('bad-session', 'bad', 'hat'),
    requestChefThumbnail('next-session', 'good', 'hat'),
  ])
  assert.equal(failed.state, 'error')
  assert.deepEqual(failed.diagnostics, ['invalid-geometry'])
  assert.equal(next.state, 'ready')
  assert.equal(boundary.captures, 1)
  assert.equal(boundary.livePrepared, 0)
  assert.equal(boundary.disposals, 1)
  failed.release(); next.release()
})

test('clearing aborts a running capture, coalesces clicks and holds new requests until backend cleanup completes', async t => {
  const boundary = setup(t, 'complete')
  boundary.captureGate = deferred()
  boundary.clearGate = deferred()
  const old = boundary.request()
  await tick()
  assert.equal(boundary.captures, 1)
  const clear = clearChefThumbnailCache()
  assert.equal(clearChefThumbnailCache(), clear)
  const fresh = boundary.request()
  try {
    await tick()
    assert.equal(boundary.clears, 1)
    assert.equal(boundary.metadata, 2, 'a new request must wait before checking metadata')
    boundary.captureGate.resolve()
    assert.equal((await old).state, 'error')
    assert.equal(boundary.writes.length, 0)
    assert.equal(boundary.created.length, 0, 'old capture must not repopulate the memory cache')
  } finally { boundary.captureGate.resolve(); boundary.clearGate.resolve() }
  assert.equal(await clear, 7)
  const result = await fresh
  assert.equal(result.state, 'ready')
  assert.equal(boundary.captures, 2)
  assert.equal(boundary.writes.length, 1)
  assert.equal(boundary.livePrepared, 0)
  result.release()
})

test('clearing during CPU preparation releases the old job without touching the renderer and allows regeneration', async t => {
  const boundary = setup(t, 'complete')
  boundary.prepareGate = deferred()
  const old = boundary.request()
  await tick()
  assert.equal(boundary.preparing, 1)
  await clearChefThumbnailCache()
  boundary.prepareGate.resolve()
  assert.equal((await old).state, 'error')
  assert.equal(boundary.captures, 0)
  assert.equal(boundary.releases, 0)
  assert.equal(boundary.writes.length, 0)
  const fresh = await boundary.request()
  assert.equal(fresh.state, 'ready')
  assert.equal(boundary.previews, 2)
  assert.equal(boundary.livePrepared, 0)
  fresh.release()
})

test('clearing cancels queued work while a hat dialog retains the renderer', async t => {
  const boundary = setup(t, 'complete')
  const release = await reservePreviewRenderer()
  const old = boundary.request()
  try {
    await tick()
    assert.equal(boundary.previews, 0)
    await clearChefThumbnailCache()
    assert.equal((await old).state, 'error')
  } finally { release() }
  await tick()
  assert.equal(boundary.captures, 0)
  const fresh = await boundary.request()
  assert.equal(fresh.state, 'ready')
  assert.equal(boundary.captures, 1)
  fresh.release()
})

test('a disk read returning after clear cannot restore its old PNG', async t => {
  const boundary = setup(t, 'complete')
  const diskGate = deferred()
  const original = boundary.api.diyChefThumbnail
  boundary.api.diyChefThumbnail = async (key, bytes, partial) => {
    if (bytes) return original(key, bytes, partial)
    boundary.reads++
    await diskGate.promise
    return new Uint8Array([0, 137, 80, 78, 71]).buffer
  }
  const old = boundary.request()
  await tick()
  await clearChefThumbnailCache()
  diskGate.resolve()
  assert.equal((await old).state, 'error')
  assert.equal(boundary.created.length, 0)
  boundary.api.diyChefThumbnail = original
  const fresh = await boundary.request()
  assert.equal(fresh.state, 'ready')
  assert.equal(boundary.captures, 1)
  fresh.release()
})

test('a PNG being encoded when clear begins cannot be written back afterwards', async t => {
  const boundary = setup(t, 'complete')
  const encoding = deferred()
  const capture = boundary.renderer.capture
  boundary.renderer.capture = async () => {
    const image = await capture()
    image.arrayBuffer = async () => { await encoding.promise; return new Uint8Array([1, 2, 3]).buffer }
    return image
  }
  const old = boundary.request()
  await tick()
  assert.equal(boundary.created.length, 1, 'capture has entered persistence outside the GPU lane')
  await clearChefThumbnailCache()
  encoding.resolve()
  assert.equal((await old).state, 'error')
  assert.equal(boundary.writes.length, 0)
  assert.equal(boundary.revoked.includes(boundary.created[0]), true)
  boundary.renderer.capture = capture
  const fresh = await boundary.request()
  assert.equal(fresh.state, 'ready')
  assert.equal(boundary.writes.length, 1)
  fresh.release()
})

test('failed backend cleanup preserves a visible lease and does not leave future requests blocked', async t => {
  const boundary = setup(t, 'complete')
  const visible = await boundary.request()
  boundary.api.clearDiyChefCache = async () => { throw new Error('diychef.previewUnavailable') }
  await assert.rejects(clearChefThumbnailCache(), /diychef.previewUnavailable/)
  assert.equal(boundary.revoked.includes(visible.url), false)
  assert.equal(thumbnailCacheStats().entries, 0)
  const fresh = await boundary.request()
  assert.equal(fresh.state, 'ready')
  assert.notEqual(fresh.url, visible.url)
  visible.release()
  assert.equal(boundary.revoked.includes(visible.url), true)
  assert.equal(boundary.revoked.includes(fresh.url), false)
  fresh.release()
})

test('fast scrolling bounds metadata IPC and removes offscreen work before it reaches Rust', async t => {
  const boundary = setup(t, 'complete')
  t.mock.getter(navigator, 'hardwareConcurrency', () => 8)
  const gate = deferred()
  let active = 0, peak = 0
  boundary.disk = new Uint8Array([0, 137, 80, 78, 71]).buffer
  boundary.api.diyChefFingerprint = async (_session, chef) => {
    boundary.metadata++; active++; peak = Math.max(peak, active)
    try { await gate.promise; return `${boundary.fingerprint}-${chef}` }
    finally { active-- }
  }
  const controls = Array.from({ length: 40 }, () => new AbortController())
  const pending = controls.map((control, index) => requestChefThumbnail('scroll-session', `chef-${index}`, 'hat', control.signal))
  try {
    await tick()
    assert.ok(peak <= 4, `metadata calls must be bounded before invoke, got ${peak}`)
    controls.slice(0, -4).forEach(control => control.abort())
    await tick()
  } finally { gate.resolve() }
  const results = await Promise.all(pending)
  assert.deepEqual(results.map(result => result.state), [...Array(36).fill('error'), ...Array(4).fill('ready')])
  assert.ok(boundary.metadata <= 8, 'only the first running requests and final visible cards may reach Rust')
  assert.equal(boundary.captures, 0)
  results.forEach(result => result.release())
})

test('scrolling away and back shares an already running fingerprint even after every earlier consumer cancels', async t => {
  const boundary = setup(t, 'complete')
  const gate = deferred(), pending = []
  boundary.disk = new Uint8Array([0, 137, 80, 78, 71]).buffer
  boundary.api.diyChefFingerprint = async () => { boundary.metadata++; await gate.promise; return boundary.fingerprint }
  try {
    for (let i = 0; i < 12; i++) {
      const control = new AbortController()
      pending.push(boundary.request(control.signal))
      await tick()
      control.abort()
    }
    pending.push(boundary.request())
    await tick()
    assert.equal(boundary.metadata, 1, 'cancelling a WebView consumer cannot cancel an IPC already running in Rust')
  } finally { gate.resolve() }
  const results = await Promise.all(pending)
  assert.deepEqual(results.map(result => result.state), [...Array(12).fill('error'), 'ready'])
  assert.equal(boundary.reads, 1)
  results.forEach(result => result.release())
  const after = await boundary.request()
  assert.equal(boundary.metadata, 2, 'settled fingerprints are never reused without a new metadata check')
  after.release()
})
