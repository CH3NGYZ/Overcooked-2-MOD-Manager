import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import { evaluationSnapshot, clearEvaluation } from './helpers/mod-change-evaluations.mjs'

// Run production setup, watchers, route transitions and computed selections.
// Only visual rendering, README/IPC and translation are isolated from Node.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'dependency-guide:naive', shortCircuit: true }
    if (specifier === '@vicons/ionicons5') return { url: 'dependency-guide:icons', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* use normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'dependency-guide:naive') return fake(
      'export const NButton={}, NDivider={}, NSelect={}, NSkeleton={}, NTabPane={}, NTabs={}, NTag={}')
    if (url === 'dependency-guide:icons') return fake('export const CloseOutline={}, DownloadOutline={}')
    if (/\/src\/components\/(ModRepositoryDetail|ReleaseDetailPanel)\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'dependency-install-guide-test' }).content }
    }
    if (url.endsWith('.vue')) return fake('export default {}')
    if (url.endsWith('/src/i18n.ts')) return fake(
      'export const t=key=>key; export const pickI18n=(_values,fallback)=>fallback')
    if (url.endsWith('/src/services/tauri.ts')) return fake(
      'export const api=new Proxy({}, { get(_target,key) { return () => { throw new Error("Unexpected IPC: "+String(key)) } } })')
    if (url.endsWith('/src/markdown.ts')) return fake(
      'export const renderReadme=value=>value||""; export const renderMermaidBlocks=async()=>{}')
    if (url.endsWith('/src/composables/useModRepositoryReadme.ts')) return fake(
      'import { shallowRef } from "vue"; export const useModRepositoryReadme=()=>({ readme:shallowRef(null), loading:shallowRef(false), error:shallowRef("") })')
    if (url.endsWith('/src/composables/useReleasePanelFit.ts')) return fake(
      'import { shallowRef } from "vue"; export const useReleasePanelFit=()=>({ panelElement:shallowRef(null), logBoxElement:shallowRef(null), panelStyle:shallowRef(undefined), logBoxStyle:shallowRef(undefined), isFitted:shallowRef(false) })')
    return next(url, context)
  },
})

const { useDependencyInstallGuide } = await import('../src/composables/useDependencyInstallGuide.ts')
const { useMapReleaseDetail } = await import('../src/composables/useMapReleaseDetail.ts')
const { expectPageScroll, endNavigation } = await import('../src/pageTransition.ts')
const { default: ModRepositoryDetail } = await import('../src/components/ModRepositoryDetail.vue')
const { default: ReleaseDetailPanel } = await import('../src/components/ReleaseDetailPanel.vue')

// vue-router was imported before this browser timer shim and uses memory history.
// Exercise the real pageTransition gate without keeping fallback timers alive.
globalThis.window = {
  setTimeout(fn, delay) { const timer = setTimeout(fn, delay); timer.unref(); return timer },
}
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const release = tag => ({ tag_name: tag, version: tag.replace(/^v/, ''), body: `${tag} notes`,
  prerelease: false, assets: [{ name: `${tag}.zip`, size: 100, download_url: `https://example.invalid/${tag}.zip` }] })
const repository = (id = 'runtime', tags = ['v4.0.0', 'v3.10.2']) => ({
  id, name: id, name_i18n: {}, note: '', note_i18n: {}, enabled: true,
  provider: 'github', username: 'sample', repository: id, branch: 'main',
  download_mode: 'release', asset_rules: [{ type: 'glob', value: '*.zip' }], cached_releases: tags.map(release),
})
async function settle() {
  for (let index = 0; index < 4; index++) { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
}
async function mountedGuide(t, { query = {}, repositories = [repository()], loading = false } = {}) {
  endNavigation()
  const router = createRouter({ history: createMemoryHistory(), routes: [
    { name: 'mods-online', path: '/mods-online', component: {} },
    { name: 'maps-local', path: '/maps-local', component: {} },
  ] })
  await router.push({ name: 'mods-online', query })
  const repos = shallowRef(repositories), busy = shallowRef(loading), calls = []
  let guide
  const app = renderer.createApp({ setup() {
    guide = useDependencyInstallGuide({ repositories: () => repos.value, loading: () => busy.value,
      reveal: repo => calls.push(['reveal', repo.id]), focus: id => calls.push(['focus', id]),
      open: id => calls.push(['open', id]), unavailable: () => calls.push(['unavailable']) })
    return () => null
  } })
  app.use(router)
  app.mount({})
  let unmounted = false
  const unmount = () => { if (!unmounted) { app.unmount(); unmounted = true } }
  t.after(() => { unmount(); endNavigation() })
  return { router, repos, busy, calls, guide, unmount }
}
function mountedComponent(t, component, initial, events = {}) {
  const props = shallowRef(initial)
  const subject = { ...component, render: () => null }
  const app = renderer.createApp({ render: () => h(subject, { ...props.value, ...events }) })
  // ModRepositoryDetail uses the real cross-link composable, with a real router.
  app.use(createRouter({ history: createMemoryHistory(), routes: [] }))
  app.mount({})
  t.after(() => app.unmount())
  return { props, state: app._instance.subTree.component.setupState }
}
const exactQuery = (id = 'runtime', tag = 'v3.10.2') => ({ focus: id, installTag: tag })
const target = (request = 1, tag = 'v3.10.2') => ({ repositoryId: 'runtime', tag, request })

test('plain focus preserves ordinary navigation and does not expand an install guide', async t => {
  const { guide, calls, router } = await mountedGuide(t, { query: { focus: 'runtime' } })
  await settle()
  assert.equal(guide.target.value, undefined)
  assert.deepEqual(calls, [])
  assert.equal(router.currentRoute.value.query.focus, 'runtime')
})

test('focus plus installTag opens the exact cached tag and consumes only its own query pair', async t => {
  const { guide, calls, router } = await mountedGuide(t, { query: { ...exactQuery(), preserve: 'yes' } })
  await settle()
  assert.deepEqual(guide.target.value, target())
  assert.deepEqual(calls, [['reveal', 'runtime'], ['focus', 'runtime'], ['open', 'runtime']])
  assert.deepEqual({ ...router.currentRoute.value.query }, { preserve: 'yes' })
  assert.equal(guide.target.value.tag, 'v3.10.2', 'query cleanup must preserve the prepared target')
})

test('an unfinished source tree waits for its repository before opening details', async t => {
  const { repos, busy, guide, calls } = await mountedGuide(t, { query: exactQuery(), repositories: [], loading: true })
  await settle()
  assert.deepEqual(calls, [])
  repos.value = [repository()]
  busy.value = false
  await settle()
  assert.equal(guide.target.value.tag, 'v3.10.2')
  assert.deepEqual(calls.at(-1), ['open', 'runtime'])
})

test('a repository shell waits for the exact cached Release while its refresh is still pending', async t => {
  const { repos, busy, guide, calls } = await mountedGuide(t, {
    query: exactQuery(), repositories: [], loading: true,
  })
  repos.value = [repository('runtime', [])]
  await settle()
  assert.deepEqual(calls, [], 'repository discovery must not outrun cached Release refresh')
  repos.value = [repository()]
  await settle()
  assert.equal(guide.target.value.tag, 'v3.10.2')
  assert.deepEqual(calls.at(-1), ['open', 'runtime'])
  busy.value = false
})

test('a missing target ends its bounded wait once, and a later refresh cannot reopen it', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const { repos, busy, guide, calls } = await mountedGuide(t, {
    query: exactQuery(), repositories: [], loading: true,
  })
  t.mock.timers.tick(9999)
  await settle()
  assert.deepEqual(calls, [])
  t.mock.timers.tick(1)
  await settle()
  assert.equal(guide.target.value, undefined)
  assert.deepEqual(calls, [['unavailable']])
  repos.value = [repository()]
  busy.value = false
  await settle()
  assert.deepEqual(calls, [['unavailable']])
})

test('finished loading with no requested repository or cached tag reports unavailable without using latest', async t => {
  for (const initial of [[], [repository('runtime', ['v4.0.0'])]]) {
    const { busy, guide, calls } = await mountedGuide(t, { query: exactQuery(), repositories: initial, loading: true })
    await settle()
    assert.deepEqual(calls, [], 'unavailable is deferred until current loading finishes')
    busy.value = false
    await settle()
    assert.equal(guide.target.value, undefined)
    assert.deepEqual(calls, [['unavailable']])
  }
})

test('cancel and unmount prevent a repository arriving later from opening details', async t => {
  for (const action of ['cancel', 'unmount']) {
    const { repos, busy, guide, calls, unmount } = await mountedGuide(t, {
      query: exactQuery(), repositories: [], loading: true,
    })
    if (action === 'cancel') guide.cancel()
    else unmount()
    repos.value = [repository()]
    busy.value = false
    await settle()
    assert.equal(guide.target.value, undefined)
    assert.deepEqual(calls, [])
  }
})

test('a route change cancels a waiting guide and cannot consume the new route query', async t => {
  const { router, repos, busy, guide, calls } = await mountedGuide(t, {
    query: exactQuery(), repositories: [], loading: true,
  })
  await router.push({ name: 'maps-local', query: { focus: 'my-map', preserve: 'yes' } })
  repos.value = [repository()]
  busy.value = false
  await settle()
  assert.equal(guide.target.value, undefined)
  assert.deepEqual(calls, [])
  assert.deepEqual({ ...router.currentRoute.value.query }, { focus: 'my-map', preserve: 'yes' })
})

test('rapid successive navigation opens only the newest requested repository and exact tag', async t => {
  const { router, repos, busy, guide, calls } = await mountedGuide(t, {
    query: exactQuery(), repositories: [], loading: true,
  })
  await router.push({ name: 'mods-online', query: exactQuery('diy', 'v0.10.8-beta.12') })
  repos.value = [repository(), repository('diy', ['v0.11.0', 'v0.10.8-beta.12'])]
  busy.value = false
  await settle()
  assert.equal(guide.target.value.repositoryId, 'diy')
  assert.equal(guide.target.value.tag, 'v0.10.8-beta.12')
  assert.deepEqual(calls, [['reveal', 'diy'], ['focus', 'diy'], ['open', 'diy']])
})

test('back-to-back awaited router pushes keep the newest guide despite previous query cleanup', async t => {
  const { router, guide, calls } = await mountedGuide(t, {
    repositories: [repository(), repository('diy', ['v0.11.0', 'v0.10.8'])],
  })
  // No ticks between pushes: an earlier guide's router.replace can still be
  // pending when the user requests a different dependency.
  await router.push({ name: 'mods-online', query: exactQuery() })
  await router.push({ name: 'mods-online', query: exactQuery('diy', 'v0.10.8') })
  await settle()
  assert.equal(guide.target.value?.repositoryId, 'diy')
  assert.equal(guide.target.value?.tag, 'v0.10.8')
  assert.deepEqual(calls.filter(call => call[0] === 'open').at(-1), ['open', 'diy'])
  assert.deepEqual({ ...router.currentRoute.value.query }, {})
})

test('consecutive navigation handles a changed tag, a shared tag across repositories and an identical intent', async t => {
  for (const [first, second] of [
    [exactQuery(), exactQuery('runtime', 'v4.0.0')],
    [exactQuery(), exactQuery('diy', 'v3.10.2')],
    [exactQuery(), exactQuery()],
  ]) {
    const { router, guide, calls } = await mountedGuide(t, {
      repositories: [repository(), repository('diy', ['v3.10.2'])],
    })
    await router.push({ name: 'mods-online', query: first })
    await router.push({ name: 'mods-online', query: second })
    await settle()
    assert.equal(guide.target.value?.repositoryId, second.focus, JSON.stringify(second))
    assert.equal(guide.target.value?.tag, second.installTag, JSON.stringify(second))
    assert.deepEqual(calls.filter(call => call[0] === 'open').at(-1), ['open', second.focus])
  }
})

test('requesting the same exact target again after query cleanup starts a fresh guide', async t => {
  const { router, guide, calls } = await mountedGuide(t, { query: exactQuery() })
  await settle()
  const firstRequest = guide.target.value.request
  await router.push({ name: 'mods-online', query: exactQuery() })
  await settle()
  assert.equal(guide.target.value.repositoryId, 'runtime')
  assert.equal(guide.target.value.tag, 'v3.10.2')
  assert.ok(guide.target.value.request > firstRequest)
  assert.deepEqual(calls.filter(call => call[0] === 'open'), [['open', 'runtime'], ['open', 'runtime']])
})

test('the transition gate rechecks the exact tag after refresh and refuses a missing old version', async t => {
  const { router, repos, guide, calls } = await mountedGuide(t)
  expectPageScroll(true)
  await router.push({ name: 'mods-online', query: exactQuery() })
  await settle()
  assert.deepEqual(calls, [['reveal', 'runtime']])
  repos.value = [repository('runtime', ['v4.0.0'])]
  endNavigation()
  await settle()
  assert.equal(guide.target.value, undefined)
  assert.deepEqual(calls, [['reveal', 'runtime'], ['unavailable']])
})

test('cancel and unmount during the page transition suppress pending focus and open callbacks', async t => {
  for (const action of ['cancel', 'unmount']) {
    const { router, guide, calls, unmount } = await mountedGuide(t)
    expectPageScroll(true)
    await router.push({ name: 'mods-online', query: exactQuery() })
    await settle()
    assert.deepEqual(calls, [['reveal', 'runtime']])
    if (action === 'cancel') guide.cancel()
    else unmount()
    endNavigation()
    await settle()
    assert.equal(guide.target.value, undefined)
    assert.deepEqual(calls, [['reveal', 'runtime']])
  }
})

test('a newer request arriving during a page transition invalidates the older reveal', async t => {
  const { router, guide, calls } = await mountedGuide(t, {
    repositories: [repository(), repository('diy', ['v0.11.0', 'v0.10.8'])],
  })
  expectPageScroll(true)
  await router.push({ name: 'mods-online', query: exactQuery() })
  await settle()
  await router.push({ name: 'mods-online', query: exactQuery('diy', 'v0.10.8') })
  await settle()
  endNavigation()
  await settle()
  assert.equal(guide.target.value.repositoryId, 'diy')
  assert.deepEqual(calls, [['reveal', 'runtime'], ['reveal', 'diy'], ['focus', 'diy'], ['open', 'diy']])
})

test('MOD detail selects a guided old Release and asset, then blocks if the cached tag disappears', async t => {
  const installs = []
  const { props, state } = mountedComponent(t, ModRepositoryDetail,
    { repo: repository(), installGuide: target() }, { onInstall: (...args) => installs.push(args) })
  assert.equal(state.selectedTag, 'v3.10.2')
  assert.equal(state.selectedRelease.tag_name, 'v3.10.2')
  assert.equal(state.primaryAsset.name, 'v3.10.2.zip')
  assert.deepEqual(installs, [], 'preparing a version must never install it')
  props.value = { ...props.value, repo: repository('runtime', ['v4.0.0']) }
  await settle()
  assert.equal(state.selectedTag, 'v3.10.2')
  assert.equal(state.selectedRelease, null)
  assert.equal(state.primaryAsset, null)
  assert.equal(state.guideMissing, true)
  state.installSelected('v4.0.0', 'v4.0.0.zip')
  assert.deepEqual(installs, [], 'missing exact target cannot fall back to latest')
})

test('ordinary MOD details pick the default tag as data arrives, preserve a cancelled guide selection and reset on repository change', async t => {
  const { props, state } = mountedComponent(t, ModRepositoryDetail, { repo: repository('runtime', []) })
  assert.equal(state.selectedTag, null)
  assert.equal(state.selectedRelease, null)
  props.value = { repo: repository() }
  await settle()
  assert.equal(state.selectedTag, 'v4.0.0')
  props.value = { ...props.value, installGuide: target() }
  await settle()
  assert.equal(state.selectedTag, 'v3.10.2')
  props.value = { ...props.value, installGuide: undefined }
  await settle()
  assert.equal(state.guided, false)
  assert.equal(state.selectedTag, 'v3.10.2', 'removing a guide must not clear a valid version selection')
  props.value = { repo: repository('diy', ['v0.11.0', 'v0.10.8']) }
  await settle()
  assert.equal(state.selectedTag, 'v0.11.0')
  assert.equal(state.selectedRelease.tag_name, 'v0.11.0')
})

test('explicit version changes cancel the old guide and select the chosen version without installing', async t => {
  const installs = []
  const { props, state } = mountedComponent(t, ModRepositoryDetail,
    { repo: repository(), installGuide: target() }, { onInstall: (...args) => installs.push(args) })
  state.selectVersion('v4.0.0')
  await settle()
  assert.equal(state.guided, false)
  assert.equal(state.selectedRelease.tag_name, 'v4.0.0')
  assert.deepEqual(installs, [])
  props.value = { ...props.value, installGuide: target(2) }
  await settle()
  assert.equal(state.guided, true)
  assert.equal(state.selectedRelease.tag_name, 'v3.10.2')
  state.installSelected('v3.10.2', 'v3.10.2.zip')
  assert.deepEqual(installs, [['v3.10.2', 'v3.10.2.zip']])
})

test('MOD detail shows the selected Release verdict and leaves declarations unassessed with no selection', async t => {
  const relations = [{ kind: 'requires', guid: 'dev.loader', version: '>=2' }]
  evaluationSnapshot([
    { id: 'mod:runtime', repo: 'runtime', dlls: ['Runtime.dll'], rows: [{ relation: relations[0], state: 'disabled' }] },
    { id: 'subject:runtime:v4.0.0', repo: 'runtime', tag: 'v4.0.0', enabled: false, rows: [{ relation: relations[0], state: 'satisfied' }] },
    { id: 'subject:runtime:v3.10.2', repo: 'runtime', tag: 'v3.10.2', enabled: false,
      candidates: [{ id: 'release:runtime:v3.10.2', tag: 'v3.10.2' }], rows: [{ relation: relations[0], state: 'missing' }] },
  ])
  t.after(clearEvaluation)
  const { props, state } = mountedComponent(t, ModRepositoryDetail, { repo: { ...repository(), relations } })
  assert.equal(state.dependencyRows[0]?.status, 'tools.relationPassed')
  assert.equal(state.dependencyRows[0]?.jump, undefined)
  state.selectVersion('v3.10.2')
  await nextTick()
  assert.equal(state.dependencyRows[0]?.reason, 'modChanges.relation.missing')
  assert.deepEqual(state.dependencyRows[0]?.jump?.repair, { node_id: 'mod:runtime', candidate_id: 'release:runtime:v3.10.2' })
  props.value = { repo: { ...repository('runtime', ['v4.0.0']), relations }, installGuide: target() }
  await nextTick()
  assert.equal(state.selectedRelease, null)
  assert.equal(state.dependencyRows[0]?.requirement, '>=2')
  assert.equal(state.dependencyRows[0]?.status, undefined)
  assert.equal(state.dependencyRows[0]?.jump, undefined)
})

test('map detail switches between exact Release assessments and does not use the installed or latest verdict without selection', async t => {
  const relations = [{ kind: 'requires', guid: 'dev.loader', version: '>=2' }]
  const repo = { ...repository('map'), relations, latest_tag: 'v4.0.0' }
  evaluationSnapshot([
    { id: 'map:Installed', kind: 'map', dir: 'Installed', map: repo.id, rows: [{ relation: relations[0], state: 'disabled' }] },
    { id: 'subject:map:v4.0.0', kind: 'map', map: repo.id, tag: 'v4.0.0', enabled: false,
      rows: [{ relation: relations[0], state: 'satisfied' }] },
    { id: 'subject:map:v3.10.2', kind: 'map', map: repo.id, tag: 'v3.10.2', enabled: false,
      rows: [{ relation: relations[0], state: 'unknown', reason: 'relation_metadata_unknown' }] },
  ])
  t.after(clearEvaluation)
  const maps = shallowRef([repo])
  const { state } = mountedComponent(t, { setup: () => useMapReleaseDetail({
    maps, detailId: repo.id, mods: [], repositories: [], mapName: entry => entry.id,
  }) }, {})
  assert.equal(state.detailDependencies[0]?.status, 'tools.relationPassed')
  state.detailSelectedTag = 'v3.10.2'
  await nextTick()
  assert.equal(state.detailDependencies[0]?.status, 'tools.relationUnconfirmed')
  assert.equal(state.detailDependencies[0]?.reason, 'modChanges.reason.metadata')
  assert.equal(state.detailDependencies[0]?.jump, undefined)
  maps.value = [{ ...repo, cached_releases: [] }]
  await nextTick()
  assert.equal(state.detailRelease, null)
  assert.equal(state.detailDependencies[0]?.requirement, '>=2')
  assert.equal(state.detailDependencies[0]?.status, undefined)
  assert.equal(state.detailDependencies[0]?.jump, undefined)
})

test('panel waits for drawer readiness, opens the install tab and focuses the primary action without installation', async t => {
  const installs = [], focuses = []
  const { props, state } = mountedComponent(t, ReleaseDetailPanel, {
    title: 'Runtime', provider: 'github', owner: 'sample', repository: 'runtime',
    version: 'v3.10.2', assets: [{ name: 'runtime.zip' }], installGuide: target(), guideReady: false,
  }, { onInstall: (...args) => installs.push(args) })
  state.panelElement = { querySelector: selector => selector === '.install-primary'
    ? { focus: options => focuses.push(options) } : null }
  await settle()
  assert.equal(state.activeTab, 'info')
  assert.equal(state.guideHighlighted, false)
  props.value = { ...props.value, guideReady: true }
  await settle()
  assert.equal(state.activeTab, 'install')
  assert.equal(state.guideHighlighted, true)
  assert.deepEqual(focuses, [{ preventScroll: true }])
  assert.deepEqual(installs, [])
  state.runPrimaryAction()
  assert.deepEqual(installs, [['v3.10.2', 'runtime.zip']])
})

test('panel guards missing assets, blocked game, in-flight action and unavailable targets at the action boundary', async t => {
  for (const extra of [ { assets: [] }, { gameBlocked: true }, { installing: true }, { actionDisabled: true } ]) {
    const installs = []
    const { state } = mountedComponent(t, ReleaseDetailPanel, {
      title: 'Runtime', provider: 'github', owner: 'sample', repository: 'runtime',
      version: 'v3.10.2', assets: [{ name: 'runtime.zip' }], ...extra,
    }, { onInstall: (...args) => installs.push(args) })
    state.runPrimaryAction()
    assert.deepEqual(installs, [], JSON.stringify(extra))
  }
})

test('cancelling a prepared panel clears its highlight and ordinary details keep the information tab', async t => {
  const { props, state } = mountedComponent(t, ReleaseDetailPanel, {
    title: 'Runtime', provider: 'github', owner: 'sample', repository: 'runtime',
    version: 'v3.10.2', assets: [{ name: 'runtime.zip' }], installGuide: target(), guideReady: true,
  })
  await settle()
  assert.equal(state.activeTab, 'install')
  assert.equal(state.guideHighlighted, true)
  props.value = { ...props.value, installGuide: undefined, guideReady: false }
  await settle()
  assert.equal(state.guideHighlighted, false)
  const plain = mountedComponent(t, ReleaseDetailPanel, {
    title: 'Runtime', provider: 'github', owner: 'sample', repository: 'runtime', guideReady: true,
  })
  await settle()
  assert.equal(plain.state.activeTab, 'info')
  assert.equal(plain.state.guideHighlighted, false)
})

test('production templates connect exact version preparation and primary click through existing installation events', () => {
  const mod = readFileSync(new URL('../src/components/ModRepositoryDetail.vue', import.meta.url), 'utf8')
  const panel = readFileSync(new URL('../src/components/ReleaseDetailPanel.vue', import.meta.url), 'utf8')
  assert.match(mod, /:version="selectedTag"/)
  assert.match(mod, /@update:version="selectVersion"/)
  assert.match(mod, /:action-disabled="guideMissing"/)
  assert.match(mod, /@install="installSelected"/)
  assert.match(panel, /@click="runPrimaryAction"/)
})
