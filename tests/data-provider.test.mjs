import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { computed, ref } from 'vue'
import { settings } from '../src/locale/settings.ts'
import { toolsUiSettings } from '../src/locale/tools/settings.ts'
import { normalizeRepositoryRegion } from '../src/repositoryAvailability.ts'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const view = read('src/views/SettingsView.vue')

test('Settings displays the live active provider after fallback and preserves tutorial isolation', () => {
  const declarations = view.split('\n').filter(line => /^const (dataSourceConfig|dataProvider) = computed\(/.test(line)).join('\n')
  assert.ok(declarations.includes('const dataSourceConfig'))
  const demoActive = ref(false), config = ref({ data_provider: 'github' })
  const dashboard = { config: ref({ data_provider: 'gitee' }) }
  const active = new Function('computed', 'demoActive', 'config', 'dashboard', `${declarations}; return dataProvider`)(computed, demoActive, config, dashboard)
  assert.equal(active.value, 'gitee')
  dashboard.config.value = { data_provider: 'gitcode' }
  assert.equal(active.value, 'gitcode')
  dashboard.config.value = { data_provider: 'cloudflare' }
  assert.equal(active.value, 'cloudflare')
  dashboard.config.value = { data_provider: 'builtin' }
  assert.equal(active.value, 'builtin')
  demoActive.value = true
  assert.equal(active.value, 'github')
  demoActive.value = false
  assert.equal(active.value, 'builtin')
})

test('Settings displays the shared saved region, normalizes legacy values and isolates tutorial data', () => {
  const declarations = view.split('\n').filter(line => /^const (dataSourceConfig|userRegion) = computed\(/.test(line)).join('\n')
  const demoActive = ref(false), config = ref({ region: 'cn' })
  const dashboard = { config: ref({ region: 'international' }) }
  const active = new Function('computed', 'demoActive', 'config', 'dashboard', 'normalizeRepositoryRegion', `${declarations}; return userRegion`)(computed, demoActive, config, dashboard, normalizeRepositoryRegion)
  assert.equal(active.value, 'global')
  dashboard.config.value = { region: 'chinese_mainland' }
  assert.equal(active.value, 'cn')
  dashboard.config.value = { region: 'global' }
  assert.equal(active.value, 'global')
  demoActive.value = true
  assert.equal(active.value, 'cn')
  demoActive.value = false
  assert.equal(active.value, 'global')
  dashboard.config.value = null
  assert.equal(active.value, 'cn')
})

test('region choice sits above the data source with matching controls and shared tutorial and switching gates', () => {
  const group = view.match(/<SettingsGroup :title="t\('settings.dataSources'\)"[\s\S]*?<\/SettingsGroup>/)?.[0]
  assert.ok(group)
  assert.ok(group.indexOf('data-testid="user-region"') < group.indexOf('data-testid="data-provider"'))
  const controls = [...group.matchAll(/<n-select[\s\S]*?\/>/g)].map(match => match[0])
  assert.equal(controls.length, 2)
  for (const control of controls) {
    assert.ok(control.includes('style="width: 304px; max-width: 100%"'))
    assert.ok(control.includes(':loading="switchingDataProvider"'))
    assert.ok(control.includes(':disabled="switchingDataProvider || !config || demoActive"'))
    assert.ok(control.includes('switchDataSelection({'))
  }
  assert.match(controls[0], /switchDataSelection\(\{ region: value \}\)/)
  assert.match(controls[1], /switchDataSelection\(\{ data_provider: value \}\)/)
  assert.deepEqual([...view.matchAll(/\{ value: '(cn|global)', label: t\('(tools\.\w+)'\) \}/g)].map(match => [match[1], match[2]]), [
    ['cn', 'tools.mainlandChina'], ['global', 'tools.international'],
  ])
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    for (const key of ['region', 'regionHint', 'regionChanged']) assert.ok(settings[key][locale])
    for (const key of ['mainlandChina', 'international']) assert.ok(toolsUiSettings[key][locale])
  }
  assert.match(read('src/types.ts'), /UserRegion = 'cn' \| 'global'/)
  assert.match(read('src/types.ts'), /SettingsPatch \{ region\?: UserRegion;/)
})

test('Settings selection handler skips unchanged values and protects tutorial state before coordinating either choice', async () => {
  const source = view.slice(view.indexOf('async function switchDataSelection('), view.indexOf('/* ===== API 凭据 ===== */'))
    .replace('patch: DataSelectionPatch', 'patch')
  const switchingDataProvider = ref(false), demoActive = ref(false), userRegion = ref('global'), dataProvider = ref('github'), config = ref(null)
  const calls = [], notices = []
  const switchSelection = async patch => { calls.push(patch); return { config: { region: patch.region ?? 'global' }, ready: true, errors: [] } }
  const handler = new Function('switchingDataProvider', 'demoActive', 'userRegion', 'dataProvider', 'config', 'switchSelection', 'message', 't', 'fail', `${source}; return switchDataSelection`)(
    switchingDataProvider, demoActive, userRegion, dataProvider, config, switchSelection,
    { success: value => notices.push(value), warning: () => assert.fail('unexpected warning') }, key => key, error => { throw error },
  )
  await handler({ region: 'global' }); await handler({ data_provider: 'github' })
  switchingDataProvider.value = true
  await handler({ region: 'cn' }); await handler({ data_provider: 'builtin' })
  switchingDataProvider.value = false; demoActive.value = true
  await handler({ region: 'cn' }); await handler({ data_provider: 'builtin' })
  assert.deepEqual(calls, [])
  demoActive.value = false
  await handler({ region: 'cn' }); await handler({ data_provider: 'builtin' })
  assert.deepEqual(calls, [{ region: 'cn' }, { data_provider: 'builtin' }])
  assert.deepEqual(notices, ['settings.regionChanged', 'settings.dataProviderChanged'])
})

test('all five source choices have five-language labels in regional fallback order', () => {
  for (const suffix of ['Github', 'Gitee', 'Gitcode', 'Cloudflare', 'Builtin']) {
    for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) assert.ok(settings[`dataProvider${suffix}`][locale])
    assert.ok(view.includes(`t('settings.dataProvider${suffix}')`))
  }
  assert.deepEqual(
    [...view.matchAll(/\{ value: '(gitcode|gitee|github|cloudflare|builtin)', label: t\('settings\.dataProvider\w+'\) \}/g)].map(match => match[1]),
    ['gitcode', 'gitee', 'github', 'cloudflare', 'builtin'],
  )
  assert.equal(settings.dataProviderGitcode['zh-CN'], 'GitCode（中国大陆 - 推荐）')
  assert.equal(settings.dataProviderGitee['zh-CN'], 'Gitee (中国大陆 - 备用)')
  assert.equal(settings.dataProviderGithub['zh-CN'], 'Github (国际)')
  assert.equal(settings.dataProviderCloudflare['zh-CN'], 'Cloudflare (全球)')
  assert.equal(settings.dataProviderBuiltin['zh-CN'], '内置数据源 (保底)')
  assert.match(view, /\{ value: 'gitcode', label: t\('settings.dataProviderGitcode'\) \}/)
  for (const label of Object.values(settings.dataProviderBuiltinShort)) assert.ok(label)
  assert.match(read('src/types.ts'), /DataProvider = 'github' \| 'gitee' \| 'gitcode' \| 'cloudflare' \| 'builtin'/)
})

test('both installation headers show the backend active provider through one shared reactive badge', () => {
  const badge = read('src/components/InstallSourceTags.vue')
  assert.match(badge, /const \{ config \} = useDashboard\(\)/)
  assert.match(badge, /const provider = computed\(\(\) => config\.value\?\.data_provider\)/)
  assert.match(badge, /dataProviderShortLabel\(provider, t\('settings\.dataProviderBuiltinShort'\)\)/)
  assert.match(badge, /v-if="provider"/)
  assert.match(badge, /const region = computed\(\(\) => config\.value\?\.region\)/)
  assert.match(badge, /repositoryRegionLabel\(region\)/)
  assert.ok(badge.indexOf('data-testid="install-region"') < badge.indexOf(':data-provider="provider"'))
  for (const file of ['ModsOnlineView', 'MapsOnlineView']) {
    const page = read(`src/views/${file}.vue`)
    assert.match(page, /<Teleport to="#page-header-title-extra" defer>[\s\S]*?<InstallSourceTags \/>[\s\S]*?<\/Teleport>/)
  }
})

test('five-language source hints describe the current regional archive fallback routes', () => {
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    const hint = settings.dataProviderHint[locale]
    assert.ok(hint.includes('GitCode → Gitee → Cloudflare'), locale)
    assert.ok(hint.includes('GitHub → Cloudflare'), locale)
    assert.ok(hint.includes('ZIP'), locale)
  }
})

test('runtime selection events re-read shared config and both refresh engines publish the active source', () => {
  const app = read('src/App.vue'), service = read('src/services/tauri.ts')
  assert.match(app, /watchDataProvider\(\(\) => \{ void useDashboard\(\)\.reloadConfig\(\)/)
  assert.match(app, /onBeforeUnmount\(\(\) => \{ providerDisposed = true; providerUnlisten\?\.\(\)/)
  assert.match(service, /listen\('data-provider-changed', \(\) => callback\(\)\)/)
  assert.match(read('src-tauri/src/lib.rs'), /provider_events\.emit\("data-provider-changed", \(\)\)/)
  assert.match(read('src/composables/useMapSources.ts'), /finally \{[\s\S]*await useDashboard\(\)\.reloadConfig\(\{\s*project: \(current, received\) => ticket === state\.generation\.value\s*\? \{ \.\.\.current, data_provider: received\.data_provider \} : current/)
  const mods = read('src/composables/useModRepositoryRefresh.ts')
  assert.match(mods, /await reloadConfig\(\{ throwOnError: true, project: sourceConfigProjection\(rootId, sourceId, pass\) \}\)/)
  assert.match(mods, /return \{ \.\.\.current, data_provider: received\.data_provider, repositories, repository_sources \}/)
  assert.doesNotMatch(mods, /catch \(reason\) \{\s*await reloadConfig\(\)/, 'failed source reads must not publish an unrelated full config snapshot')
})
