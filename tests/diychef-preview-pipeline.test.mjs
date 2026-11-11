import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
    try { return next(`${specifier}.ts`, context) } catch { /* normal module resolution */ }
  }
  return next(specifier, context)
} })
const { decodeGeometry } = await import('../src/diychef/preview/geometry.ts')
const { configurePreviewConcurrency, enqueuePreviewPreparation, enqueuePreview, reservePreviewRenderer } = await import('../src/diychef/preview/queue.ts')
const { acquireThumbnail, clearThumbnailCache, putThumbnail, thumbnailCacheStats } = await import('../src/diychef/preview/cache.ts')
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
const tick = () => new Promise(resolve => setImmediate(resolve))

function triangle() {
  const buffer = new ArrayBuffer(3 * 32 + 3 * 4)
  const view = new DataView(buffer)
  const vertices = [[-1, -2, 0, 0, 0, 1, 0, 0], [1, 0, 3, 0, 0, 1, 1, 0], [0, 2, 1, 0, 0, 1, 0, 1]]
  vertices.flat().forEach((value, index) => view.setFloat32(index * 4, value, true))
  ;[2, 0, 1].forEach((value, index) => view.setUint32(96 + index * 4, value, true))
  return buffer
}

test('binary geometry decodes actual f32/u32 little-endian data, UVs and bounds', () => {
  const geometry = decodeGeometry(triangle(), 3, 3)
  try {
    assert.deepEqual([...geometry.getAttribute('position').array], [-1, -2, 0, 1, 0, 3, 0, 2, 1])
    assert.deepEqual([...geometry.getAttribute('normal').array], [0, 0, 1, 0, 0, 1, 0, 0, 1])
    assert.deepEqual([...geometry.getAttribute('uv').array], [0, 0, 1, 0, 0, 1])
    assert.deepEqual([...geometry.getIndex().array], [2, 0, 1])
    assert.deepEqual(geometry.boundingBox.min.toArray(), [-1, -2, 0])
    assert.deepEqual(geometry.boundingBox.max.toArray(), [1, 2, 3])
    assert.ok(Number.isFinite(geometry.boundingSphere.radius))
  } finally { geometry.dispose() }
})

test('binary geometry rejects truncated/extra bytes, bad counts and non-triangle index lengths', () => {
  const input = triangle()
  for (const [buffer, vertices, indices] of [
    [input.slice(0, -4), 3, 3], [new ArrayBuffer(input.byteLength + 4), 3, 3],
    [input, 3, 6], [input, 3, 2], [input, -1, 3], [input, 1.5, 3],
    [input, Number.NaN, 3], [input, Number.MAX_SAFE_INTEGER + 1, 3],
    [input, 2_000_001, 3], [input, 3, 6_000_003],
  ]) assert.throws(() => decodeGeometry(buffer, vertices, indices), /diychef\.previewInvalid/)
})

test('non-finite position, normal and UV data are rejected before creating a renderable geometry', () => {
  for (const component of [0, 4, 7]) {
    for (const value of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
      const input = triangle()
      new DataView(input).setFloat32(component * 4, value, true)
      assert.throws(() => decodeGeometry(input, 3, 3), /diychef\.previewInvalid/)
    }
  }
})

test('indices must address existing vertices, including unsigned overflow values', () => {
  for (const index of [3, 0xffff_ffff]) {
    const input = triangle()
    new DataView(input).setUint32(96, index, true)
    assert.throws(() => decodeGeometry(input, 3, 3), /diychef\.previewInvalid/)
  }
})

test('preview queue never runs two captures concurrently', async () => {
  const firstGate = deferred(), secondGate = deferred()
  let active = 0, peak = 0
  const events = []
  const job = (name, gate) => async () => {
    events.push(`start:${name}`)
    active++; peak = Math.max(active, peak)
    await gate.promise
    active--; events.push(`end:${name}`)
    return name
  }
  const first = enqueuePreview(job('first', firstGate))
  const second = enqueuePreview(job('second', secondGate))
  await tick()
  assert.deepEqual(events, ['start:first'])
  firstGate.resolve()
  assert.equal(await first, 'first')
  await tick()
  assert.deepEqual(events, ['start:first', 'end:first', 'start:second'])
  secondGate.resolve()
  assert.equal(await second, 'second')
  assert.equal(peak, 1)
  assert.equal(active, 0)
})

test('a dialog reservation waits for the active capture then pauses and resumes queued work', async () => {
  const gate = deferred()
  const events = []
  const current = enqueuePreview(async () => { events.push('current'); await gate.promise; return 'current' })
  const queued = enqueuePreview(async () => { events.push('queued'); return 'queued' })
  const reservation = reservePreviewRenderer()
  await tick()
  assert.deepEqual(events, ['current'])
  gate.resolve()
  await current
  const release = await reservation
  try {
    assert.deepEqual(events, ['current'], 'dialog owns the renderer before the next capture can start')
  } finally { release() }
  assert.equal(await queued, 'queued')
  assert.deepEqual(events, ['current', 'queued'])
})

test('nested reservations and repeated release cannot resume list rendering prematurely', async () => {
  const releaseA = await reservePreviewRenderer(), releaseB = await reservePreviewRenderer()
  let ran = false
  const queued = enqueuePreview(async () => { ran = true })
  try {
    releaseA(); releaseA()
    await tick()
    assert.equal(ran, false)
  } finally { releaseB() }
  await queued
  assert.equal(ran, true)
})

test('a failed capture rejects its caller and releases the lane for subsequent captures', async () => {
  const failed = enqueuePreview(async () => { throw new Error('bad-model') })
  const next = enqueuePreview(async () => 'next')
  await assert.rejects(failed, /bad-model/)
  assert.equal(await next, 'next')
})

test('preview preparation setting one bounds the whole cold pipeline to one job', async t => {
  configurePreviewConcurrency(1)
  t.after(() => configurePreviewConcurrency(0))
  const gate = deferred()
  let active = 0, peak = 0, starts = 0
  const job = () => enqueuePreviewPreparation(async () => {
    active++; starts++; peak = Math.max(peak, active)
    try { await gate.promise } finally { active-- }
  })
  const pending = [job(), job(), job()]
  try {
    await tick()
    assert.equal(starts, 1)
    assert.equal(peak, 1)
  } finally { gate.resolve(); await Promise.all(pending) }
  assert.equal(starts, 3)
  assert.equal(active, 0)
})

test('zero preview preparation setting uses all logical cores while GPU captures remain serial', async t => {
  t.mock.getter(navigator, 'hardwareConcurrency', () => 3)
  configurePreviewConcurrency(0)
  t.after(() => configurePreviewConcurrency(0))
  const captureGate = deferred()
  let preparations = 0, peakPreparations = 0, starts = 0, captures = 0, peakCaptures = 0
  const pending = Array.from({ length: 6 }, () => enqueuePreviewPreparation(async () => {
    preparations++; starts++; peakPreparations = Math.max(peakPreparations, preparations)
    try {
      await enqueuePreview(async () => {
        captures++; peakCaptures = Math.max(peakCaptures, captures)
        try { await captureGate.promise } finally { captures-- }
      })
    } finally { preparations-- }
  }))
  try {
    await tick()
    assert.equal(starts, 3, 'prepared scenes waiting for GPU must keep their concurrency permits')
    assert.equal(peakPreparations, 3)
    assert.equal(peakCaptures, 1)
  } finally { captureGate.resolve(); await Promise.all(pending) }
  assert.equal(starts, 6)
  assert.equal(peakCaptures, 1)
})

test('a preview concurrency value saved on a larger machine is capped by current logical cores', async t => {
  t.mock.getter(navigator, 'hardwareConcurrency', () => 4)
  t.after(() => configurePreviewConcurrency(0))
  for (const [saved, expected] of [[32, 4], [2, 2]]) {
    configurePreviewConcurrency(saved)
    const gate = deferred()
    let active = 0, peak = 0, starts = 0
    const pending = Array.from({ length: 8 }, () => enqueuePreviewPreparation(async () => {
      active++; starts++; peak = Math.max(peak, active)
      try { await gate.promise } finally { active-- }
    }))
    try {
      await tick()
      assert.equal(starts, expected, `saved=${saved} on a four-core machine`)
      assert.equal(peak, expected)
    } finally { gate.resolve(); await Promise.all(pending) }
    assert.equal(starts, 8)
    assert.equal(active, 0)
  }
})

test('a dialog pauses cold preparation admission and cancelled queued work releases immediately', async () => {
  configurePreviewConcurrency(2)
  const release = await reservePreviewRenderer()
  const abort = new AbortController()
  let starts = 0
  const cancelled = enqueuePreviewPreparation(async () => { starts++ }, abort.signal)
  const rejection = assert.rejects(cancelled, { name: 'AbortError' })
  const next = enqueuePreviewPreparation(async () => { starts++; return 'next' })
  try {
    await tick()
    assert.equal(starts, 0)
    abort.abort()
    await rejection
    assert.equal(starts, 0)
  } finally { release() }
  assert.equal(await next, 'next')
  assert.equal(starts, 1)
  configurePreviewConcurrency(0)
})

test('cancelling a capture waiting behind a dialog does not execute it later', async () => {
  const release = await reservePreviewRenderer()
  const abort = new AbortController()
  let ran = false
  const pending = enqueuePreview(async () => { ran = true }, abort.signal)
  const rejection = assert.rejects(pending, { name: 'AbortError' })
  try { abort.abort(); await rejection }
  finally { release() }
  await tick()
  assert.equal(ran, false)
})

test('failed preparations free their permits and reducing concurrency waits for active work', async () => {
  configurePreviewConcurrency(2)
  const a = deferred(), b = deferred()
  const started = []
  const first = enqueuePreviewPreparation(async () => { started.push('a'); await a.promise; throw new Error('bad-asset') })
  const failure = assert.rejects(first, /bad-asset/)
  const second = enqueuePreviewPreparation(async () => { started.push('b'); await b.promise })
  configurePreviewConcurrency(1)
  const third = enqueuePreviewPreparation(async () => { started.push('c'); return 'c' })
  try {
    a.resolve(); await failure; await tick()
    assert.deepEqual(started, ['a', 'b'])
  } finally { a.resolve(); b.resolve(); await second }
  assert.equal(await third, 'c')
  assert.deepEqual(started, ['a', 'b', 'c'])
  configurePreviewConcurrency(0)
})

test('thumbnail LRU keeps every active lease alive and revokes a released old entry under pressure', t => {
  let urls = 0
  const revoked = []
  t.mock.method(URL, 'createObjectURL', () => `blob:test-${++urls}`)
  t.mock.method(URL, 'revokeObjectURL', url => { revoked.push(url) })
  const image = new Blob([new Uint8Array([1])], { type: 'image/png' })
  putThumbnail('leased', image, 'partial', ['some diagnostic'])
  const a = acquireThumbnail('leased'), b = acquireThumbnail('leased')
  assert.ok(a && b)
  assert.equal(a.url, b.url)
  assert.equal(a.state, 'partial')
  assert.deepEqual(a.diagnostics, ['some diagnostic'])
  const originalUrl = a.url
  for (let i = 0; i < 80; i++) {
    putThumbnail(`pressure:${i}`, image, 'ready', [])
    acquireThumbnail(`pressure:${i}`).release()
  }
  assert.ok(revoked.length > 0)
  assert.equal(revoked.includes(originalUrl), false)
  assert.equal(thumbnailCacheStats().referenced, 1)
  a.release(); a.release()
  assert.equal(thumbnailCacheStats().referenced, 1, 'one of two leases remains active')
  b.release()
  for (let i = 80; i < 84; i++) {
    putThumbnail(`pressure:${i}`, image, 'ready', [])
    acquireThumbnail(`pressure:${i}`).release()
  }
  assert.equal(acquireThumbnail('leased'), null)
  assert.ok(revoked.includes(originalUrl))
  assert.equal(thumbnailCacheStats().referenced, 0)
  assert.ok(thumbnailCacheStats().bytes <= 32 * 1024 * 1024)
})

test('putting an existing thumbnail does not replace its active object URL or allocate a second URL', t => {
  let created = 0
  t.mock.method(URL, 'createObjectURL', () => `blob:duplicate-${++created}`)
  t.mock.method(URL, 'revokeObjectURL', () => {})
  putThumbnail('stable-url', new Blob(['first']), 'ready', [])
  const first = acquireThumbnail('stable-url')
  putThumbnail('stable-url', new Blob(['second']), 'partial', ['ignored replacement'])
  const second = acquireThumbnail('stable-url')
  assert.equal(created, 1)
  assert.equal(first.url, second.url)
  assert.equal(second.state, 'ready')
  first.release(); second.release()
})

test('abandoned captures with no acquired lease still stay inside the thumbnail memory budget', t => {
  let created = 0
  const revoked = []
  t.mock.method(URL, 'createObjectURL', () => `blob:abandoned-${++created}`)
  t.mock.method(URL, 'revokeObjectURL', url => { revoked.push(url) })
  const image = new Blob(['thumbnail'])
  for (let i = 0; i < 100; i++) putThumbnail(`abandoned:${i}`, image, 'ready', [])
  assert.ok(thumbnailCacheStats().bytes <= 32 * 1024 * 1024)
  assert.equal(thumbnailCacheStats().referenced, 0)
  assert.equal(acquireThumbnail('abandoned:0'), null)
  const newest = acquireThumbnail('abandoned:99')
  assert.ok(newest, 'the result awaiting its consumer must survive insertion')
  newest.release()
  assert.ok(revoked.length >= 37)
})

test('clearing removes reusable keys while visible URLs live until their own final lease releases', t => {
  clearThumbnailCache()
  let created = 0
  const revoked = []
  t.mock.method(URL, 'createObjectURL', () => `blob:clear-${++created}`)
  t.mock.method(URL, 'revokeObjectURL', url => { revoked.push(url) })
  putThumbnail('same-key-after-clear', new Blob(['old']), 'ready', [])
  putThumbnail('unreferenced-at-clear', new Blob(['unused']), 'ready', [])
  const a = acquireThumbnail('same-key-after-clear'), b = acquireThumbnail('same-key-after-clear')
  clearThumbnailCache()
  assert.equal(thumbnailCacheStats().entries, 0)
  assert.equal(thumbnailCacheStats().retired, 1)
  assert.equal(acquireThumbnail('same-key-after-clear'), null)
  assert.deepEqual(revoked, ['blob:clear-2'])
  putThumbnail('same-key-after-clear', new Blob(['new']), 'ready', [])
  const fresh = acquireThumbnail('same-key-after-clear')
  assert.notEqual(fresh.url, a.url)
  a.release(); a.release()
  assert.equal(revoked.includes(a.url), false)
  b.release()
  assert.equal(revoked.includes(a.url), true)
  assert.equal(revoked.includes(fresh.url), false, 'releasing an old key must not revoke its replacement')
  assert.equal(thumbnailCacheStats().retired, 0)
  fresh.release(); clearThumbnailCache()
  assert.deepEqual(thumbnailCacheStats(), { entries: 0, retired: 0, referenced: 0, bytes: 0 })
})
