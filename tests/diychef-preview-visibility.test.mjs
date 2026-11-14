import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, getCurrentInstance, nextTick } from 'vue'

const boundary = { observer: null, calls: [], lease: null, requestGate: null, decodeGate: null, decodes: [], canvas: {} }
globalThis.__chefVisibilityBoundary = boundary
globalThis.Image = class {
  set src(url) { this.url = url; if (url) this.onload?.() }
  get src() { return this.url }
  decode() { boundary.decodes.push(this.src); return boundary.decodeGate?.promise ?? Promise.resolve() }
}
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === '@vicons/ionicons5') return { url: 'chef-visibility:icons', shortCircuit: true }
    if (specifier === 'naive-ui') return { url: 'chef-visibility:naive', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (/\/src\/components\/diychef\/Preview(?:Image|Canvas)\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'chef-visibility-test' }).content }
    }
    const sources = {
      'chef-visibility:icons': 'export const PersonOutline = {}',
      'chef-visibility:naive': 'export const NSpin = {}',
      '/src/i18n.ts': 'export const t = key => key',
      '/src/locale/backend.ts': 'export const backendMessageOf = value => String(value)',
      '/src/services/tauri.ts': 'export const api = { diyChefPreview: (...args) => globalThis.__chefVisibilityBoundary.canvas.preview(...args) }',
      '/src/diychef/preview/runtime.ts': `export const chefRenderer = () => globalThis.__chefVisibilityBoundary.canvas.renderer
        export const releaseChefRenderer = () => { globalThis.__chefVisibilityBoundary.canvas.releases++ }`,
      '/src/diychef/preview/scene.ts': 'export const prepareChef = (...args) => globalThis.__chefVisibilityBoundary.canvas.prepare(...args)',
      '/src/tutorial/demoDiyChef.ts': 'export const diyChefDemoImage = () => null; export const isDiyChefDemoSession = () => false',
      '/src/composables/useDiyChefThumbnails.ts': `export const chefThumbnailIdentity = (...args) => JSON.stringify(args)
      export const requestChefThumbnail = async (...args) => {
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
const { default: PreviewCanvas } = await import('../src/components/diychef/PreviewCanvas.vue')
const { putThumbnail, acquireThumbnail, clearThumbnailCache, thumbnailCacheStats } = await import('../src/diychef/preview/cache.ts')
const { pausePreviewWork } = await import('../src/diychef/preview/queue.ts')
const deferred = () => { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j }); return { promise, resolve, reject } }
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

test('same-chef revalidation decodes before swapping, preserves failed images and disposes cancelled or offscreen leases', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const previousObserver = Object.getOwnPropertyDescriptor(globalThis, 'IntersectionObserver')
  Object.defineProperty(globalThis, 'IntersectionObserver', { configurable: true, value: class {
    constructor(callback) { this.callback = callback; boundary.observer = this }
    observe() {} disconnect() {}
  } })
  clearThumbnailCache(); boundary.calls = []; boundary.requestGate = null; boundary.decodeGate = null; boundary.decodes = []
  for (const key of ['old', 'slow', 'new', 'broken', 'other']) putThumbnail(`swap-${key}`, new Blob([key]), 'ready', [])
  let selected = 'old', unavailable = false
  boundary.lease = () => unavailable ? { url: '', state: 'error', release() {} } : acquireThumbnail(`swap-${selected}`)
  const app = renderer.createApp({ ...PreviewImage, render() { getCurrentInstance().setupState.host = { closest() { return null } }; return null } },
    { sessionId: 'swap-initial', chefId: 'fox', hatId: 'hat', name: 'Fox' })
  const errors = []
  app.config.errorHandler = error => errors.push(error)
  try {
    app.mount({})
    const state = app._instance.setupState, props = app._instance.props
    const intersect = value => boundary.observer.callback([{ isIntersecting: value }])
    intersect(true); t.mock.timers.tick(100); await tick()
    const old = state.imageUrl
    assert.ok(old)
    const slow = deferred(); boundary.decodeGate = slow; selected = 'slow'
    props.sessionId = 'swap-slow'; await tick()
    assert.equal(state.imageUrl, old, 'waiting for PNG decode keeps the visible old source')
    assert.equal(state.loading, true)
    assert.equal(thumbnailCacheStats().referenced, 2, 'only the visible old and candidate images are pinned')

    const fresh = deferred(); boundary.decodeGate = fresh; selected = 'new'
    props.hatId = 'another-hat'; await tick()
    assert.equal(state.imageUrl, old, 'a hat change keeps the same chef visible too')
    assert.equal(thumbnailCacheStats().referenced, 2, 'aborting the first decode immediately releases its candidate')
    slow.resolve(); await tick()
    assert.equal(state.imageUrl, old, 'a late old decode cannot replace the current candidate')
    fresh.resolve(); await tick()
    const next = state.imageUrl
    assert.ok(next && next !== old)
    assert.equal(thumbnailCacheStats().referenced, 1, 'after Vue installs the new source, the previous lease is released')

    unavailable = true; props.sessionId = 'swap-request-failed'; await tick()
    assert.equal(state.imageUrl, next)
    assert.equal(state.failed, false, 'failure cannot add an unavailable overlay over an existing good image')
    unavailable = false
    const broken = deferred(); boundary.decodeGate = broken; selected = 'broken'
    props.sessionId = 'swap-decode-failed'; await tick()
    broken.reject(new Error('invalid PNG')); await tick()
    assert.equal(state.imageUrl, next)
    assert.equal(thumbnailCacheStats().referenced, 1)

    const cancelled = deferred(); boundary.decodeGate = cancelled; selected = 'slow'
    props.sessionId = 'swap-offscreen'; await tick()
    intersect(false); await tick()
    assert.equal(state.imageUrl, undefined)
    assert.equal(thumbnailCacheStats().referenced, 0, 'leaving the IO buffer releases both visible and decoding images')
    cancelled.resolve(); await tick()
    assert.equal(state.imageUrl, undefined)

    boundary.decodeGate = null
    intersect(true); t.mock.timers.tick(100); await tick()
    assert.ok(state.imageUrl)
    const other = deferred(); boundary.decodeGate = other; selected = 'other'
    props.chefId = 'wolf'; await tick()
    assert.equal(state.imageUrl, undefined, 'another chef never inherits this chef picture')
    intersect(false); intersect(true)
    assert.equal(state.imageUrl, undefined, 'another chef never inherits a remembered offscreen key either')
    other.resolve(); await tick()
    assert.deepEqual(errors, [])
  } finally {
    app.unmount(); t.mock.timers.tick(200); await tick()
    assert.equal(thumbnailCacheStats().referenced, 0)
    clearThumbnailCache(); boundary.decodeGate = null; boundary.requestGate = null
    if (previousObserver) Object.defineProperty(globalThis, 'IntersectionObserver', previousObserver)
    else delete globalThis.IntersectionObserver
  }
})

test('a cross-grid remount presents its decoded PNG before visibility or metadata, then releases first-observed offscreen cards', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const previousObserver = globalThis.IntersectionObserver
  globalThis.IntersectionObserver = class {
    constructor(callback) { this.callback = callback; boundary.observer = this }
    observe() {} disconnect() {}
  }
  clearThumbnailCache(); boundary.calls = []; boundary.requestGate = null; boundary.decodeGate = null
  putThumbnail('remount-fixture', new Blob(['decoded png']), 'ready', [])
  boundary.lease = () => acquireThumbnail('remount-fixture')
  const create = (props = {}) => renderer.createApp({ ...PreviewImage, render() {
    getCurrentInstance().setupState.host = { closest() { return null } }; return null
  } }, { sessionId: 'target-resource-session', chefId: 'fox', hatId: 'hat', name: 'Fox', ...props })
  let app = create(), others = [], resume = () => {}
  try {
    app.mount({}); boundary.observer.callback([{ isIntersecting: true }])
    t.mock.timers.tick(100); await tick()
    const ready = app._instance.setupState.imageUrl
    assert.ok(ready)
    const reads = boundary.calls.length
    resume = pausePreviewWork()
    // The source's unmount may precede destination mounting in Vue's patch.
    app.unmount(); app = create(); app.mount({})
    assert.equal(app._instance.setupState.imageUrl, ready, 'the first destination render must not contain the person placeholder')
    boundary.observer.callback([{ isIntersecting: true }])
    t.mock.timers.tick(150); await tick()
    assert.equal(boundary.calls.length, reads, 'cached presentation does not decode, read metadata or bypass the drag pause')
    for (const props of [{ chefId: 'wolf' }, { hatId: 'other' }, { sessionId: 'another-target-or-resources' }]) {
      const other = create(props); other.mount({}); others.push(other)
      assert.equal(other._instance.setupState.imageUrl, undefined, 'a different chef, hat, target or resource session cannot borrow the presentation')
    }
    const offscreen = create(); offscreen.mount({}); others.push(offscreen)
    assert.equal(offscreen._instance.setupState.imageUrl, ready)
    boundary.observer.callback([{ isIntersecting: false }])
    assert.equal(offscreen._instance.setupState.imageUrl, undefined, 'the initial false IO notification releases a speculative warm lease')
    resume(); t.mock.timers.tick(100); await tick()
    assert.equal(boundary.calls.length, reads + 1, 'the visible replacement still revalidates when drag pause ends')
    assert.equal(app._instance.setupState.imageUrl, ready)
  } finally {
    app.unmount(); for (const other of others) other.unmount(); resume()
    t.mock.timers.tick(200); await tick()
    assert.equal(thumbnailCacheStats().referenced, 0)
    clearThumbnailCache(); globalThis.IntersectionObserver = previousObserver
  }
})

test('dialog revalidation preserves the drawing buffer through delays and failures without keeping the old 3D model', async t => {
  let gate = null, current = null, pixels = '', preparedCount = 0, disposedCount = 0
  const previousFrame = globalThis.requestAnimationFrame, previousCancel = globalThis.cancelAnimationFrame
  globalThis.requestAnimationFrame = () => 1
  globalThis.cancelAnimationFrame = () => {}
  const canvas = boundary.canvas = {
    releases: 0,
    async preview() { const pending = gate; if (pending) return pending.promise; return { status: 'complete', meshes: [{}], diagnostics: [] } },
    async prepare() { preparedCount++; let disposed = false; return { dispose() { if (!disposed) { disposed = true; disposedCount++ } } } },
    renderer: {
      canvas: { remove() {} },
      clear(preserve) { current?.dispose(); current = null; if (!preserve) pixels = '' },
      setChef(prepared) { this.clear(); current = prepared; pixels = `model-${preparedCount}` },
      render() {},
    },
  }
  const app = renderer.createApp({ ...PreviewCanvas, render() { getCurrentInstance().setupState.host = { prepend() {} }; return null } },
    { sessionId: 'canvas-initial', chefId: 'fox', hatId: 'hat' })
  const errors = []
  app.config.errorHandler = error => errors.push(error)
  try {
    app.mount({}); await tick()
    const state = app._instance.setupState, props = app._instance.props
    assert.equal(state.state, 'ready'); assert.equal(state.hasPixels, true)
    const first = pixels
    gate = deferred(); props.sessionId = 'canvas-waiting'; await tick()
    assert.equal(state.state, 'loading'); assert.equal(state.hasPixels, true, 'loading can leave the existing drawing buffer unobscured')
    assert.equal(pixels, first)
    assert.equal(disposedCount, 1, 'the old 3D model is released before loading another one')
    gate.reject(new Error('refresh failed')); await tick()
    assert.equal(state.state, 'error'); assert.equal(pixels, first)

    gate = deferred(); props.hatId = 'new-hat'; await tick()
    assert.equal(pixels, first)
    gate.resolve({ status: 'complete', meshes: [{}], diagnostics: [] }); await tick()
    assert.equal(state.state, 'ready'); assert.notEqual(pixels, first)
    gate = deferred(); props.chefId = 'wolf'; await tick()
    assert.equal(state.hasPixels, false, 'a different chef must start with its own initial placeholder')
    assert.equal(pixels, '')
    gate.resolve({ status: 'unavailable', meshes: [], diagnostics: [] }); await tick()
    assert.equal(state.state, 'error'); assert.equal(pixels, '')
    assert.deepEqual(errors, [])
  } finally {
    app.unmount(); await tick()
    assert.equal(disposedCount, preparedCount)
    assert.equal(canvas.releases, 1)
    globalThis.requestAnimationFrame = previousFrame
    globalThis.cancelAnimationFrame = previousCancel
  }
})
