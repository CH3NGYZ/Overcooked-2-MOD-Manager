import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, ref, shallowRef } from 'vue'
import { settings } from '../src/locale/settings.ts'
import { download } from '../src/locale/installPipeline.ts'
import downloadDefaults from '../src/downloadDefaults.json' with { type: 'json' }

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const boundary = globalThis.__downloadSettingsBoundary = {
  demoActive: ref(false), calls: [], snapshots: [], notices: [], locale: ref('zh-CN'),
  sharedConfig: shallowRef(null),
  messages: settings, save: async () => { throw new Error('unexpected save') },
}

// Compile the production group and its production row/section components. IPC,
// shared state and Naive leaves are fixtures; no user configuration is opened.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'download-settings:naive', shortCircuit: true }
    if (specifier === '@vicons/ionicons5') return { url: 'download-settings:icons', shortCircuit: true }
    if (context.parentURL?.startsWith('download-settings:')) return next(specifier, { ...context, parentURL: import.meta.url })
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'download-settings:naive') return fake(`
      import { h } from 'vue'
      export const NSelect={props:['value','options','loading','disabled'],emits:['update:value'],
        setup:(props,{emit})=>()=>h('select',{value:props.value,options:props.options,
          loading:props.loading,disabled:props.disabled,'onUpdate:value':value=>emit('update:value',value)})}`)
    if (url === 'download-settings:icons') return fake('export const ChevronForwardOutline={} ')
    if (/\/components\/settings\/(?:SettingsDownloadGroup|SettingsGroup|SettingsRow)\.vue$/.test(url)) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'download-settings-test', inlineTemplate: true }).content }
    }
    const modules = {
      '/src/services/tauri.ts': `export const api={saveSettings:patch=>{
        const b=globalThis.__downloadSettingsBoundary;b.calls.push(patch);return b.save(patch)}}`,
      '/src/composables/useDashboardStore.ts': `export const useDashboard=()=>({
        config:globalThis.__downloadSettingsBoundary.sharedConfig,
        applyConfigSnapshot:config=>{const b=globalThis.__downloadSettingsBoundary;b.snapshots.push(config);b.sharedConfig.value=config}})`,
      '/src/tutorial/demo.ts': 'export const demoActive=globalThis.__downloadSettingsBoundary.demoActive',
      '/src/useNotify.ts': `export const useNotify=()=>({
        success:text=>globalThis.__downloadSettingsBoundary.notices.push(['success',text]),
        error:text=>globalThis.__downloadSettingsBoundary.notices.push(['error',text])})`,
      '/src/locale/backend.ts': 'export const backendMessageOf=error=>`translated:${error.message}`',
      '/src/i18n.ts': `export const t=(key,args)=>{
        const b=globalThis.__downloadSettingsBoundary
        return (b.messages[key.replace(/^settings\\./,'')]?.[b.locale.value]??key)
          .replace(/\{(seconds|value)\}/g,(_,param)=>String(args?.[param]??''))
      }`,
    }
    for (const [suffix, source] of Object.entries(modules)) if (url.endsWith(suffix)) return fake(source)
    return next(url, context)
  },
})
const { default: SettingsDownloadGroup } = await import('../src/components/settings/SettingsDownloadGroup.vue')
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
  setElementText: (node, text) => { node.text = text; node.children = [] },
  parentNode: node => node.parent,
  nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
})
function findAll(node, tag) {
  return [...(node.tag === tag ? [node] : []), ...(node.children?.flatMap(child => findAll(child, tag)) ?? [])]
}
const fields = ['api_timeout_seconds', 'first_byte_timeout_seconds']
const defaultNetwork = { timeout_seconds: 270, download_threads: 4, max_retry_count: 3,
  api_timeout_seconds: downloadDefaults.api_timeout_seconds, first_byte_timeout_seconds: downloadDefaults.first_byte_timeout_seconds }
const makeConfig = (network = {}) => ({ region: 'global', data_provider: 'github', network: { ...defaultNetwork, ...network } })
function mounted(t, initial = makeConfig()) {
  boundary.calls.length = boundary.snapshots.length = boundary.notices.length = 0
  boundary.demoActive.value = false; boundary.locale.value = 'zh-CN'
  boundary.sharedConfig.value = initial
  const props = shallowRef({ config: initial, disabled: false }), busy = [], saved = []
  boundary.save = async patch => ({ ...props.value.config, network: { ...props.value.config.network, ...patch } })
  const root = { children: [] }
  const app = renderer.createApp({ render: () => h(SettingsDownloadGroup, {
    ...props.value, onSaving: value => busy.push(value),
    onSaved: config => { saved.push(config); props.value = { ...props.value, config } },
  }) })
  app.mount(root)
  t.after(() => app.unmount())
  return { app, root, props, busy, saved, selects: () => findAll(root, 'select') }
}
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await nextTick() }
const change = (select, value) => select.props['onUpdate:value'](value)

test('download group follows Data sources and precedes Performance with real row controls matching region width', t => {
  const source = read('src/views/SettingsView.vue'), group = source.indexOf('<SettingsDownloadGroup :config="dataSourceConfig"')
  assert.ok(source.indexOf('data-testid="data-sources"') < group)
  assert.ok(group < source.indexOf('data-tutorial="settings-performance"'))
  assert.match(source, /:disabled="switchingDataProvider"[\s\S]*?@saved="config = \$event" @saving="savingDownload = \$event"/)
  const { root, selects } = mounted(t)
  assert.equal(findAll(root, 'section')[0].props['data-testid'], 'download-settings')
  assert.equal(selects().length, 2)
  assert.deepEqual(selects().map(node => node.props['data-testid']), fields.map(key => `download-${key}`))
  for (const select of selects()) {
    assert.equal(select.props.style.width, '304px')
    assert.equal(select.props.style['max-width'], '100%')
    assert.ok(select.props['aria-label'])
    assert.equal(select.props.disabled, false)
    assert.equal(select.props.loading, false)
  }
})

test('both timeouts offer all 26 integer seconds from 5 through 30, defaulting to 20', t => {
  const { selects } = mounted(t)
  assert.deepEqual(selects().map(node => node.props.value), [20, 20])
  for (const select of selects()) assert.deepEqual(select.props.options.map(option => option.value), Array.from({ length: 26 }, (_, index) => index + 5))
  assert.deepEqual(boundary.calls, [])
})

test('missing individual timeout fields use the shared defaults instead of timeout_seconds', t => {
  const config = makeConfig(); config.network = { timeout_seconds: 300, download_threads: 4, max_retry_count: 3 }
  const { selects } = mounted(t, config)
  assert.deepEqual(selects().map(node => node.props.value), [20, 20])
})

test('each select sends only its flat patch and applies the returned normalized config to shared state', async t => {
  const { selects, props, saved, busy } = mounted(t, makeConfig({ api_timeout_seconds: 12, first_byte_timeout_seconds: 19 }))
  assert.deepEqual(selects().map(node => node.props.value), [12, 19])
  for (const [index, value] of [30, 5].entries()) { change(selects()[index], value); await flush() }
  assert.deepEqual(boundary.calls, fields.map((key, index) => ({ [key]: [30, 5][index] })))
  assert.deepEqual(busy, [true, false, true, false])
  assert.equal(saved.length, 2)
  assert.strictEqual(boundary.snapshots.at(-1), props.value.config)
  assert.deepEqual(selects().map(node => node.props.value), [30, 5])
  assert.equal(props.value.config.network.timeout_seconds, 270)
  assert.equal(boundary.notices.length, 2)
})

test('a pending save locks both controls and rejects concurrent writes until the authoritative response', async t => {
  const state = mounted(t), request = deferred(), before = state.props.value.config
  boundary.save = () => request.promise
  change(state.selects()[0], 30); await nextTick()
  assert.deepEqual(state.busy, [true])
  assert.ok(state.selects().every(node => node.props.disabled && node.props.loading))
  assert.equal(state.selects()[0].props.value, 20)
  change(state.selects()[1], 5); change(state.selects()[0], 19)
  assert.deepEqual(boundary.calls, [{ api_timeout_seconds: 30 }])
  const result = { ...before, network: { ...before.network, api_timeout_seconds: 30 } }
  request.resolve(result); await flush()
  assert.deepEqual(state.props.value.config, result)
  assert.equal(state.selects()[0].props.value, 30)
  assert.ok(state.selects().every(node => !node.props.disabled && !node.props.loading))
  assert.deepEqual(state.busy, [true, false])
})

test('failed saves preserve all values, translate the error, unlock, and allow retry', async t => {
  const { selects, props, saved, busy } = mounted(t), before = props.value.config
  boundary.save = async () => { throw new Error('config.saveFailed') }
  change(selects()[1], 19); await flush()
  assert.strictEqual(props.value.config, before)
  assert.deepEqual(selects().map(node => node.props.value), [20, 20])
  assert.deepEqual(saved, []); assert.deepEqual(boundary.snapshots, [])
  assert.deepEqual(boundary.notices, [['error', 'translated:config.saveFailed']])
  assert.ok(selects().every(node => !node.props.disabled && !node.props.loading))
  boundary.save = async patch => ({ ...before, network: { ...before.network, ...patch } })
  change(selects()[1], 19); await flush()
  assert.equal(selects()[1].props.value, 19)
  assert.deepEqual(busy, [true, false, true, false])
})

test('tutorial, missing config and an active provider switch disable controls and block writes in the handler', async t => {
  const { selects, props } = mounted(t)
  boundary.demoActive.value = true; await nextTick()
  assert.ok(selects().every(node => node.props.disabled))
  change(selects()[0], 30)
  boundary.demoActive.value = false; props.value = { ...props.value, disabled: true }; await nextTick()
  assert.ok(selects().every(node => node.props.disabled))
  change(selects()[1], 5)
  props.value = { config: null, disabled: false }; await nextTick()
  assert.ok(selects().every(node => node.props.disabled))
  change(selects()[0], 19)
  assert.deepEqual(boundary.calls, [])
})

test('unchanged values and values outside 5..30 integers never write', async t => {
  const { selects } = mounted(t)
  for (const select of selects()) for (const value of [20, 4, 31, 10.5, NaN]) change(select, value)
  await flush()
  assert.deepEqual(boundary.calls, [])
})

test('the real options and labels update for all five locales and all new keys have every language', async t => {
  const { root, selects } = mounted(t)
  const keys = ['downloadSettings', 'downloadApiTimeout', 'downloadApiTimeoutHint', 'downloadFirstByteTimeout', 'downloadFirstByteTimeoutHint', 'downloadSeconds']
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    boundary.locale.value = locale; await nextTick()
    for (const key of keys) assert.ok(settings[key][locale])
    assert.equal(selects()[0].props['aria-label'], settings.downloadApiTimeout[locale])
    assert.equal(selects()[1].props.options[0].label, settings.downloadSeconds[locale].replace('{seconds}', '5'))
    assert.equal(selects()[1].props.options.at(-1).label, settings.downloadSeconds[locale].replace('{seconds}', '30'))
    const hints = findAll(root, 'small')
    assert.deepEqual(hints.map(node => node.props.title),
      ['ApiTimeout', 'FirstByteTimeout'].map((key, index) =>
        settings[`download${key}Hint`][locale].replace('{value}', String(defaultNetwork[fields[index]]))))
  }
})

test('first-byte timeout messages do not hardcode a duration now that the limit is configurable', () => {
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    assert.ok(download.firstByteTimeout[locale])
    assert.doesNotMatch(download.firstByteTimeout[locale], /\d/)
  }
})

test('an old request finishing after navigation cannot overwrite a new region/source snapshot', async t => {
  const state = mounted(t), request = deferred()
  boundary.save = () => request.promise
  change(state.selects()[1], 5)
  const staleHandler = state.selects()[0].props['onUpdate:value']
  state.app.unmount()
  const newer = { ...makeConfig(), region: 'cn', data_provider: 'gitcode', repository_sources: [{ id: 'new-source' }] }
  boundary.sharedConfig.value = newer
  const result = makeConfig({ first_byte_timeout_seconds: 5 })
  request.resolve(result); await flush()
  assert.deepEqual(state.saved, [])
  assert.deepEqual(boundary.notices, [])
  assert.deepEqual(boundary.snapshots, [])
  assert.strictEqual(boundary.sharedConfig.value, newer)
  staleHandler(32)
  assert.equal(boundary.calls.length, 1)
})

test('a mounted delayed save merges only its setting into a newer shared region/source snapshot', async t => {
  const state = mounted(t), request = deferred()
  boundary.save = () => request.promise
  change(state.selects()[1], 5)
  const newer = { ...makeConfig({ api_timeout_seconds: 19 }), region: 'cn', data_provider: 'gitcode', repository_sources: [{ id: 'new-source' }] }
  boundary.sharedConfig.value = newer
  request.resolve(makeConfig({ first_byte_timeout_seconds: 5 })); await flush()
  assert.deepEqual(boundary.sharedConfig.value, { ...newer, network: { ...newer.network, first_byte_timeout_seconds: 5 } })
  assert.strictEqual(state.props.value.config, boundary.sharedConfig.value)
})

test('a later shared change to the same field wins over an older save response', async t => {
  const state = mounted(t), request = deferred()
  boundary.save = () => request.promise
  change(state.selects()[0], 5)
  const newer = makeConfig({ api_timeout_seconds: 19 })
  boundary.sharedConfig.value = newer
  request.resolve(makeConfig({ api_timeout_seconds: 5 })); await flush()
  assert.equal(boundary.sharedConfig.value.network.api_timeout_seconds, 19)
  assert.equal(state.selects()[0].props.value, 19)
})
