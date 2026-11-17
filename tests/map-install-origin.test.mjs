import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { levelSetState, mapVersionRefs } from '../src/levelSetStatus.ts'
import { mapCardState, mapCardAction } from '../src/mapCardStatus.ts'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')

test('多图 ZIP 安装只透传后端选定的主地图来源，不从请求 tag 或首张图猜版本', () => {
  const flow = read('src/composables/useLevelSetImport.ts')
  assert.match(flow, /map_import: \{[\s\S]*zip_path: staged\.path, selected: sets\.map\(set => set\.dir_name\), origin: staged\.origin/)
  assert.doesNotMatch(flow, /mapsetVersion/)
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
  // 不属于此地图的随包记录不能覆盖可信info；缺共同表时不能报更新。
  assert.equal(levelSetState({ ...bundled, installed_mapset_version: '10.5' }, mapVersionRefs([music])[0]), 'latest')
})
