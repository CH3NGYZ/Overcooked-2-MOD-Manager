import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { levelSetState, mapCardState, mapCardAction, mapVersionRefs } from '../src/levelSetStatus.ts'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')

test('多图 ZIP 安装只透传后端选定的主地图来源，不从请求 tag 或首张图猜版本', () => {
  const flow = read('src/composables/useLevelSetImport.ts')
  const api = read('src/services/tauri.ts')
  assert.match(flow, /api\.importLevelSets\(staged\.path, selected, staged\.origin\)/)
  assert.doesNotMatch(flow, /mapsetVersion/)
  assert.match(api, /origin: origin \?\? null/)
})

test('两张地图各用自己的版本空间，更新提示与安装按钮一致', () => {
  const local = (name, version, installed) => ({
    name, author: 'Author', version, installed_mapset_version: installed, enabled: true,
  })
  const online = (name, version) => ({
    id: name, level_set_name: name, author: 'Author', latest_tag: version,
    latest_mapset_version: version, latest_info_version: version, enabled: true,
  })
  const pear = online('pear', '11.0')
  const music = online('Music', '1.1')
  const oldPear = local('pear', '10.5', '10.5')
  const newPear = local('pear', '11.0', '11.0')
  const bundled = local('Music', '1.1', '')
  assert.equal(levelSetState(oldPear, mapVersionRefs([online('pear', '10.5')])[0]), 'latest')
  assert.equal(levelSetState(oldPear, mapVersionRefs([pear])[0]), 'update_available')
  assert.equal(levelSetState(newPear, mapVersionRefs([pear])[0]), 'latest')
  assert.equal(levelSetState(bundled, mapVersionRefs([music])[0]), 'latest')
  assert.equal(mapCardState(pear, [bundled, newPear]), 'latest')
  assert.equal(mapCardAction(pear, [bundled, newPear]), 'reinstall')
  assert.equal(mapCardState(music, [newPear, bundled]), 'latest')
  assert.equal(mapCardAction(music, [newPear, bundled]), 'reinstall')
  // 错误的主包记录确实会覆盖 info 等值，这条断言锁住本次故障条件。
  assert.equal(levelSetState({ ...bundled, installed_mapset_version: '10.5' }, mapVersionRefs([music])[0]), 'update_available')
})
