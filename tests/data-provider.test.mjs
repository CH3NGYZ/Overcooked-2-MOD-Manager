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
  dashboard.config.value = { data_provider: 'cloudflare' }
  assert.equal(active.value, 'cloudflare')
  dashboard.config.value = { data_provider: 'builtin' }
  assert.equal(active.value, 'builtin')
  demoActive.value = true
  assert.equal(active.value, 'github')
  demoActive.value = false
  assert.equal(active.value, 'builtin')
})

test('all four choices have five-language labels and Cloudflare is a global data source', () => {
  for (const suffix of ['Github', 'Gitee', 'Cloudflare', 'Builtin']) {
    for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) assert.ok(settings[`dataProvider${suffix}`][locale])
    assert.ok(view.includes(`t('settings.dataProvider${suffix}')`))
  }
  assert.equal(settings.dataProviderCloudflare['zh-CN'], 'Cloudflare（全球）')
  assert.match(read('src/types.ts'), /DataProvider = 'github' \| 'gitee' \| 'cloudflare' \| 'builtin'/)
})

test('runtime selection events re-read shared config and both refresh engines publish the active source', () => {
  const app = read('src/App.vue'), service = read('src/services/tauri.ts')
  assert.match(app, /watchDataProvider\(\(\) => \{ void useDashboard\(\)\.reloadConfig\(\)/)
  assert.match(app, /onBeforeUnmount\(\(\) => \{ providerDisposed = true; providerUnlisten\?\.\(\)/)
  assert.match(service, /listen\('data-provider-changed', \(\) => callback\(\)\)/)
  assert.match(read('src-tauri/src/lib.rs'), /provider_events\.emit\("data-provider-changed", \(\)\)/)
  assert.match(read('src/composables/useMapSources.ts'), /finally \{[\s\S]*await useDashboard\(\)\.reloadConfig\(\{\s*project: \(current, received\) => \(\{ \.\.\.current, data_provider: received\.data_provider \}\)/)
  const mods = read('src/composables/useModRepositoryRefresh.ts')
  assert.match(mods, /await reloadConfig\(\{ throwOnError: true, project: sourceConfigProjection\(rootId, sourceId, pass\) \}\)/)
  assert.match(mods, /return \{ \.\.\.current, data_provider: received\.data_provider, repositories, repository_sources \}/)
  assert.doesNotMatch(mods, /catch \(reason\) \{\s*await reloadConfig\(\)/, 'failed source reads must not publish an unrelated full config snapshot')
})
