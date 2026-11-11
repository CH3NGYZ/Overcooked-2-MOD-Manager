import assert from 'node:assert/strict'
import { registerHooks } from 'node:module'
import test from 'node:test'

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
    try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
  }
  return next(specifier, context)
} })
const { reservePreviewMemory, previewMemoryStats } = await import('../src/diychef/preview/memory.ts')
const tick = () => new Promise(resolve => setImmediate(resolve))
const mib = 1024 * 1024
const scene = bytes => ({ meshes: [], estimated_bytes: bytes })

test('scene bytes limit decoded assets independently of the all-core CPU setting', async () => {
  const first = await reservePreviewMemory(scene(64 * mib))
  let started = false
  const next = reservePreviewMemory(scene(64 * mib)).then(release => { started = true; return release })
  await tick()
  assert.equal(started, false)
  assert.deepEqual({ bytes: previewMemoryStats().bytes, queued: previewMemoryStats().queued }, { bytes: 64 * mib, queued: 1 })
  first(); first()
  const second = await next
  assert.equal(started, true)
  assert.equal(previewMemoryStats().bytes, 64 * mib)
  second()
  assert.equal(previewMemoryStats().bytes, 0)
})

test('cancelling a scene waiting for memory never allocates and cannot block the next visible scene', async () => {
  const release = await reservePreviewMemory(scene(80 * mib)), abort = new AbortController()
  const cancelled = reservePreviewMemory(scene(40 * mib), abort.signal)
  const rejection = assert.rejects(cancelled, { name: 'AbortError' })
  const next = reservePreviewMemory(scene(8 * mib))
  abort.abort()
  await rejection
  const small = await next
  assert.equal(previewMemoryStats().bytes, 88 * mib)
  small(); release()
  assert.equal(previewMemoryStats().bytes, 0)
  assert.equal(previewMemoryStats().queued, 0)
})

test('a large custom scene runs alone without starvation and fallback descriptors keep finite accounting', async () => {
  const big = await reservePreviewMemory(scene(128 * mib))
  let started = false
  const next = reservePreviewMemory({ meshes: [{}] }).then(release => { started = true; return release })
  await tick()
  assert.equal(started, false)
  big()
  const release = await next
  assert.ok(Number.isFinite(previewMemoryStats().bytes))
  release()
  assert.equal(previewMemoryStats().bytes, 0)
})
