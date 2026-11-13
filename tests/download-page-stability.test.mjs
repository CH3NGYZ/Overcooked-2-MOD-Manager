import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'

const boundary = { listener: null, calls: [], unlistens: 0 }
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
        NTag=leaf('tag'),NAlert=leaf('alert'),NProgress=leaf('progress')`)
    if (url.endsWith('/src/services/tauri.ts')) return module(`
      const state=globalThis.__downloadPageBoundary
      export const watchDownloadProgress=async listener => {state.listener=listener;return () => state.unlistens++}
      export const api={cancelDownload:async () => {state.calls.push('cancelDownload')}}`)
    if (url.endsWith('/src/composables/useDashboardStore.ts')) return module(`
      export const useDashboard=() => ({refresh:async () => globalThis.__downloadPageBoundary.calls.push('refresh')})`)
    if (url.endsWith('/src/i18n.ts')) return module('export const t=key=>key')
    if (url.endsWith('/src/locale/backend.ts')) return module('export const translateBackendMessage=String;export const backendMessageOf=String')
    if (url.endsWith('/src/components/DownloadVolumes.vue')) return module('export default {}')
    if (url.endsWith('/src/components/SourceTitle.vue')) return module(`export default {props:['label'],template:'<span>{{label}}</span>'}`)
    return next(url, context)
  },
})

const { default: DownloadStrip } = await import('../src/components/DownloadStrip.vue')
const { default: SourceDirectory } = await import('../src/components/SourceDirectory.vue')
const { beginDownloadTask, endDownloadTask } = await import('../src/downloadTask.ts')
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
    assert.deepEqual(boundary.calls, ['cancelDownload'])
  } finally { app.unmount() }
  assert.equal(boundary.unlistens, 1)
})

test('both official index roots omit the entire heading while child sources and root errors remain reactive', async () => {
  for (const rootId of ['official-map-index', 'official-repository-index']) {
    const nodes = shallowRef([
      { id: rootId, label: 'Index heading to omit', count: 1, groups: [], status: 'ready' },
      { id: 'author', parentId: rootId, label: 'Author source', count: 1, groups: [{ items: ['map'] }] },
      { id: 'nested', parentId: 'author', label: 'Nested source', count: 0, groups: [] },
      { id: 'community', label: 'Community source', count: 0, groups: [] },
    ])
    const root = { children: [] }
    const app = renderer.createApp({ setup: () => () => h(SourceDirectory,
      { nodes: nodes.value, transparentRoot: rootId, expanded: ['author', 'nested', 'community'], busy: () => false },
      { default: ({ node }) => h('source-items', { id: node.id }, node.groups.flatMap(group => group.items ?? []).join(',')) }) })
    app.mount(root)
    try {
      assert.deepEqual(all(root, node => node.tag === 'collapse-item').map(node => node.props.name), ['author', 'nested', 'community'])
      assert.equal(textOf(root).includes('Index heading to omit'), false)
      assert.equal(textOf(root).includes('map'), true)
      const author = all(root, node => node.tag === 'collapse-item' && node.props.name === 'author')[0]
      nodes.value = nodes.value.map(node => node.id === rootId ? { ...node, error: 'root offline' } : node)
      await nextTick()
      assert.equal(all(root, node => node.tag === 'alert').map(textOf).join(''), 'root offline')
      assert.strictEqual(all(root, node => node.tag === 'collapse-item' && node.props.name === 'author')[0], author)
      nodes.value = [{ id: rootId, label: 'Index heading to omit', status: 'ready', count: 1, groups: [{ items: ['direct root item'] }] }]
      await nextTick()
      assert.equal(all(root, node => node.tag === 'collapse-item').length, 0)
      assert.equal(textOf(root), 'direct root item')
      nodes.value = [{ ...nodes.value[0], count: 0, groups: [] }]
      await nextTick()
      assert.equal(textOf(root), 'source.noVisibleRepositories')
    } finally { app.unmount() }
  }
})
