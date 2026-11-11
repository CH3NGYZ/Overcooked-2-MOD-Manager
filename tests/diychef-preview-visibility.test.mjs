import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, getCurrentInstance, nextTick } from 'vue'

const boundary = { observer: null, calls: [], lease: null }
globalThis.__chefVisibilityBoundary = boundary
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === '@vicons/ionicons5') return { url: 'chef-visibility:icons', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/components/diychef/PreviewImage.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'chef-visibility-test' }).content }
    }
    const sources = {
      'chef-visibility:icons': 'export const PersonOutline = {}',
      '/src/i18n.ts': 'export const t = key => key',
      '/src/tutorial/demoDiyChef.ts': 'export const diyChefDemoImage = () => null; export const isDiyChefDemoSession = () => false',
      '/src/composables/useDiyChefThumbnails.ts': `export const requestChefThumbnail = async (...args) => {
        globalThis.__chefVisibilityBoundary.calls.push(args)
        return globalThis.__chefVisibilityBoundary.lease()
      }`,
    }
    for (const [suffix, source] of Object.entries(sources)) {
      if (url.endsWith(suffix)) return { format: 'module', shortCircuit: true, source }
    }
    return next(url, context)
  },
})
const { default: PreviewImage } = await import('../src/components/diychef/PreviewImage.vue')
const { putThumbnail, acquireThumbnail, clearThumbnailCache, thumbnailCacheStats } = await import('../src/diychef/preview/cache.ts')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }

test('the real portrait lifecycle ignores passing cards, reuses a warm image immediately, and releases offscreen leases', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const previousObserver = Object.getOwnPropertyDescriptor(globalThis, 'IntersectionObserver')
  Object.defineProperty(globalThis, 'IntersectionObserver', { configurable: true, value: class {
    constructor(callback, options) { this.callback = callback; this.options = options; boundary.observer = this }
    observe() {} disconnect() { this.disconnected = true }
  } })
  clearThumbnailCache()
  boundary.calls = []
  putThumbnail('visibility-fixture', new Blob(['png']), 'ready', [])
  boundary.lease = () => acquireThumbnail('visibility-fixture')
  const element = {}, errors = []
  const app = renderer.createApp({ ...PreviewImage, render() { getCurrentInstance().setupState.host = element; return null } },
    { sessionId: 'visibility-session', chefId: 'fox', hatId: 'hat', name: 'Fox' })
  app.config.errorHandler = error => errors.push(error)
  try {
    app.mount({})
    assert.equal(boundary.observer.options.rootMargin, '240px 80px', 'retain the adjacent row while bounding offscreen image references')
    const state = app._instance.setupState
    const intersect = value => boundary.observer.callback([{ isIntersecting: value }])
    for (let i = 0; i < 25; i++) { intersect(true); intersect(false) }
    t.mock.timers.tick(200)
    await tick()
    assert.equal(boundary.calls.length, 0, 'quickly passing cards must never reach metadata IPC')
    intersect(true)
    t.mock.timers.tick(100)
    await tick()
    assert.equal(boundary.calls.length, 1)
    assert.ok(state.imageUrl)
    assert.equal(thumbnailCacheStats().referenced, 1)
    const url = state.imageUrl
    intersect(false)
    assert.equal(thumbnailCacheStats().referenced, 0, 'offscreen cards cannot pin every decoded image in memory')
    intersect(true)
    assert.equal(state.imageUrl, url, 'use the existing LRU image immediately while its metadata is revalidated')
    assert.equal(boundary.calls.length, 1)
    t.mock.timers.tick(100)
    await tick()
    assert.equal(boundary.calls.length, 2, 'visibility settling must not turn into a time based fingerprint cache')
    assert.deepEqual(errors, [])
  } finally {
    app.unmount()
    t.mock.timers.tick(200)
    await tick()
    assert.equal(thumbnailCacheStats().referenced, 0)
    clearThumbnailCache()
    if (previousObserver) Object.defineProperty(globalThis, 'IntersectionObserver', previousObserver)
    else delete globalThis.IntersectionObserver
  }
})
