import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, getCurrentInstance, nextTick } from 'vue'

const boundary = { observer: null, calls: [], lease: null, requestGate: null }
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
        const gate = globalThis.__chefVisibilityBoundary.requestGate
        if (gate) {
          const signal = args.at(-1)
          let cancel
          const aborted = new Promise((_, reject) => { cancel = () => reject(new DOMException('Aborted', 'AbortError')); signal?.addEventListener('abort', cancel, { once: true }) })
          try { await Promise.race([gate.promise, aborted]) } finally { signal?.removeEventListener('abort', cancel) }
        }
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
const { pausePreviewWork } = await import('../src/diychef/preview/queue.ts')
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})

test('paused portraits keep the ready lease, abort in-flight reads without an error, and resume only visible cards', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const previousObserver = Object.getOwnPropertyDescriptor(globalThis, 'IntersectionObserver')
  Object.defineProperty(globalThis, 'IntersectionObserver', { configurable: true, value: class {
    constructor(callback, options) { this.callback = callback; this.options = options; boundary.observer = this }
    observe() {} disconnect() { this.disconnected = true }
  } })
  clearThumbnailCache(); boundary.calls = []; boundary.requestGate = null
  putThumbnail('pause-fixture', new Blob(['png']), 'ready', [])
  boundary.lease = () => acquireThumbnail('pause-fixture')
  const element = { closest() { return {} } }, errors = []
  const app = renderer.createApp({ ...PreviewImage, render() { getCurrentInstance().setupState.host = element; return null } },
    { sessionId: 'pause-session', chefId: 'fox', hatId: 'hat', name: 'Fox' })
  app.config.errorHandler = error => errors.push(error)
  const release = pausePreviewWork()
  try {
    app.mount({})
    const state = app._instance.setupState
    const intersect = value => boundary.observer.callback([{ isIntersecting: value }])
    intersect(true)
    t.mock.timers.tick(120); await tick()
    assert.equal(boundary.calls.length, 0, 'paused cards cannot admit even a warm-cache request')
    release()
    t.mock.timers.tick(100); await tick()
    assert.equal(boundary.calls.length, 1)
    const oldUrl = state.imageUrl
    assert.ok(oldUrl)

    const gate = deferred(); boundary.requestGate = gate
    intersect(false); intersect(true)
    t.mock.timers.tick(100); await tick()
    assert.equal(boundary.calls.length, 2)
    assert.equal(state.imageUrl, oldUrl, 'revalidation keeps the previous ready image visible')
    const pauseAgain = pausePreviewWork()
    try {
      assert.equal(boundary.calls.at(-1).at(-1).aborted, true)
      assert.equal(state.imageUrl, oldUrl)
      assert.equal(thumbnailCacheStats().referenced, 1, 'pausing never releases a ready image')
      gate.resolve()
      await tick()
      assert.equal(state.imageUrl, oldUrl)
      assert.equal(state.failed, false, 'the cancelled generation cannot display an unavailable warning')
      assert.equal(state.loading, false)
      assert.deepEqual(errors, [])
    } finally { pauseAgain() }
    t.mock.timers.tick(89); await tick()
    assert.equal(boundary.calls.length, 2, 'resuming still observes the original 90ms visibility settling')
    t.mock.timers.tick(1); await tick()
    assert.equal(boundary.calls.length, 3, 'release revalidates only the still-visible card')
  } finally {
    release()
    app.unmount(); t.mock.timers.tick(200); await tick()
    assert.equal(thumbnailCacheStats().referenced, 0)
    clearThumbnailCache(); boundary.requestGate = null
    if (previousObserver) Object.defineProperty(globalThis, 'IntersectionObserver', previousObserver)
    else delete globalThis.IntersectionObserver
  }
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
  const scrollRoot = {}
  const element = { closest(selector) { assert.equal(selector, '.n-scrollbar-container'); return scrollRoot } }, errors = []
  const app = renderer.createApp({ ...PreviewImage, render() { getCurrentInstance().setupState.host = element; return null } },
    { sessionId: 'visibility-session', chefId: 'fox', hatId: 'hat', name: 'Fox' })
  app.config.errorHandler = error => errors.push(error)
  try {
    app.mount({})
    assert.equal(boundary.observer.options.root, scrollRoot, 'the scrollport must own the margin so its clipping does not prevent preloading')
    assert.equal(boundary.observer.options.rootMargin, '480px 80px', 'prepare the next two rows before cards enter the scrollport')
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

test('pausing clears settling timers and resumes current visible props without replaying offscreen requests', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const previousObserver = Object.getOwnPropertyDescriptor(globalThis, 'IntersectionObserver')
  Object.defineProperty(globalThis, 'IntersectionObserver', { configurable: true, value: class {
    constructor(callback) { this.callback = callback; boundary.observer = this }
    observe() {} disconnect() {}
  } })
  clearThumbnailCache(); boundary.calls = []; boundary.requestGate = null
  putThumbnail('settle-pause-fixture', new Blob(['png']), 'ready', [])
  boundary.lease = () => acquireThumbnail('settle-pause-fixture')
  const element = { closest() { return null } }
  const app = renderer.createApp({ ...PreviewImage, render() { getCurrentInstance().setupState.host = element; return null } },
    { sessionId: 'settle-pause-session', chefId: 'fox', hatId: 'hat', name: 'Fox' })
  let resume = () => {}, resumeNested = () => {}
  try {
    app.mount({})
    const intersect = value => boundary.observer.callback([{ isIntersecting: value }])
    intersect(true); t.mock.timers.tick(40)
    resume = pausePreviewWork()
    t.mock.timers.tick(200); await tick()
    assert.equal(boundary.calls.length, 0, 'pause must clear an already scheduled settling timer')
    intersect(false); resume()
    t.mock.timers.tick(200); await tick()
    assert.equal(boundary.calls.length, 0, 'an offscreen card must not resume its obsolete pending load')

    resume = pausePreviewWork(); resumeNested = pausePreviewWork()
    intersect(true)
    app._instance.props.chefId = 'wolf'
    await tick()
    t.mock.timers.tick(200); await tick()
    assert.equal(boundary.calls.length, 0, 'prop changes and new visibility are both gated')
    resume(); resume()
    t.mock.timers.tick(200); await tick()
    assert.equal(boundary.calls.length, 0, 'nested release cannot resume the portrait early')
    resumeNested()
    t.mock.timers.tick(89); await tick()
    assert.equal(boundary.calls.length, 0)
    t.mock.timers.tick(1); await tick()
    assert.equal(boundary.calls.length, 1)
    assert.equal(boundary.calls[0][1], 'wolf', 'only current visible props may reach the thumbnail request')

    resume = pausePreviewWork()
    intersect(false); intersect(true)
    assert.ok(app._instance.setupState.imageUrl, 'in-memory LRU reuse remains available while paused')
    assert.equal(boundary.calls.length, 1)
  } finally {
    app.unmount(); resume(); resumeNested()
    t.mock.timers.tick(200); await tick()
    assert.equal(boundary.calls.length, 1, 'an unmounted visible card cannot restart after resume')
    assert.equal(thumbnailCacheStats().referenced, 0)
    clearThumbnailCache()
    if (previousObserver) Object.defineProperty(globalThis, 'IntersectionObserver', previousObserver)
    else delete globalThis.IntersectionObserver
  }
})
