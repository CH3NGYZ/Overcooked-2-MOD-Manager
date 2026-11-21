import assert from 'node:assert/strict'
import test from 'node:test'
import { aggregateLevelSetVersion, levelSetAggregateState, levelSetCardAction, levelSetCardTags } from '../src/levelSetStatus.ts'

const installed = { dir_name: 'map', name: 'Map', author: 'Author', enabled: false, version: '1.0' }
const remote = { id: 'online-map', level_set_name: 'Map', author: 'Author', latest_tag: 'v2.0', latest_info_version: '2.0' }

for (const [name, local, online] of [
  ['info version', installed, { ...remote, latest_info_version: '2.0' }],
  ['asset version', { ...installed, installed_mapset_version: '1.0' }, { ...remote, latest_tag: '2.0', latest_mapset_version: '2.0', asset_versions: [
    { mapset_version: '1.0', info_version: '1.0' }, { mapset_version: '2.0', info_version: '2.0' },
  ] }],
]) {
  test(`disabled maps retain their update reminder and action using ${name}`, () => {
    assert.equal(levelSetAggregateState(local, aggregateLevelSetVersion(local, [online])), 'update_available')
    assert.ok(levelSetCardTags(local, [online], [], false).some(tag => tag.text === 'tools.update'))
    assert.equal(levelSetCardAction(local, [online], [], false)?.kind, 'update')
    assert.equal(levelSetCardAction(local, [online], [], false)?.mapId, online.id)
    assert.equal(local.enabled, false)
  })
}

test('disabled maps without an update keep their disabled status and no update action', () => {
  for (const online of [undefined, { ...remote, latest_tag: 'v1.0', latest_info_version: '1.0' }, { ...remote, latest_tag: 'v0.9', latest_info_version: '0.9' }]) {
    const maps = online ? [online] : []
    assert.equal(levelSetAggregateState(installed, aggregateLevelSetVersion(installed, maps)), 'disabled')
    assert.equal(levelSetCardAction(installed, maps, [], false), undefined)
  }
})

test('disabled map updates wait for refresh completion and readable metadata', () => {
  const refreshing = { maps: new Set([remote.id]) }
  assert.equal(levelSetCardAction(installed, [remote], [], false, [], refreshing), undefined)
  assert.ok(levelSetCardTags(installed, [remote], [], false, [], refreshing).some(tag => tag.text === 'tools.refreshing'))
  assert.equal(levelSetCardAction({ ...installed, error: 'levelSet.infoMissing' }, [remote], [], false), undefined)
})
