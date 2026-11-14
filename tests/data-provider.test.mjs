import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { computed, ref } from 'vue'
import { settings } from '../src/locale/settings.ts'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const view = read('src/views/SettingsView.vue')

test('Settings displays the live active provider after fallback and preserves tutorial isolation', () => {
  const declaration = view.split('\n').find(line => line.startsWith('const dataProvider = computed('))
  assert.ok(declaration)
  const demoActive = ref(false), config = ref({ data_provider: 'github' })
  const dashboard = { config: ref({ data_provider: 'gitee' }) }
  const active = new Function('computed', 'demoActive', 'config', 'dashboard', `${declaration}; return dataProvider`)(computed, demoActive, config, dashboard)
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
