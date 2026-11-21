import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, nextTick, shallowReactive, shallowRef } from 'vue'
import { modChanges } from '../src/locale/modChanges.ts'
import { common } from '../src/locale/common.ts'
import { modsOnline } from '../src/locale/modsOnline.ts'

const fixture = globalThis.__changeRender = {
  state: shallowRef(null), preparation: shallowRef(null), cancelDownload: shallowRef(false), cancelling: shallowRef(false),
  execution: shallowReactive(new Map()), installNotes: shallowRef([]), locale: shallowRef('zh-CN'), messages: { modChanges, common, modsOnline }, calls: [],
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
      export const NButton=leaf('button'),NProgress=leaf('progress'),NTag=leaf('tag'),NAlert=leaf('alert'),NSpin=leaf('spin'),NIcon=leaf('i')`)
    if (/\/modChanges\/[^/]+\.vue$|\/components\/MarkdownBody\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'change-render', inlineTemplate: true }).content }
    }
    if (url.endsWith('/src/modChanges/store.ts')) return fake(`export const useModChanges=()=>{const f=globalThis.__changeRender;
      return{state:f.state,preparation:f.preparation,execution:f.execution,canCancelDownload:f.cancelDownload,cancellingDownload:f.cancelling,
      installNotes:f.installNotes,acceptInstallNotes:()=>f.calls.push('note-read'),
      confirm:()=>f.calls.push('confirm'),retry:()=>f.calls.push('retry'),cancel:()=>f.calls.push('cancel'),cancelPreparationDownload:()=>f.calls.push('cancel-download')}}`)
    if (url.endsWith('/src/modChanges/useModChangesLifecycle.ts')) return fake('export const useModChangesLifecycle=()=>{}')
    if (url.endsWith('/src/components/AppModal.vue')) return fake(`import{h}from'vue';export default{props:['show','cardClass','maxWidth','maxHeight','fill'],setup:(p,{slots})=>()=>p.show?h('modal',{class:p.cardClass,maxWidth:p.maxWidth,maxHeight:p.maxHeight,fill:p.fill},[h('header',{},slots['header-extra']?.()),slots.default?.(),h('footer',{},slots.footer?.())]):null}`)
    if (url.endsWith('/src/markdown.ts')) return fake('export const renderReadme=String;export const renderMermaidBlocks=async()=>{}')
    if (url.endsWith('/src/locale/backend.ts')) return fake('export const backendMessageOf=String;export const translateBackendMessage=String')
    if (url.endsWith('/src/i18n.ts')) return fake(`export const locale=globalThis.__changeRender.locale;
      export const pickI18n=(values,fallback)=>values?.[locale.value]??fallback;
      export const t=(key,args={})=>{const [domain,...path]=key.split('.');let text=globalThis.__changeRender.messages[domain]?.[path.join('.')]?.[locale.value]??key;for(const[k,v]of Object.entries(args))text=text.replaceAll('{'+k+'}',String(v));return text}`)
    return next(url, context)
  },
})
const { default: Dialog } = await import('../src/modChanges/ChangePlanDialog.vue')
const { changeStatusKey } = await import('../src/modChanges/presentation.ts')
const host = createRenderer({
  createElement: tag => ({ tag, props: {}, children: [], focus() {} }), createText: text => ({ text }), createComment: () => ({}),
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
const bodyRows = root => find(root, node => node.tag === 'tbody').flatMap(body => body.children.filter(node => node.tag === 'tr'))
const cells = row => row.children.filter(node => node.tag === 'td' || node.tag === 'th')
const category = row => cells(row)[1].props['data-change-category']
const statuses = row => find(cells(row)[2], node => node.props?.['data-change-stage'])
const stage = row => statuses(row)[0]?.props['data-change-stage']
const icon = row => statuses(row)[0]?.props['data-change-icon']
const action = (id, kind, extra = {}) => ({ node_id: id, kind, reason: '', caused_by: [], ...extra })
const plan = { plan_id: 'one', status: 'ready', prepared: false, blockers: [], impacts: [], roots: [], evaluations: [], install_notes: [],
  nodes: [{ node_id: 'web', kind: 'mod', label: 'Web 前置', dll_files: [], candidates: [] }, { node_id: 'level', kind: 'mod', label: 'DIYLevel 框架', dll_files: [], candidates: [] }],
  actions: [action('web', 'downgrade', { from_tag: 'v4.0.0', to_tag: 'v3.10.2' }), action('level', 'replace', { from_tag: 'v0.10', to_tag: 'v0.10' })] }
function mounted(t) {
  fixture.state.value = { phase: 'confirming', plan, error: null }; fixture.preparation.value = null
  fixture.execution.clear(); fixture.calls = []; fixture.cancelDownload.value = false; fixture.installNotes.value = []; fixture.locale.value = 'zh-CN'
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

test('the plan table shows live preparation only in the affected row and leaves transfer details to the race overlay', async t => {
  const root = mounted(t)
  fixture.state.value = { phase: 'preparing', plan, error: null }
  fixture.preparation.value = { plan_id: 'one', stage: 'downloading', actions: [
    action('web', 'upgrade', { from_tag: 'v4.0.0', to_tag: 'v5.0.0' }), action('web', 'disable', { member_key: 'opaque-uuid' })],
    items: [{ node_id: 'web', label: 'Web 前置', tag: 'v5.0.0', candidate_id: 'candidate', stage: 'downloading',
      download: { downloaded: 50, total: 100, speed_bytes: 1024, finished: false, phase: 'connecting' } }] }
  await nextTick()
  const rows = bodyRows(root)
  assert.equal(rows.length, 2)
  assert.match(text(rows[0]), /升级.*Web 前置.*v4\.0\.0.*v5\.0\.0/)
  assert.match(text(rows[1]), /禁用.*包内插件/)
  assert.doesNotMatch(text(root), /opaque-uuid|DIYLevel 框架|降级/)
  assert.deepEqual(rows.map(stage), ['downloading', 'queued'])
  assert.deepEqual(rows.map(icon), ['spinner', 'dots'])
  assert.equal(statuses(rows[0])[0].props.title, '正在下载')
  const table = find(root, n => n.tag === 'table')[0]
  assert.equal(find(table, n => n.tag === 'progress').length, 0)
  assert.doesNotMatch(text(table), /50%|50 B|100 B|KB\/s/)
  assert.equal(find(root, n => n.tag === 'footer').flatMap(n => find(n, x => x.tag === 'progress'))[0].props.percentage, 0)
  fixture.preparation.value = { ...fixture.preparation.value, stage: 'verifying', items: [{ ...fixture.preparation.value.items[0],
    stage: 'verifying', download: { downloaded: 100, total: 100, speed_bytes: 1024, finished: true } }] }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['verifying', 'queued'])
  assert.equal(statuses(bodyRows(root)[0])[0].props.title, '正在验证')
  assert.equal(find(table, n => n.tag === 'progress').length, 0)
  assert.doesNotMatch(text(table), /100%|100 B|KB\/s/)
  fixture.preparation.value = { ...fixture.preparation.value, stage: 'failed', items: [{ ...fixture.preparation.value.items[0], stage: 'failed' }] }
  fixture.state.value = { phase: 'failed', plan, error: 'Download failed.' }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['preparationFailed', 'queued'])
  assert.deepEqual(bodyRows(root).map(icon), ['error', 'dots'])
  assert.match(statuses(bodyRows(root)[0])[0].props.title, /失败/)
  assert.deepEqual(fixture.calls, [])
})

test('a completed cache or local payload paints a check, and the new apply dialog resets it', async t => {
  const root = mounted(t)
  const verified = { ...plan, prepared: true, actions: [action('level', 'downgrade', { from_tag: 'v0.11', to_tag: 'v0.10' })] }
  fixture.preparation.value = { plan_id: 'one', stage: 'ready', actions: [action('web', 'upgrade', { to_tag: 'v5' })], items: [] }
  const panel = () => find(root, node => node.tag === 'modal')[0]
  const original = panel()
  fixture.state.value = { phase: 'prepared', plan: verified, error: null }
  await nextTick()
  assert.strictEqual(panel(), original)
  assert.deepEqual(bodyRows(root).map(stage), ['prepared'])
  assert.deepEqual(bodyRows(root).map(icon), ['check'])
  assert.equal(statuses(bodyRows(root)[0])[0].props.title, '文件已验证，等待应用')
  assert.deepEqual(fixture.calls, [])
  fixture.state.value = { phase: 'reconfirming', applyReview: 'prepared', plan: verified, error: null }
  await nextTick()
  assert.notStrictEqual(panel(), original)
  const rows = bodyRows(root)
  assert.equal(rows.length, 1); assert.match(text(rows[0]), /降级.*DIYLevel 框架.*v0\.11.*v0\.10/)
  assert.deepEqual(rows.map(stage), ['queued'])
  assert.deepEqual(rows.map(icon), ['dots'])
  assert.equal(statuses(rows[0])[0].props.title, '等待处理')
  assert.doesNotMatch(text(rows[0]), /升级|v5|Web 前置/)
  assert.equal(find(root, n => n.tag === 'details').length, 0)
  assert.deepEqual(fixture.calls, [])
  assert.match(text(find(root, n => n.tag === 'footer')[0]), /确认并应用/)
})

test('the apply review alone lists untracked replacements with localized names and exact text paths', async t => {
  const root = mounted(t)
  const paths = ['BepInEx/plugins/Existing DLL.dll', 'BepInEx/plugins/Details<local>.ini']
  const verified = { ...plan, prepared: true,
    nodes: [{ ...plan.nodes[0], name_i18n: { 'zh-CN': '本地化前置', en: 'Localized prerequisite' } }],
    untracked_overwrites: [{ node_id: 'web', path: paths[0] }, { node_id: 'unlabelled', path: paths[1] }] }
  const notice = () => find(root, n => n.props && Object.hasOwn(n.props, 'data-untracked-overwrites'))[0]
  for (const state of [
    { phase: 'confirming', plan: verified },
    { phase: 'preparing', plan },
    { phase: 'prepared', plan: verified },
    { phase: 'reconfirming', applyReview: 'changed', plan: { ...verified, untracked_overwrites: [] } },
  ]) {
    fixture.state.value = { ...state, error: null }
    await nextTick()
    assert.equal(notice(), undefined)
  }
  fixture.state.value = { phase: 'reconfirming', applyReview: 'changed', plan: verified, error: null }
  await nextTick()
  assert.equal(notice().props.type, 'warning')
  assert.equal(notice().props.title, '将覆盖的现有文件')
  assert.match(text(notice()), /这些文件没有安装记录。本次应用会用所选版本的文件替换它们。/)
  assert.deepEqual(find(notice(), n => n.tag === 'li').map(row => find(row, n => n.tag === 'strong' || n.tag === 'code').map(text)),
    [['本地化前置', paths[0]], ['未命名项目', paths[1]]])
  fixture.locale.value = 'en'
  await nextTick()
  assert.equal(notice().props.title, 'Existing files to overwrite')
  assert.deepEqual(find(notice(), n => n.tag === 'strong').map(text), ['Localized prerequisite', 'Unnamed item'])
  fixture.state.value = { ...fixture.state.value, phase: 'executing' }
  await nextTick()
  assert.equal(notice(), undefined)
})

test('the only view is a six-column table with separate actions, version changes, and readable reasons', async t => {
  const root = mounted(t)
  fixture.state.value = { phase: 'confirming', plan: { ...plan, impacts: ['mod:opaque'], actions: [
    action('web', 'disable', { from_tag: 'v4.0.0', to_tag: 'v4.0.0', reason: 'disable_conflict_or_dependent',
      reason_details: [{ kind: 'conflicts_with', related_node_ids: ['web', 'level'], requirement: '>=0.11', prerelease: true }] }),
    action('web', 'upgrade', { from_tag: 'v3.10.2', to_tag: 'v4.0.0',
      reason_details: [{ kind: 'required_by', related_node_ids: ['level'], requirement: '>=4' }] }),
  ] }, error: null }
  await nextTick()
  const [modal] = find(root, n => n.tag === 'modal')
  assert.equal(modal.props.maxWidth, '90%'); assert.equal(modal.props.maxHeight, '90%'); assert.ok(!modal.props.fill)
  assert.equal(find(root, n => n.tag === 'table').length, 1)
  assert.deepEqual(find(root, n => n.props?.scope === 'col').map(text), ['操作', '标识', '状态', '项目', '版本', '变更原因'])
  assert.equal(find(root, n => n.tag === 'article' || n.tag === 'details').length, 0)
  const rows = bodyRows(root)
  assert.equal(rows.length, 2)
  assert.deepEqual(cells(rows[0]).slice(0, 5).map(text), ['禁用', 'MOD', '', 'Web 前置', ''])
  assert.match(text(cells(rows[0])[5]), /与 DIYLevel 框架 冲突.*冲突版本 >=0\.11.*仅预发布版/)
  assert.equal(text(cells(rows[1])[4]), 'v3.10.2 → v4.0.0')
  assert.match(text(cells(rows[1])[5]), /DIYLevel 框架 依赖此项.*要求 >=4/)
  assert.doesNotMatch(text(root), /disable_conflict_or_dependent|mod:opaque/)
})

test('a manual map selection remains visible by name and version when no action covers it', async t => {
  const root = mounted(t)
  fixture.state.value = { phase: 'confirming', error: null, plan: { ...plan,
    actions: [], roots: [{ node_id: 'preview:temporary-id', label: '羊的回合', tag: '0.8' }],
  } }
  await nextTick()
  const [row] = bodyRows(root)
  assert.equal(bodyRows(root).length, 1)
  assert.match(text(cells(row)[3]), /羊的回合/)
  assert.equal(text(cells(row)[4]), '0.8')
  assert.doesNotMatch(text(root), /temporary-id/)
})

test('blocked plans show tried versions and actual failures without exposing any plan action or runtime check', async t => {
  const root = mounted(t)
  const web = version => ({ owner_id: 'web', owner_version: 'v4.0.0', relation: { guid: 'dev.gua.overcooked.diylevel', version: '>=0.11,<0.12' },
    targets: [{ node_id: 'level', version, enabled: true }], state: 'satisfied', reason: 'relation_satisfied' })
  const fix = { owner_id: 'level', owner_version: '0.11.1', relation: { guid: 'com.example.fastinit', version: '>=1.9.6', prerelease: false }, targets: [],
    state: 'missing', reason: 'required_plugin_missing' }
  const versionMismatch = { ...fix, owner_version: '0.11.0', targets: [{ node_id: 'old-fix-id', version: '1.6.4', enabled: true }],
    state: 'version_mismatch', reason: 'required_version_mismatch' }
  const ownership = { ...versionMismatch, targets: [...versionMismatch.targets, { node_id: 'older-fix-id', version: '1.3.0', enabled: true }],
    reason: 'modChanges.multipleInstalledCopies' }
  fixture.execution.set(changeStatusKey('level'), 'applied')
  fixture.state.value = { phase: 'reconfirming', plan: { ...plan, status: 'incomplete',
    roots: [{ node_id: 'web', label: 'Web 前置', tag: 'v4.0.0' }], blockers: [
      { code: 'relation_has_no_solution', traces: [[web('0.11.1'), fix], [web('0.11.0'), versionMismatch], [web('0.11.0'), ownership],
      [{ ...fix, owner_id: 'independent', owner_version: '2.0', relation: undefined, state: 'unknown', reason: 'candidate_relations_unknown' }]], traces_truncated: true },
      { code: 'search_exhausted', owner_id: 'web' },
    ] }, error: null }
  await nextTick()
  const rows = bodyRows(root)
  assert.deepEqual(rows.map(row => [text(cells(row)[0]), text(cells(row)[3]), text(cells(row)[4]), stage(row)]), [
    ['', 'Web 前置', 'v4.0.0', undefined], ['', 'DIYLevel 框架', '0.11.1', undefined], ['', 'com.example.fastinit', '—', 'blocked'],
    ['', 'DIYLevel 框架', '0.11.0', undefined], ['', 'com.example.fastinit', '1.6.4', 'blocked'], ['', 'com.example.fastinit', '', 'blocked'],
    ['', '未命名项目', '2.0', 'blocked'],
  ])
  assert.match(text(rows[0]), /路径 1.*所选目标/)
  assert.match(text(rows[1]), /路径 1\.1.*Web 前置 \(v4\.0\.0\) 依赖此项.*要求 >=0\.11,<0\.12/)
  assert.match(text(rows[2]), /DIYLevel 框架 \(0\.11\.1\) 依赖此项.*要求 >=1\.9\.6.*仅正式版.*依赖未安装/)
  assert.match(text(rows[3]), /路径 1\.2/)
  assert.match(text(rows[5]), /路径 1\.2\.2.*1\.6\.4.*1\.3\.0.*对应多个已安装副本/)
  assert.deepEqual(rows.map(row => row.props['data-diagnostic-depth']), [undefined, 1, 2, 1, 2, 2, undefined])
  const failures = rows.flatMap(statuses)
  assert.ok(failures.every(status => status.props.role === 'img'))
  assert.match(failures.map(status => status.props['aria-label']).join('\n'),
    /^无法继续：依赖未安装\n无法继续：依赖版本不符\n无法继续：这个仓库对应多个已安装副本[^\n]+\n无法继续：缺少确认所需的版本或关系元数据。$/)
  assert.match(text(root), /不会执行任何操作.*还有未展示的依赖路径/)
  assert.match(text(root), /安全搜索范围/)
  assert.doesNotMatch(text(root), /relation_has_no_solution|请确认完整变更列表|请重新确认完整列表|old-fix-id/)
  assert.equal(find(root, node => node.props?.['data-plan-action'] === 'confirm')[0].props.disabled, true)
  assert.deepEqual(fixture.calls, [])
})

test('an older blocked result without traces still explains the known issue and hides attempts', async t => {
  const root = mounted(t)
  fixture.state.value = { phase: 'confirming', error: null, plan: { ...plan, status: 'unsatisfiable',
    roots: [{ node_id: 'web', label: 'Web 前置', tag: 'v4.0.0' }],
    blockers: [{ code: 'relation_has_no_solution', owner_id: 'web', target_guid: 'dev.gua.overcooked.diylevel' }],
  } }
  await nextTick()
  assert.match(text(root), /无法找到同时满足.*Web 前置.*dev\.gua\.overcooked\.diylevel/)
  assert.equal(bodyRows(root).length, 1)
  assert.equal(text(cells(bodyRows(root)[0])[0]), '')
  assert.equal(find(root, node => node.props?.['data-plan-action'] === 'confirm')[0].props.disabled, true)
})

test('verified files never get an execution check before changes actually run', async t => {
  const root = mounted(t)
  const actions = [action('web', 'disable'), action('level', 'upgrade', { from_tag: 'v0.10', to_tag: 'v0.11.1' })]
  fixture.state.value = { phase: 'preparing', plan: { ...plan, actions }, error: null }
  fixture.preparation.value = { plan_id: 'one', stage: 'downloading', actions, items: [
    { node_id: 'web', label: 'Web 前置', candidate_id: 'installed', stage: 'ready' },
    { node_id: 'level', label: 'DIYLevel 框架', candidate_id: 'next', tag: 'v0.11.1', stage: 'downloading',
      download: { downloaded: 25, total: 100, speed_bytes: 1024, finished: false } },
  ] }
  await nextTick()
  const rows = bodyRows(root)
  assert.deepEqual(rows.map(stage), ['queued', 'downloading'])
  assert.match(text(rows[0]), /禁用.*Web 前置/)
  assert.doesNotMatch(text(rows[0]), /准备完成|文件已就绪|执行完毕|已禁用|✓/)
  assert.equal(text(cells(rows[0])[4]), '')
  assert.equal(text(cells(rows[1])[4]), 'v0.10 → v0.11.1')
  assert.deepEqual(fixture.calls, [])
})

test('execution icons follow exact member writes, and a rolled-back write loses its check', async t => {
  const root = mounted(t)
  const activePlan = { ...plan, prepared: true, nodes: [
    { ...plan.nodes[0], members: [{ key: 'a', dll_file: 'Package/A.dll', enabled: true }, { key: 'b', dll_file: 'Package/B.dll', enabled: true }] },
    plan.nodes[1],
  ], actions: [action('web', 'disable', { member_key: 'a' }), action('web', 'disable', { member_key: 'b' }),
    action('level', 'upgrade', { from_tag: 'v0.10', to_tag: 'v0.11.1' })] }
  fixture.state.value = { phase: 'executing', plan: activePlan, error: null }
  fixture.execution.set(changeStatusKey('web', 'a'), 'applying')
  await nextTick()
  const rows = bodyRows(root)
  assert.equal(rows.length, 3)
  assert.match(text(cells(rows[0])[3]), /Web 前置.*A\.dll/)
  assert.match(text(cells(rows[1])[3]), /Web 前置.*B\.dll/)
  assert.deepEqual(rows.map(stage), ['applying', 'queued', 'queued'])
  assert.equal(statuses(rows[0])[0].props.title, '正在处理')
  assert.equal(find(statuses(rows[0])[0], node => node.tag === 'spin').length, 1)
  fixture.execution.set(changeStatusKey('web', 'a'), 'processed')
  fixture.execution.set(changeStatusKey('level'), 'applying')
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['processed', 'queued', 'applying'])
  assert.equal(statuses(bodyRows(root)[0])[0].props.title, '已处理，等待整批提交')
  fixture.execution.set(changeStatusKey('web', 'a'), 'rolled_back')
  fixture.execution.set(changeStatusKey('level'), 'failed')
  fixture.state.value = { phase: 'failed', plan: activePlan, error: 'Rolled back.' }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['rolled_back', 'queued', 'failed'])
  assert.deepEqual(bodyRows(root).map(icon), ['undo', 'dots', 'error'])
  assert.match(statuses(bodyRows(root)[0])[0].props.title, /已回滚/)
  assert.equal(statuses(bodyRows(root)[2])[0].props.title, '未能应用')
  assert.match(text(root), /Rolled back\./)
  assert.deepEqual(fixture.calls, [])
})

test('a release upgrade shows each old copy before approval and gives it independent execution status', async t => {
  const root = mounted(t)
  const id = 'mod:fastinit'
  const first = { member_key: 'old/a.dll', label: 'FastInit', version: '1.3', dll_file: 'Old/A.dll' }
  const second = { member_key: 'old/b.dll', label: 'FastInit', version: '1.4', dll_file: 'Old/B.dll.disabled' }
  const activePlan = { ...plan, nodes: [{ ...plan.nodes[0], node_id: id, label: 'FastInit' }],
    actions: [action(id, 'upgrade', { from_tag: 'v1.9.5', to_tag: 'v1.9.6' })],
    retired_mod_copies: { [id]: [first, second] } }
  fixture.state.value = { phase: 'confirming', plan: activePlan, error: null }
  await nextTick()
  const rows = bodyRows(root)
  assert.deepEqual(rows.map(row => [text(cells(row)[0]), text(cells(row)[4]), row.props['data-change-parent']]), [
    ['升级', 'v1.9.5 → v1.9.6', undefined], ['卸载', '1.3', id], ['卸载', '1.4', id],
  ])
  assert.equal(find(cells(rows[2])[3], node => node.props?.title === second.dll_file).length, 1)
  assert.match(text(cells(rows[1])[5]), /同一 MOD 的旧副本/)
  assert.deepEqual(fixture.calls, [])
  fixture.execution.set(changeStatusKey(id), 'applying')
  fixture.execution.set(changeStatusKey(id, first.member_key), 'processed')
  fixture.state.value = { phase: 'executing', plan: { ...activePlan, prepared: true }, error: null }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['applying', 'processed', 'queued'])
  fixture.execution.set(changeStatusKey(id), 'failed')
  fixture.execution.set(changeStatusKey(id, first.member_key), 'rolled_back')
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['failed', 'rolled_back', 'queued'])
})

test('the rows for a bundled DLL stay with its map and show the map transaction status', async t => {
  const root = mounted(t)
  const activePlan = { ...plan, prepared: true, nodes: [
    { ...plan.nodes[0], kind: 'map', members: [{ key: 'addon', dll_file: 'Map/Addon.dll', enabled: true }] },
    plan.nodes[1],
  ], actions: [action('web', 'disable'), action('level', 'upgrade', { from_tag: 'v0.10', to_tag: 'v0.11.1' })] }
  fixture.state.value = { phase: 'executing', plan: activePlan, error: null }
  fixture.execution.set(changeStatusKey('web'), 'processed')
  await nextTick()
  const rows = bodyRows(root)
  assert.equal(rows.length, 3)
  assert.deepEqual(rows.map(stage), ['processed', 'processed', 'queued'])
  assert.deepEqual(rows.map(row => text(cells(row)[3]).trim()), ['Web 前置', 'Addon.dll', 'DIYLevel 框架'])
  assert.match(text(cells(rows[1])[5]), /随包.*禁用/)
  fixture.execution.set(changeStatusKey('web'), 'applied')
  fixture.execution.set(changeStatusKey('level'), 'applied')
  await nextTick()
  assert.deepEqual(bodyRows(root).map(stage), ['applied', 'applied', 'applied'])
  assert.equal(statuses(rows[1])[0].props.title, '已应用')
})

test('names switch with the UI for sources, local maps, actions and reasons and survive progress updates', async t => {
  const root = mounted(t)
  const nodes = [
    { node_id: 'online', kind: 'map', label: 'Source fallback', name_i18n: { 'zh-CN': '在线地图卡片', en: 'Online map card' } },
    { node_id: 'local', kind: 'map', label: 'Local Map', map_name_zh: '本地地图', map_dir_name: 'Dir' },
    { node_id: 'mod', kind: 'mod', label: 'Package.dll', name_i18n: { 'zh-CN': '仓库卡片名称', en: 'Repository card name' } },
  ]
  const actions = [action('local', 'disable'), action('mod', 'upgrade', { to_tag: '2.0',
    reason_details: [{ kind: 'required_by', related_node_ids: ['online'] }] })]
  const activePlan = { ...plan, nodes, actions, roots: [{ node_id: 'online', label: 'Stale fallback' }] }
  fixture.state.value = { phase: 'preparing', plan: activePlan, error: null }
  fixture.preparation.value = { plan_id: 'one', stage: 'verifying', actions, items: [
    { node_id: 'mod', label: 'Raw DLL progress label', stage: 'verifying' },
  ] }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(row => text(cells(row)[3])), ['本地地图', '仓库卡片名称', '在线地图卡片'])
  assert.match(text(cells(bodyRows(root)[1])[5]), /在线地图卡片 依赖此项/)
  assert.doesNotMatch(text(root), /Raw DLL progress label|Stale fallback/)
  fixture.locale.value = 'en'
  await nextTick()
  assert.deepEqual(bodyRows(root).map(row => text(cells(row)[3])), ['Local Map', 'Repository card name', 'Online map card'])
  assert.match(text(cells(bodyRows(root)[1])[5]), /Online map card requires this item/)
})

test('the missing edge and blocker notes use the source title while retaining the exact failing row', async t => {
  const root = mounted(t)
  const guid = 'Example.Plugin'
  const nodes = [{ node_id: 'owner', kind: 'map', label: 'Local Map', map_name_zh: '本地地图', map_dir_name: 'Dir' }]
  const step = { owner_id: 'owner', owner_version: '1.0', relation: { guid }, targets: [], state: 'missing', reason: 'required_plugin_missing' }
  fixture.state.value = { phase: 'confirming', error: null, plan: { ...plan, status: 'unsatisfiable', nodes,
    roots: [{ node_id: 'owner', label: 'Local Map', tag: '1.0' }],
    relation_labels: { 'example.plugin': { label: 'Dependency package', name_i18n: { 'zh-CN': '依赖仓库卡片', en: 'Dependency card' } } },
    blockers: [{ code: 'relation_has_no_solution', traces: [[step]] }, { code: 'relation_has_no_solution', owner_id: 'owner', target_guid: guid }],
  } }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(row => [text(cells(row)[3]), icon(row)]), [['本地地图', undefined], ['依赖仓库卡片', 'error']])
  assert.deepEqual(bodyRows(root).map(category), ['map', undefined])
  assert.match(text(cells(bodyRows(root)[1])[5]), /本地地图 \(1\.0\) 依赖此项/)
  assert.match(text(root), /无法找到同时满足.*本地地图.*依赖仓库卡片/)
  assert.doesNotMatch(text(root), /Example\.Plugin/)
  fixture.locale.value = 'en'
  await nextTick()
  assert.deepEqual(bodyRows(root).map(row => text(cells(row)[3])), ['Local Map', 'Dependency card'])
})

test('verified package roles distinguish bundled DLLs from other MODs in the dependency chain', async t => {
  const root = mounted(t)
  const pack = { ...plan.nodes[0], presentation_kind: 'mod_package', members: [
    { key: 'plugin', dll_file: 'Package/A.dll', enabled: true, role: 'mod' },
    { key: 'library', dll_file: 'Package/B.dll', enabled: true, role: 'dependency' },
    { key: 'unknown', dll_file: 'Package/C.dll', enabled: true },
  ] }
  const other = { ...plan.nodes[1], members: pack.members }
  const map = { ...plan.nodes[1], node_id: 'map', kind: 'map' }
  const activePlan = { ...plan, nodes: [pack, other, map], actions: [
    action('web', 'upgrade'), action('level', 'enable', { reason_details: [{ kind: 'required_by', related_node_ids: ['web'] }] }),
    action('map', 'enable'),
  ] }
  fixture.state.value = { phase: 'executing', plan: activePlan, error: null }
  fixture.execution.set(changeStatusKey('web'), 'applying')
  await nextTick()
  const rows = bodyRows(root)
  assert.deepEqual(rows.map(category), ['mod_package', 'plugin', 'dependency', undefined, 'mod', 'map'])
  assert.deepEqual(rows.map(row => text(cells(row)[1])), ['MOD 包', '插件', '依赖', '', 'MOD', '地图'])
  assert.deepEqual(rows.map(stage), ['applying', 'applying', 'applying', 'applying', 'queued', 'queued'])
  fixture.locale.value = 'en'
  await nextTick()
  assert.deepEqual(rows.map(row => text(cells(row)[1])), ['MOD Pack', 'Plugin', 'Dependency', '', 'MOD', 'Map'])
  fixture.state.value = { phase: 'confirming', error: null, plan: { ...activePlan, status: 'unsatisfiable',
    roots: [{ node_id: 'web', label: 'Web 前置', tag: 'v4.0.0' }], blockers: [{ code: 'relation_has_no_solution', traces: [[
      { owner_id: 'web', owner_version: 'v4.0.0', relation: { guid: 'com.example.level', version: '>=2' },
        targets: [{ node_id: 'level', version: '1.0', enabled: true }], state: 'version_mismatch', reason: 'required_version_mismatch' },
    ]] }],
  } }
  await nextTick()
  assert.deepEqual(bodyRows(root).map(category), ['mod_package', 'mod'])
  assert.deepEqual(bodyRows(root).map(icon), [undefined, 'error'])
})
