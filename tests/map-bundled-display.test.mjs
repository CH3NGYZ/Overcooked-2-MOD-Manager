import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { mapBundleLabel } from '../src/mapBundleLabel.ts'

test('map bundle label prefers the active language and uses one combined fallback tag', () => {
  const name = { label: 'English', name_i18n: { en: 'Bound English' }, name_zh: '中文' }
  assert.equal(mapBundleLabel(name, 'en'), 'Bound English')
  assert.equal(mapBundleLabel(name, 'zh-CN'), '中文 · English')
  assert.equal(mapBundleLabel({ ...name, name_zh: 'English' }, 'zh-CN'), 'English')
})

test('map-owned labels use the source translation only when the active locale exists', () => {
  const source = { 'zh-CN': '地图源中文名', en: 'Source name' }
  assert.equal(source['zh-CN'] || '', '地图源中文名')
  assert.equal(source['zh-TW'] || '', '')
  assert.equal(mapBundleLabel({ label: 'Map name', name_zh: '地图名' }, 'zh-TW'), '地图名 · Map name')
})

test('read-only map bundle rendering uses scanned DLLs and never enters ordinary MOD operations', () => {
  const mods = readFileSync(new URL('../src/views/ModsLocalView.vue', import.meta.url), 'utf8')
  const card = readFileSync(new URL('../src/components/LevelSetPluginCard.vue', import.meta.url), 'utf8')
  const groups = readFileSync(new URL('../src/components/LevelSetGroupBody.vue', import.meta.url), 'utf8')
  assert.match(mods, /levelSets\.value\.flatMap\(set =>[\s\S]*?set\.plugins\.map\(plugin =>/)
  assert.match(mods, /data-map-bundled-group/)
  assert.match(mods, /:data-map-owned="set\.dir_name"/)
  assert.match(card, /plugin\.path \|\| plugin\.file_name/)
  assert.match(card, /:switchable="false"/)
  assert.match(card, /sourceName \|\| name \|\| set\.dir_name/)
  assert.match(groups, /LevelSetPluginCard v-for="plugin in set\.plugins"/)
  assert.doesNotMatch(card, /@toggle=|@click=|services\/tauri/)
})
