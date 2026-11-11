import assert from 'node:assert/strict'
import test from 'node:test'
import { levelSetCardAction, levelSetCardTags, levelSetState, mapVersionForLevelSet, mapVersionRefs } from '../src/levelSetStatus.ts'

const installed = { dir_name: 'map', name: 'Map', author: 'Author', enabled: false, version: '1.0' }
const remote = { id: 'online-map', level_set_name: 'Map', author: 'Author', latest_tag: 'v2.0' }

for (const [name, local, online] of [
  ['release version', installed, remote],
  ['info version', installed, { ...remote, latest_info_version: '2.0' }],
  ['asset version', { ...installed, installed_mapset_version: '1.0' }, { ...remote, latest_mapset_version: '2.0' }],
]) {
  test(`disabled maps retain their update reminder and action using ${name}`, () => {
    assert.equal(levelSetState(local, mapVersionForLevelSet(local, mapVersionRefs([online]))), 'update_available')
    assert.ok(levelSetCardTags(local, [online], [], false).some(tag => tag.text === 'tools.update'))
    assert.equal(levelSetCardAction(local, [online], [], false)?.kind, 'update')
    assert.equal(levelSetCardAction(local, [online], [], false)?.mapId, online.id)
    assert.equal(local.enabled, false)
  })
}

test('disabled maps without an update keep their disabled status and no update action', () => {
  for (const online of [undefined, { ...remote, latest_tag: 'v1.0' }, { ...remote, latest_tag: 'v0.9' }]) {
    const maps = online ? [online] : []
    assert.equal(levelSetState(installed, mapVersionForLevelSet(installed, mapVersionRefs(maps))), 'disabled')
    assert.equal(levelSetCardAction(installed, maps, [], false), undefined)
  }
})

test('disabled map updates wait for refresh completion and readable metadata', () => {
  const refreshing = { maps: new Set([remote.id]) }
  assert.equal(levelSetCardAction(installed, [remote], [], false, [], refreshing), undefined)
  assert.ok(levelSetCardTags(installed, [remote], [], false, [], refreshing).some(tag => tag.text === 'tools.refreshing'))
  assert.equal(levelSetCardAction({ ...installed, error: 'levelSet.infoMissing' }, [remote], [], false), undefined)
})
