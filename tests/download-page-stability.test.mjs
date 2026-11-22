import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef, toRaw } from 'vue'

const boundary = { listener: null, calls: [], unlistens: 0, registration: null }
globalThis.__downloadPageBoundary = boundary
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'download-page:naive', shortCircuit: true }
    if (context.parentURL === 'download-page:naive') return next(specifier, { ...context, parentURL: import.meta.url })
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const module = source => ({ format: 'module', shortCircuit: true, source })
    if (/\/(DownloadStrip|SourceDirectory|SourceGroupSlot)\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'), { filename: new URL(url).pathname })
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'download-page-test', inlineTemplate: !url.endsWith('/DownloadStrip.vue') }).content }
    }
    if (url === 'download-page:naive') return module(`
      import { h } from 'vue'
      const leaf = tag => ({props:['name'],setup:(props,{slots}) => () => h(tag,{name:props.name},[slots.header?.(),slots.default?.()])})
      export const NCollapse=leaf('collapse'),NCollapseItem=leaf('collapse-item'),NButton=leaf('button'),
        NTag=leaf('tag'),NAlert=leaf('alert'),NProgress=leaf('progress'),NSpin=leaf('spin')`)
    if (url.endsWith('/src/services/tauri.ts')) return module(`
      const state=globalThis.__downloadPageBoundary
      export const watchDownloadProgress=async listener => {state.listener=listener;await state.registration;return () => state.unlistens++}
      export const api={cancelDownload:async () => {state.calls.push('cancelDownload')}}`)
    if (url.endsWith('/src/composables/useDashboardStore.ts')) return module(`
      export const useDashboard=() => ({refresh:async () => globalThis.__downloadPageBoundary.calls.push('refresh')})`)
    if (url.endsWith('/src/i18n.ts')) return module('export const t=key=>key')
    if (url.endsWith('/src/locale/backend.ts')) return module('export const translateBackendMessage=String;export const backendMessageOf=String')
    if (/\/src\/components\/(DownloadVolumes|AppModal)\.vue$/.test(url)) return module('export default {}')
    if (url.endsWith('/src/components/SourceTitle.vue')) return module(`export default {props:['label'],template:'<span>{{label}}</span>'}`)
    return next(url, context)
  },
})

const { default: DownloadStrip } = await import('../src/components/DownloadStrip.vue')
const { default: SourceDirectory } = await import('../src/components/SourceDirectory.vue')
const { buildSourceGroupTree } = await import('../src/composables/useSourceGroupTree.ts')
const { beginDownloadTask, endDownloadTask, downloadTaskState, updateDownloadProgress } = await import('../src/downloadTask.ts')
const { appUpdateState } = await import('../src/appUpdateState.ts')
DownloadStrip.render = () => null
const renderer = createRenderer({
  createElement: tag => ({ tag, props: {}, children: [] }),
  createText: text => ({ text }), createComment: () => ({}),
  patchProp: (node, key, _, value) => { node.props[key] = value },
  insert(node, parent, anchor) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = parent
    const before = parent.children.indexOf(anchor)
    parent.children.splice(before < 0 ? parent.children.length : before, 0, node)
  },
  remove(node) { if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1) },
  setText: (node, text) => { node.text = text },
  setElementText(node, text) { node.text = text; node.children = [] },
  parentNode: node => node.parent,
  nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
})
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
function all(node, predicate) { return [...(predicate(node) ? [node] : []), ...(node.children ?? []).flatMap(child => all(child, predicate))] }
const textOf = node => (node?.text ?? '') + (node?.children?.map(textOf).join('') ?? '')

test('download progress completion and cancellation never reload local lists before installation', async () => {
  boundary.calls = []
  const root = { children: [] }, app = renderer.createApp(DownloadStrip)
  app.mount(root)
  await tick()
  const state = app._instance.setupState
  try {
    for (const label of ['MOD update', 'Map update', 'Framework install']) {
      const token = beginDownloadTask({ label, retry: () => {} })
      await tick()
      boundary.listener({ downloaded: 5, total: 10, finished: false })
      assert.equal(state.progress.downloaded, 5)
      boundary.listener({ downloaded: 10, total: 10, finished: true })
      assert.equal(state.progress, null)
      assert.deepEqual(boundary.calls, [], `${label}: asset staging does not mean files were installed`)
      endDownloadTask(token)
    }
    await state.cancel()
    assert.deepEqual(boundary.calls, [], 'a closed task cannot cancel a later request')
  } finally { app.unmount() }
  assert.equal(boundary.unlistens, 1)
})

test('one ordinary progress subscription handles cancellation and does not retain previous bytes', async () => {
  boundary.calls = []
  const root = { children: [] }, app = renderer.createApp(DownloadStrip)
  app.mount(root)
  await tick()
  const state = app._instance.setupState
  try {
    const chain = beginDownloadTask({ label: 'first download', retry() {} })
    assert.equal(state.progress, null, 'preparing begins with indeterminate stage, not previous percentages')
    boundary.listener({ label: 'package A', downloaded: 25, total: 100, speed_bytes: 2048, finished: false })
    assert.equal(state.percent, 25)
    assert.equal(state.rate, '2 KB/s')
    assert.equal(state.visible, true)
    await state.cancel()
    assert.deepEqual(boundary.calls, ['cancelDownload'], 'cancel must reach the real IPC boundary')
    boundary.listener({ label: 'package A', downloaded: 50, total: 100, finished: false })
    assert.equal(state.progress, null, 'queued bytes do not reopen a cancelled download')
    endDownloadTask(chain)
    assert.equal(state.visible, false)

    const ordinary = beginDownloadTask({ label: 'normal', retry() {} })
    boundary.listener({ label: 'package B', downloaded: 40, total: 100, speed_bytes: 1024, finished: false })
    boundary.listener({ label: 'package A', downloaded: 100, total: 100, finished: true })
    assert.equal(state.percent, 40, 'differently labelled old finished event does not erase current bytes')
    assert.equal(downloadTaskState.running.id, ordinary)
    // The public transition owns this readonly state; raw reset is fixture cleanup only.
    const { beginAppUpdateCheck, finishAppUpdateCheck, finishBackgroundAppUpdate, beginStagedAppUpdate } = await import('../src/appUpdateState.ts')
    const update = { version: 'fixture', current_version: 'fixture', background_install: true, source_channel: 'fixture' }
    beginAppUpdateCheck()
    finishAppUpdateCheck({ update, all_failed: false })
    assert.equal(state.visible, true, 'background updates do not hide ordinary downloads')
    finishBackgroundAppUpdate({ update })
    assert.equal(beginStagedAppUpdate(), true)
    await state.cancel()
    assert.equal(state.visible, false)
    assert.deepEqual(boundary.calls, ['cancelDownload'], 'update installation remains non-cancellable')
    toRaw(appUpdateState).stage = 'idle'
    endDownloadTask(ordinary)
  } finally { toRaw(appUpdateState).stage = 'idle'; app.unmount() }
})

test('unmount releases a late subscription and ignores events after disposal', async () => {
  let finishRegistration
  boundary.registration = new Promise(resolve => { finishRegistration = resolve })
  const before = boundary.unlistens, app = renderer.createApp(DownloadStrip)
  app.mount({ children: [] })
  app.unmount()
  finishRegistration()
  await tick()
  assert.equal(boundary.unlistens, before + 1)
  const token = beginDownloadTask({ label: 'new owner', retry() {} })
  boundary.listener({ label: 'disposed', downloaded: 5, finished: false })
  assert.equal(downloadTaskState.progress, null)
  endDownloadTask(token)
  boundary.registration = null
})

test('the overlay delegates cancellation without hiding a caller-owned download when cancellation fails', async () => {
  boundary.calls = []
  const app = renderer.createApp(DownloadStrip)
  app.mount({ children: [] })
  await tick()
  const state = app._instance.setupState
  const token = beginDownloadTask({ label: 'chain', retry() {}, progressSource: 'caller', cancel: async () => {
    boundary.calls.push('cancel-chain')
    throw new Error('update.inProgress')
  } })
  try {
    updateDownloadProgress({ label: 'chain file', downloaded: 6, total: 10, speed_bytes: 2048, finished: false }, token)
    assert.equal(state.visible, true)
    await state.cancel()
    assert.deepEqual(boundary.calls, ['cancel-chain'])
    updateDownloadProgress({ label: 'chain file', downloaded: 9, total: 10, speed_bytes: 2048, finished: false }, token)
    assert.equal(state.visible, true)
    assert.equal(state.percent, 90)
  } finally { endDownloadTask(token); app.unmount() }
})

test('both official index roots omit the entire heading while child sources and root errors remain reactive', async () => {
  for (const rootId of ['official-map-index', 'official-repository-index']) {
    const order = shallowRef([
      { id: rootId, label: 'Index heading to omit', status: 'ready' },
      { id: 'author', parentId: rootId, label: 'Author source', status: 'ready' },
      { id: 'nested', parentId: 'author', label: 'Nested source', status: 'loading' },
      { id: 'empty', parentId: rootId, label: 'Empty source', status: 'ready' },
    ])
    const cards = shallowRef([{ sourceId: 'author', group: 'maps', groupLabel: 'Maps', item: 'map' }])
    const root = { children: [] }
    const app = renderer.createApp({ setup: () => () => h(SourceDirectory,
      { nodes: buildSourceGroupTree(cards.value, { order: order.value, ungrouped: { id: 'none', label: 'Other' } }),
        transparentRoot: rootId, expanded: ['author', 'nested'], busy: () => false },
      { default: ({ node }) => h('source-items', { id: node.id }, node.groups.flatMap(group => group.items ?? []).join(',')) }) })
    app.mount(root)
    try {
      assert.deepEqual(all(root, node => node.tag === 'collapse-item').map(node => node.props.name), ['author', 'nested'])
      assert.equal(textOf(root).includes('Index heading to omit'), false)
      assert.equal(textOf(root).includes('map'), true)
      const author = all(root, node => node.tag === 'collapse-item' && node.props.name === 'author')[0]
      order.value = order.value.map(node => node.id === rootId ? { ...node, status: 'error', error: 'root offline' } : node)
      await nextTick()
      assert.equal(all(root, node => node.tag === 'alert').map(textOf).join(''), 'root offlinecommon.action.retry')
      assert.strictEqual(all(root, node => node.tag === 'collapse-item' && node.props.name === 'author')[0], author)
      order.value = [{ id: rootId, label: 'Index heading to omit', status: 'loading' }]
      cards.value = [{ sourceId: rootId, group: 'root', groupLabel: 'Root', item: 'direct root item' }]
      await nextTick()
      assert.equal(all(root, node => node.tag === 'collapse-item').length, 0)
      assert.equal(textOf(root), 'common.loadingdirect root item')
      order.value = [{ ...order.value[0], status: 'ready' }]
      await nextTick()
      assert.equal(textOf(root), 'direct root item')
      cards.value = []
      await nextTick()
      assert.equal(textOf(root), '')
    } finally { app.unmount() }
  }
})
