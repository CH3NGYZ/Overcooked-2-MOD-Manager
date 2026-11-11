import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { BufferGeometry, MeshStandardMaterial, Texture, TextureLoader } from 'three'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal module resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/services/tauri.ts')) return { format: 'module', shortCircuit: true, source: `
      export const api = { diyChefPreviewAsset: (...args) => globalThis.__chefSceneAsset(...args) }
    ` }
    return next(url, context)
  },
})
const { prepareChef } = await import('../src/diychef/preview/scene.ts')
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
const tick = () => new Promise(resolve => setImmediate(resolve))

function geometryBytes() {
  const bytes = new ArrayBuffer(3 * 32 + 3 * 4)
  const view = new DataView(bytes)
  const vertices = [[-1, 0, 0, 0, 0, 1, 0, 0], [1, 0, 0, 0, 0, 1, 1, 0], [0, 1, 0, 0, 0, 1, 0, 1]]
  vertices.flat().forEach((value, i) => view.setFloat32(i * 4, value, true))
  ;[0, 1, 2].forEach((value, i) => view.setUint32(96 + i * 4, value, true))
  return bytes
}
function description(texture = false, meshes = 1) {
  return { fingerprint: 'scene-fixture', status: 'complete', diagnostics: [], meshes: Array.from({ length: meshes }, (_, i) => ({
    name: `part-${i}`, geometry_asset: `geometry-${i}`, vertex_count: 3, index_count: 3,
    texture_asset: texture ? 'texture' : null, texture_scale: [1, 1], texture_offset: [0, 0], material_floats: {},
  })) }
}
function disposals(t) {
  const counts = { geometry: 0, material: 0, texture: 0 }
  for (const [name, type] of [['geometry', BufferGeometry], ['material', MeshStandardMaterial], ['texture', Texture]]) {
    const original = type.prototype.dispose
    t.mock.method(type.prototype, 'dispose', function () { counts[name]++; return original.call(this) })
  }
  return counts
}

test('preparing a scene releases each real geometry, material and shared texture exactly once', async t => {
  const counts = disposals(t), revoked = []
  globalThis.__chefSceneAsset = async (_session, id) => id === 'texture' ? new ArrayBuffer(4) : geometryBytes()
  t.mock.method(URL, 'createObjectURL', () => 'blob:prepared-texture')
  t.mock.method(URL, 'revokeObjectURL', value => revoked.push(value))
  const load = t.mock.method(TextureLoader.prototype, 'loadAsync', async () => new Texture())
  const prepared = await prepareChef('session', description(true, 2))
  assert.equal(prepared.group.children.length, 2)
  assert.equal(load.mock.callCount(), 1, 'one scene shares a texture across its meshes')
  assert.deepEqual(revoked, ['blob:prepared-texture'])
  prepared.dispose(); prepared.dispose()
  assert.equal(prepared.group.children.length, 0)
  assert.deepEqual(counts, { geometry: 2, material: 2, texture: 1 })
})

test('cancellation during asset reading stops before decoding the next geometry and disposes earlier meshes', async t => {
  const counts = disposals(t), gate = deferred(), abort = new AbortController()
  globalThis.__chefSceneAsset = async (_session, id) => id === 'geometry-1' ? gate.promise : geometryBytes()
  const pending = prepareChef('session', description(false, 2), abort.signal)
  const rejected = assert.rejects(pending, { name: 'AbortError' })
  await tick()
  abort.abort(); gate.resolve(geometryBytes())
  await rejected
  assert.deepEqual(counts, { geometry: 1, material: 1, texture: 0 })
})

test('cancellation while a texture decodes disposes the late texture and revokes its URL', async t => {
  const counts = disposals(t), gate = deferred(), abort = new AbortController(), revoked = []
  globalThis.__chefSceneAsset = async (_session, id) => id === 'texture' ? new ArrayBuffer(4) : geometryBytes()
  t.mock.method(URL, 'createObjectURL', () => 'blob:cancelled-texture')
  t.mock.method(URL, 'revokeObjectURL', value => revoked.push(value))
  t.mock.method(TextureLoader.prototype, 'loadAsync', () => gate.promise)
  const pending = prepareChef('session', description(true), abort.signal)
  const rejected = assert.rejects(pending, { name: 'AbortError' })
  await tick()
  abort.abort(); gate.resolve(new Texture())
  await rejected
  assert.deepEqual(revoked, ['blob:cancelled-texture'])
  assert.deepEqual(counts, { geometry: 1, material: 0, texture: 1 })
})

test('an asset failure disposes preceding scene parts and permits a later preparation', async t => {
  const counts = disposals(t)
  globalThis.__chefSceneAsset = async (_session, id) => {
    if (id === 'geometry-1') throw new Error('missing-geometry')
    return geometryBytes()
  }
  await assert.rejects(prepareChef('session', description(false, 2)), /missing-geometry/)
  assert.deepEqual(counts, { geometry: 1, material: 1, texture: 0 })
  const next = await prepareChef('session', description())
  assert.equal(next.group.children.length, 1)
  next.dispose()
  assert.deepEqual(counts, { geometry: 2, material: 2, texture: 0 })
})
