import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'
import { modChanges } from '../src/locale/modChanges.ts'

const fixture = globalThis.__changeRender = {
  state: shallowRef(null), preparation: shallowRef(null), cancelDownload: shallowRef(false), cancelling: shallowRef(false),
  installNotes: shallowRef([]), messages: modChanges, calls: [],
}
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'change-render:naive', shortCircuit: true }
    if (context.parentURL === 'change-render:naive') return next(specifier, { ...context, parentURL: import.meta.url })
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'change-render:naive') return fake(`import{h}from'vue';
      const leaf=tag=>({props:['percentage','showIndicator','processing','disabled','type'],setup:(p,{slots})=>()=>h(tag,{...p},slots.default?.())});
      export const NButton=leaf('button'),NProgress=leaf('progress'),NTag=leaf('tag'),NAlert=leaf('alert'),NSpin=leaf('spin')`)
    if (/\/modChanges\/(?:ChangePlan(?:Dialog|Details|Item)|ModInstallNoteDialog)\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'change-render', inlineTemplate: true }).content }
    }
    if (url.endsWith('/src/modChanges/store.ts')) return fake(`export const useModChanges=()=>{const f=globalThis.__changeRender;
      return{state:f.state,preparation:f.preparation,canCancelDownload:f.cancelDownload,cancellingDownload:f.cancelling,
      installNotes:f.installNotes,acceptInstallNotes:()=>f.calls.push('note-read'),
      confirm:()=>f.calls.push('confirm'),retry:()=>f.calls.push('retry'),cancel:()=>f.calls.push('cancel'),cancelPreparationDownload:()=>f.calls.push('cancel-download')}}`)
    if (url.endsWith('/src/modChanges/useModChangesLifecycle.ts')) return fake('export const useModChangesLifecycle=()=>{}')
    if (url.endsWith('/src/components/AppModal.vue')) return fake(`import{h}from'vue';export default{props:['show','cardClass'],setup:(p,{slots})=>()=>p.show?h('modal',{class:p.cardClass},[slots.default?.(),h('footer',{},slots.footer?.())]):null}`)
    if (url.endsWith('/src/markdown.ts')) return fake('export const renderReadme=String')
    if (url.endsWith('/src/locale/backend.ts')) return fake('export const backendMessageOf=String;export const translateBackendMessage=String')
    if (url.endsWith('/src/i18n.ts')) return fake(`export const pickI18n=(_,fallback)=>fallback; export const locale={value:'zh-CN'};
      export const t=(key,args={})=>{let text=globalThis.__changeRender.messages[key.replace(/^modChanges\\./,'')]?.['zh-CN']??key;for(const[k,v]of Object.entries(args))text=text.replaceAll('{'+k+'}',String(v));return text}`)
    return next(url, context)
  },
})
const { default: Dialog } = await import('../src/modChanges/ChangePlanDialog.vue')
const host = createRenderer({
  createElement: tag => ({ tag, props: {}, children: [] }), createText: text => ({ text }), createComment: () => ({}),
  patchProp: (node, key, _, value) => { node.props[key] = value },
  insert(node, parent, anchor) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = parent; const before = parent.children.indexOf(anchor)
    parent.children.splice(before < 0 ? parent.children.length : before, 0, node)
  },
  remove(node) { if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1) },
  setText: (node, text) => { node.text = text }, setElementText: (node, text) => { node.text = text; node.children = [] },
  parentNode: node => node.parent, nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
})
const find = (node, match) => [...(match(node) ? [node] : []), ...(node.children?.flatMap(child => find(child, match)) ?? [])]
const text = node => (node.text ?? '') + (node.children?.map(text).join('') ?? '')
const action = (id, kind, extra = {}) => ({ node_id: id, kind, reason: '', caused_by: [], ...extra })
const plan = { plan_id: 'one', status: 'ready', prepared: false, blockers: [], impacts: [], roots: [], evaluations: [], install_notes: [],
  nodes: [{ node_id: 'web', label: 'Web 前置', dll_files: [], candidates: [] }, { node_id: 'level', label: 'DIYLevel 框架', dll_files: [], candidates: [] }],
  actions: [action('web', 'downgrade', { from_tag: 'v4.0.0', to_tag: 'v3.10.2' }), action('level', 'replace', { from_tag: 'v0.10', to_tag: 'v0.10' })] }
function mounted(t) {
  fixture.state.value = { phase: 'confirming', plan, error: null }; fixture.preparation.value = null
  fixture.calls = []; fixture.cancelDownload.value = false; fixture.installNotes.value = []
  const root = { children: [] }, app = host.createApp(Dialog)
  app.mount(root); t.after(() => app.unmount())
  return root
}

test('first install hides the entire plan while one five-second note window can be cancelled', async t => {
  const root = mounted(t)
  fixture.installNotes.value = [
    { id: 'street', name: '街机 MOD 稳定版', text: 'Read before installing.' },
    { id: 'dependency', name: '前置', text: 'Required component.' },
  ]
  await nextTick()
  const modals = find(root, node => node.tag === 'modal')
  assert.equal(modals.length, 1)
  assert.equal(modals[0].props.class, 'mod-install-note')
  assert.match(text(modals[0]), /街机 MOD 稳定版.*前置/)
  assert.doesNotMatch(text(root), /确认并应用|Web 前置/)
  const [cancel, next] = find(root, node => node.tag === 'button')
  assert.match(text(next), /\(5\)$/)
  assert.equal(next.props.disabled, true)
  next.props.onClick()
  assert.deepEqual(fixture.calls, [])
  cancel.props.onClick()
  assert.deepEqual(fixture.calls, ['cancel'])
})

test('production dialog renders current actions and stages without inline transfer UI or technical ids', async t => {
  const root = mounted(t)
  fixture.state.value = { phase: 'preparing', plan, error: null }
  fixture.preparation.value = { plan_id: 'one', stage: 'downloading', actions: [
    action('web', 'upgrade', { from_tag: 'v4.0.0', to_tag: 'v5.0.0' }), action('web', 'disable', { member_key: 'opaque-uuid' })],
    items: [{ node_id: 'web', label: 'Web 前置', tag: 'v5.0.0', candidate_id: 'candidate', stage: 'downloading',
      download: { downloaded: 50, total: 100, speed_bytes: 1024, finished: false, phase: 'connecting' } }] }
  await nextTick()
  assert.equal(find(root, n => n.tag === 'article' && n.props['data-change-node']).length, 1)
  const row = find(root, n => n.tag === 'article' && n.props['data-change-node'])[0]
  assert.match(text(row), /升级.*禁用.*Web 前置.*v4\.0\.0.*v5\.0\.0/)
  assert.doesNotMatch(text(row), /opaque-uuid|DIYLevel 框架|降级/)
  assert.match(text(find(row, n => n.props?.role === 'status')[0]), /正在下载/)
  assert.equal(find(row, n => n.tag === 'progress').length, 0)
  assert.doesNotMatch(text(row), /50%|50 B|100 B|KB\/s/)
  assert.equal(find(root, n => n.tag === 'footer').flatMap(n => find(n, x => x.tag === 'progress'))[0].props.percentage, 0)
  fixture.preparation.value = { ...fixture.preparation.value, stage: 'verifying', items: [{ ...fixture.preparation.value.items[0],
    stage: 'verifying', download: { downloaded: 100, total: 100, speed_bytes: 1024, finished: true } }] }
  await nextTick()
  assert.match(text(find(row, n => n.props?.role === 'status')[0]), /正在验证/)
  assert.equal(find(row, n => n.tag === 'progress').length, 0)
  assert.doesNotMatch(text(row), /正在下载|100%|100 B|KB\/s/)
  assert.deepEqual(fixture.calls, [])
})

test('final confirmation uses the returned plan actions, keeps details collapsed and never self-executes', async t => {
  const root = mounted(t)
  fixture.preparation.value = { plan_id: 'one', stage: 'ready', actions: [action('web', 'upgrade', { to_tag: 'v5' })], items: [] }
  fixture.state.value = { phase: 'reconfirming', plan: { ...plan, plan_id: 'verified', prepared: true,
    actions: [action('level', 'downgrade', { from_tag: 'v0.11', to_tag: 'v0.10' })] }, error: null }
  await nextTick()
  const rows = find(root, n => n.tag === 'article' && n.props['data-change-node'])
  assert.equal(rows.length, 1); assert.match(text(rows[0]), /降级.*DIYLevel 框架.*v0\.11.*v0\.10/)
  assert.doesNotMatch(text(rows[0]), /升级|v5|Web 前置/)
  assert.equal(find(root, n => n.tag === 'details')[0].props.open, false)
  assert.deepEqual(fixture.calls, [])
  assert.match(text(find(root, n => n.tag === 'footer')[0]), /确认更新后的方案/)
})

test('expanded details replace the summary and expose actual dependency and conflict reasons in separate rows', async t => {
  const root = mounted(t)
  fixture.state.value = { phase: 'confirming', plan: { ...plan, impacts: ['mod:opaque'], actions: [
    action('web', 'disable', { from_tag: 'v4.0.0', to_tag: 'v4.0.0', reason: 'disable_conflict_or_dependent',
      reason_details: [{ kind: 'conflicts_with', related_node_ids: ['web', 'level'], requirement: '>=0.11', prerelease: true }] }),
    action('web', 'upgrade', { from_tag: 'v3.10.2', to_tag: 'v4.0.0',
      reason_details: [{ kind: 'required_by', related_node_ids: ['level'], requirement: '>=4' }] }),
  ] }, error: null }
  await nextTick()
  const details = find(root, n => n.tag === 'details')[0]
  details.props.onToggle({ currentTarget: { open: true } })
  await nextTick()
  assert.equal(find(root, n => n.tag === 'article').length, 0)
  const rows = find(root, n => n.tag === 'tbody')[0].children.filter(n => n.tag === 'tr')
  assert.equal(rows.length, 2)
  assert.equal(text(rows[0].children[2]), '')
  assert.match(text(rows[0].children[3]), /与 DIYLevel 框架 冲突.*冲突版本 >=0\.11.*仅预发布版/)
  assert.equal(text(rows[1].children[2]), 'v3.10.2 → v4.0.0')
  assert.match(text(rows[1].children[3]), /DIYLevel 框架 依赖此项.*要求 >=4/)
  assert.doesNotMatch(text(root), /disable_conflict_or_dependent|mod:opaque/)
  details.props.onToggle({ currentTarget: { open: false } })
  await nextTick()
  assert.equal(find(root, n => n.tag === 'table').length, 0)
  assert.equal(find(root, n => n.tag === 'article').length, 1)
})

test('prepared switches show a check and honest preparation status inside the same heading', async t => {
  const root = mounted(t)
  const actions = [action('web', 'disable'), action('level', 'upgrade', { from_tag: 'v0.10', to_tag: 'v0.11.1' })]
  fixture.state.value = { phase: 'preparing', plan: { ...plan, actions }, error: null }
  fixture.preparation.value = { plan_id: 'one', stage: 'downloading', actions, items: [
    { node_id: 'web', label: 'Web 前置', candidate_id: 'installed', stage: 'ready' },
    { node_id: 'level', label: 'DIYLevel 框架', candidate_id: 'next', tag: 'v0.11.1', stage: 'downloading',
      download: { downloaded: 25, total: 100, speed_bytes: 1024, finished: false } },
  ] }
  await nextTick()
  const rows = find(root, n => n.tag === 'article' && n.props['data-change-node'])
  const statuses = rows.map(row => find(row, n => n.props?.role === 'status')[0])
  assert.match(text(rows[0]), /禁用.*Web 前置.*✓准备完成/)
  assert.equal(statuses[0].parent.props.class, 'item-heading')
  assert.equal(find(statuses[0], n => n.props?.['aria-hidden'] === 'true').length, 1)
  assert.equal(find(rows[0], n => n.tag === 'progress').length, 0)
  assert.doesNotMatch(text(rows[0]), /文件已就绪|执行完毕|已禁用/)
  assert.equal(statuses[1].parent.props.class, 'item-heading')
  assert.match(text(rows[1]), /v0\.10.*v0\.11\.1.*正在下载/)
  assert.equal(find(rows[1], n => n.tag === 'progress').length, 0)
  assert.doesNotMatch(text(rows[1]), /25%|25 B|KB\/s/)
  assert.deepEqual(fixture.calls, [])
})
