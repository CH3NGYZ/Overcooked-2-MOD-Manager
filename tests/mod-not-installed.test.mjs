import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'
import { modRepositoryForGuid } from '../src/repositoryAvailability.ts'
import { modsLocal } from '../src/locale/modsLocal.ts'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const guid = 'dev.gua.overcooked.diychef'
const repo = (extra = {}) => ({ id: 'chef-source', plugin_guid: guid, enabled: true, cached_releases: [], ...extra })
const config = (repositories = [], extra = {}) => ({ repositories, repository_sources: [], region: 'international', platform: 'windows', ...extra })
const boundary = { config: shallowRef(null), links: [] }
globalThis.__missingModBoundary = boundary

// Mount both production SFCs. Only navigation, dashboard data, translations and
// Naive leaves are boundaries; computed selection and rendering remain real.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'missing-mod:naive-ui', shortCircuit: true }
    if (context.parentURL?.startsWith('missing-mod:')) return next(specifier, { ...context, parentURL: import.meta.url })
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (/\/(?:ModNotInstalled|ContentState)\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'missing-mod-state', inlineTemplate: true }).content }
    }
    const modules = {
      'missing-mod:naive-ui': `
        import { h } from 'vue'
        export const NButton = {setup: (_, {slots}) => () => h('button', {}, slots.default?.())}
        export const NSpin = {setup: () => () => h('spinner')}
        export const NEmpty = {props: ['description'], setup: (props, {slots}) => () => h('empty', {description: props.description}, slots.extra?.())}
      `,
      '/src/composables/useDashboardStore.ts': 'export const useDashboard = () => ({config: globalThis.__missingModBoundary.config})',
      '/src/crossLink.ts': 'export const useModRepositoryLink = () => ({linkToModRepository: id => globalThis.__missingModBoundary.links.push(id)})',
      '/src/i18n.ts': 'export const t = (key, args) => args?.name ? `missing:${args.name}` : key',
    }
    for (const [suffix, source] of Object.entries(modules)) {
      if (url.endsWith(suffix)) return { format: 'module', shortCircuit: true, source }
    }
    return next(url, context)
  },
})
const { default: ModNotInstalled } = await import('../src/components/ModNotInstalled.vue')
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
  setElementText: (node, text) => { node.text = text },
  parentNode: node => node.parent,
  nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
})
function find(node, tag) {
  if (node.tag === tag) return node
  return node.children?.map(child => find(child, tag)).find(Boolean)
}

test('GUID navigation locates a loaded visible card without requiring cached Release data or release mode', () => {
  const raw = repo({ plugin_guid: ` ${guid.toUpperCase()} `, download_mode: 'repository_file' })
  assert.strictEqual(modRepositoryForGuid(guid, config([raw])), raw)
  assert.equal(modRepositoryForGuid(' ', config([raw])), undefined)
  assert.equal(modRepositoryForGuid('unknown.guid', config([raw])), undefined)
  assert.equal(modRepositoryForGuid(guid, null), undefined)
})

test('GUID navigation skips cards hidden by enabled, platform, region or removed sources', () => {
  for (const hidden of [
    { enabled: false }, { platform: 'macos' }, { region: 'chinese_mainland' },
    { source_id: 'removed-source' }, { source_id: 'github-community' },
  ]) {
    const available = repo({ id: 'visible' })
    assert.strictEqual(modRepositoryForGuid(guid, config([repo(hidden), available])), available)
    assert.equal(modRepositoryForGuid(guid, config([repo(hidden)])), undefined)
  }
})

test('the shared missing state reacts to loaded repositories and only routes an enabled matching button', async t => {
  boundary.config.value = config()
  boundary.links.length = 0
  const props = shallowRef({ guid, name: 'DIYChef' })
  const root = { children: [] }
  const app = renderer.createApp({ render: () => h(ModNotInstalled, props.value) })
  app.mount(root)
  t.after(() => app.unmount())
  assert.equal(find(root, 'empty').props.description, 'missing:DIYChef')
  assert.equal(find(root, 'button'), undefined)
  boundary.config.value = config([repo()])
  await nextTick()
  find(root, 'button').props.onClick()
  assert.deepEqual(boundary.links, ['chef-source'])
  props.value = { ...props.value, disabled: true, layout: 'inline' }
  await nextTick()
  find(root, 'button').props.onClick()
  assert.deepEqual(boundary.links, ['chef-source'])
  assert.match(find(root, 'div').props.class, /content-state--inline/)
  boundary.config.value = config([repo({ plugin_guid: 'different.guid' })])
  await nextTick()
  assert.equal(find(root, 'button'), undefined)
})

test('all missing-mod translations use the same name placeholder and requested Chinese wording', () => {
  const entry = modsLocal.installBeforeManaging
  assert.equal(entry['zh-CN'], '未安装 {name}，请安装后再来管理')
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) assert.match(entry[locale], /\{name\}/)
})

test('chef, local maps and drawer share missing-state routing and centered layout with tutorial isolation', () => {
  const chef = read('src/views/DiyChefView.vue'), levels = read('src/views/LevelsView.vue')
  const panel = read('src/components/settings/DiyLevelPanel.vue'), shared = read('src/components/ModNotInstalled.vue')
  assert.match(chef, /<ModNotInstalled[^>]*opened\?\.status === 'not_installed'[^>]*guid="dev\.gua\.overcooked\.diychef"/)
  assert.doesNotMatch(chef, /chef-empty|chef-loading|router\.push/)
  assert.match(chef, /\.diychef-page \{[^}]*display: flex;[^}]*flex: 1 1 auto;/)
  assert.match(levels, /demoActive\.value \|\| \(modsStatus\.value === 'ready' && !!diyLevelTarget\.value\)/)
  assert.match(levels, /!demoActive\.value && modsStatus\.value === 'ready' && !diyLevelTarget\.value/)
  assert.match(levels, /!diyLevelTarget\.value && !localReadError\.value/)
  assert.match(levels, /modsStatus === 'error' \|\| localReadError/)
  assert.match(levels, /<ModNotInstalled[^>]*diyLevelMissing[^>]*guid="dev\.gua\.overcooked\.diylevel"/)
  assert.match(levels, /realSetsTarget\.value === diyLevelTarget\.value \? realSets\.value : \[\]/)
  assert.match(panel, /<ModNotInstalled[^>]*layout="inline"/)
  assert.doesNotMatch(panel, /cached_releases|installableRepository/)
  assert.match(shared, /useModRepositoryLink\(\)/)
  assert.match(shared, /v-if="repository" #extra/)
  assert.doesNotMatch(shared, /router\.push|api\.|https?:\/\//)
  for (const page of ['ModsOnline', 'ModsLocal', 'MapsOnline', 'Levels']) {
    const source = read(`src/views/${page}View.vue`)
    assert.match(source, /<ContentState\b/)
    assert.doesNotMatch(source, /<n-empty\b/)
  }
  assert.match(read('src/components/PageLoading.vue'), /<ContentState v-if="show" loading/)
  const state = read('src/components/ContentState.vue')
  assert.match(state, /align-items: center;/)
  assert.match(state, /justify-content: center;/)
  assert.match(state, /\.content-state--fill \{ flex: 1 1 auto; \}/)
  assert.match(state, /\.content-state--inline \{ flex: 0 0 auto; \}/)
  assert.doesNotMatch(state, /position: fixed|vh|transform:/)
})
