import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { aggregateLevelSetVersion, assetVersionIndex, mapsForLevelSet, mapUpdateState, mapVersionRef } from '../src/mapVersion.ts'
import { levelSetCardAction, levelSetCardTags, mapForLevelSet } from '../src/levelSetStatus.ts'
import { mapCardAction, mapCardActionLabel, mapCardState } from '../src/mapCardStatus.ts'
import { i18n } from '../src/locale/index.ts'
import { LOCALE_CODES } from '../src/locale/types.ts'

const rows = (...entries) => entries.map(([mapset_version, info_version]) => ({ mapset_version, info_version }))
const installed = (overrides = {}) => ({ dir_name: 'local', name: 'Map', name_zh: '地图', author: 'Author', version: '1', enabled: true, ...overrides })
const online = (id, info = '2', overrides = {}) => ({ id, source_id: id, level_set_name: 'Map', level_set_name_zh: '地图', author: 'Author', latest_tag: `package-${id}`, latest_info_version: info, cached_releases: [], ...overrides })
const hint = (sourceOrder = [], overrides = {}) => ({ maps: new Set(), sources: new Set(), sourceOrder, ...overrides })
const aggregate = (maps, local = installed(), refreshing) => aggregateLevelSetVersion(local, maps, refreshing)
const fixedTable = rows(['3.4.1', '3.4.1'], ['3.4.1-fix', '3.4.1'])

test('tag and publisher package numbers alone never imply a map update', () => {
  for (const local of ['1', '2', '999']) {
    assert.equal(mapUpdateState(local, { tag: 'v2', latestMapsetVersion: '2' }, local), 'unknown')
    assert.equal(mapUpdateState(local, { tag: 'v2', latestInfoVersion: '2' }), local === '1' ? 'update_available' : local === '2' ? 'latest' : 'ahead')
  }
})

test('same-info fixes require both endpoints in one complete ordered history', () => {
  const ref = { tag: '3.4.1-fix', latestInfoVersion: '3.4.1', assetVersions: fixedTable }
  assert.equal(mapUpdateState('3.4.1', ref, '3.4.1'), 'update_available')
  assert.equal(mapUpdateState('3.4.1', { ...ref, tag: '3.4.1' }, '3.4.1-fix'), 'ahead')
  assert.equal(mapUpdateState('3.4.1', { ...ref, assetVersions: [] }, '3.4.1'), 'latest')
  assert.equal(mapUpdateState('3.4.1', ref), 'latest')
})

test('author order supports 0.69 then 0.7 regardless of numeric order', () => {
  const ref = { tag: '0.7', latestInfoVersion: '0.69', assetVersions: rows(['0.69', '0.69'], ['0.7', '0.69']) }
  assert.equal(mapUpdateState('0.69', ref, '0.69'), 'update_available')
  assert.equal(mapUpdateState('0.69', { ...ref, tag: '0.69' }, '0.7'), 'ahead')
})

test('table evidence requires local and target info to agree with their own rows', () => {
  const ref = { tag: '3.4.1-fix', latestInfoVersion: '3.4.1', assetVersions: fixedTable }
  assert.equal(mapUpdateState('9', ref, '3.4.1'), 'ahead', 'mismatched local info falls back to info')
  assert.equal(mapUpdateState('3.4.1', { ...ref, latestInfoVersion: '2' }, '3.4.1'), 'ahead', 'mismatched target info cannot prove an update')
})

test('exact trimmed asset names win; semantic aliases must be unique', () => {
  const table = rows(['1', '1'], ['v1.0', '1'], ['2', '2'])
  assert.equal(assetVersionIndex(table, ' 1 '), 0)
  assert.equal(assetVersionIndex(table, 'v1.0'), 1)
  assert.equal(assetVersionIndex(table, '1.0.0'), undefined)
  assert.equal(assetVersionIndex(rows(['1', '1'], ['2', '2']), 'v1.0.0'), 0)
  assert.equal(assetVersionIndex(rows(['1', '1'], ['1', '1']), '1'), undefined)
})

test('cached Release truncation does not truncate the authoritative history', () => {
  const map = online('fix', 'obsolete', { latest_tag: '3.4.1-fix', asset_versions: fixedTable, cached_releases: [{ tag_name: '3.4.1-fix' }] })
  const result = aggregate([map], installed({ version: '3.4.1', installed_mapset_version: '3.4.1' }))
  assert.equal(result.order, 'update_available')
  assert.equal(result.evidence, 'table')
  assert.equal(result.primaryTarget.tag, '3.4.1-fix')
})

test('actual available attachment target is compared instead of a missing declared tail', () => {
  const map = online('missing', '3', { latest_tag: '2', latest_mapset_version: '3', asset_versions: rows(['1', '1'], ['2', '2'], ['3', '3']) })
  assert.equal(mapVersionRef(map).latestInfoVersion, '2')
  assert.equal(aggregate([map], installed({ version: '2', installed_mapset_version: '2' })).order, 'latest')
  assert.equal(aggregate([map], installed({ version: '3', installed_mapset_version: '3' })).order, 'ahead')
})

test('unmapped release transport tag cannot identify an actual mapset target', () => {
  const map = online('transport', '0.69', { latest_tag: 'release-2026', asset_versions: rows(['0.69', '0.69'], ['0.7', '0.69']) })
  const local = installed({ version: '0.69', installed_mapset_version: '0.69' })
  assert.equal(aggregate([map], local).order, 'unknown')
  assert.equal(mapVersionRef(map).latestMapsetVersion, '')
})


test('nonempty declaration with a blank tail or missing target remains unknown', () => {
  for (const asset_versions of [rows(['1', '1'], ['', '2']), rows(['1', '1'], ['2', '']), rows(['1', '1'], ['2', '2'])]) {
    const map = online('blank', '99', { latest_tag: 'not-in-table', latest_mapset_version: '99', asset_versions })
    const ref = mapVersionRef(map)
    assert.equal(ref.latestInfoVersion, '')
    assert.equal(aggregate([map]).order, 'unknown')
  }
})

test('consistent complete mirror history may be shared without joining tables', () => {
  const full = online('mirror', '3.4.1', { latest_tag: '3.4.1', asset_versions: fixedTable })
  const partial = online('target', '3.4.1', { latest_tag: '3.4.1-fix', asset_versions: rows(['3.4.1-fix', '3.4.1']) })
  const local = installed({ version: '3.4.1', installed_mapset_version: '3.4.1' })
  assert.equal(aggregate([partial], local).order, 'latest')
  const result = aggregate([partial, full], local)
  assert.equal(result.primaryTarget.mapId, 'target')
  assert.equal(result.primaryTarget.evidence, 'table')
  const chain = [
    online('a', '1', { latest_tag: 'a', asset_versions: rows(['old', '1'], ['a', '1']) }),
    online('b', '1', { latest_tag: 'b', asset_versions: rows(['a', '1'], ['b', '1']) }),
  ]
  assert.equal(aggregate(chain, installed({ installed_mapset_version: 'old' })).candidates.find(item => item.mapId === 'b').order, 'latest', 'old -> a -> b cannot fabricate one full history')
})

test('mapping and ordering conflicts reject table evidence and fall back to info', () => {
  const a = online('a', '1', { latest_tag: 'new', asset_versions: rows(['old', '1'], ['new', '1']) })
  for (const table of [rows(['old', '9'], ['new', '1']), rows(['new', '1'], ['old', '1'])]) {
    const b = online('b', '1', { latest_tag: 'new', asset_versions: table })
    const result = aggregate([a, b], installed({ installed_mapset_version: 'old' }))
    assert.equal(result.order, 'latest')
    assert.equal(result.candidates[0].evidence, 'info')
  }
})

test('failed and pending snapshots cannot donate their cached table evidence', () => {
  const target = online('target', '1', { latest_tag: 'new', asset_versions: rows(['new', '1']) })
  const donor = online('donor', '1', { latest_tag: 'old', asset_versions: rows(['old', '1'], ['new', '1']) })
  const local = installed({ installed_mapset_version: 'old' })
  assert.equal(aggregate([target, { ...donor, last_error: 'failed' }], local).candidates[1].order, 'latest')
  const pending = aggregate([target, donor], local, hint([], { maps: new Set(['donor']) }))
  assert.equal(pending.order, 'refreshing')
  assert.equal(pending.updateCandidates.length, 0)
  assert.equal(pending.candidates.find(item => item.mapId === 'target').order, 'latest')
})

test('GAODA candidates anchor directly to local Chinese identity without a missing-name bridge', () => {
  const gaoda = (id, zh) => online(id, '2', { level_set_name: 'GAODA', author: 'GAODA', level_set_name_zh: zh })
  const maps = [gaoda('college', '胡闹糕中'), gaoda('unknown', null), gaoda('university', '胡闹达学')]
  const local = installed({ name: 'GAODA', author: 'GAODA', name_zh: '胡闹糕中' })
  assert.deepEqual(mapsForLevelSet(local, maps).map(map => map.id), ['college', 'unknown'])
  assert.equal(aggregate(maps, local).candidates.some(candidate => candidate.mapId === 'university'), false)
  const ambiguous = aggregate(maps, { ...local, name_zh: null })
  assert.equal(ambiguous.order, 'unknown')
  assert.equal(ambiguous.updateCandidates.length, 0)
})

test('maximal updates win before declared source order; ties do not depend on completion order', () => {
  const a = online('a', '2'), b = online('b', '3'), c = online('c', '3')
  const result = aggregate([a, b, c], installed(), hint(['c', 'a', 'b']))
  assert.equal(result.primaryTarget.mapId, 'c')
  assert.equal(aggregate([c, b, a], installed(), hint(['c', 'a', 'b'])).primaryTarget.mapId, 'c')
  assert.deepEqual(result.updateCandidates.map(candidate => candidate.mapId), ['c', 'a', 'b'])
})

test('equal-info same-source updates choose stable map id without sorting a partial order', () => {
  const a = online('a', '2', { source_id: 'same' }), z = online('z', '2', { source_id: 'same' })
  assert.equal(aggregate([z, a], installed(), hint(['same'])).primaryTarget.mapId, 'a')
})

test('a global three-source order cycle stays unknown without inventing a local-newer conclusion or update action', () => {
  const local = installed({ installed_mapset_version: 'local' })
  const a = online('a', '1', { latest_tag: 'A', asset_versions: rows(['local', '1'], ['A', '1'], ['B', '1']) })
  const b = online('b', '1', { latest_tag: 'B', asset_versions: rows(['local', '1'], ['B', '1'], ['C', '1']) })
  const c = online('c', '1', { latest_tag: 'C', asset_versions: rows(['local', '1'], ['C', '1'], ['A', '1']) })
  for (const maps of [[a, b, c], [a, c, b], [b, a, c], [b, c, a], [c, a, b], [c, b, a]]) {
    const refreshing = hint(maps.map(map => map.source_id).reverse())
    const result = aggregate(maps, local, refreshing)
    assert.equal(result.updateCandidates.length, 3, 'each source still proves an update from the local asset')
    assert.equal(result.order, 'unknown', 'inconsistent global order cannot mean that the local version is newer')
    assert.equal(result.evidence, 'unknown')
    assert.equal(result.primaryTarget, undefined)
    assert.equal(levelSetCardTags(local, maps, [], false, [], refreshing).at(-1).text, 'tools.versionUnknown')
    assert.equal(levelSetCardAction(local, maps, [], false, [], refreshing), undefined)
  }
})

test('an acyclic table partial order keeps its maximal update across source order changes', () => {
  const local = installed({ installed_mapset_version: 'local' })
  const a = online('a', '1', { latest_tag: 'A', asset_versions: rows(['local', '1'], ['A', '1'], ['B', '1']) })
  const b = online('b', '1', { latest_tag: 'B', asset_versions: rows(['local', '1'], ['B', '1'], ['C', '1']) })
  const c = online('c', '1', { latest_tag: 'C', asset_versions: rows(['local', '1'], ['C', '1']) })
  for (const maps of [[a, b, c], [c, a, b], [b, c, a]]) {
    const refreshing = hint(maps.map(map => map.source_id))
    const result = aggregate(maps, local, refreshing)
    assert.equal(result.order, 'update_available')
    assert.equal(result.primaryTarget.mapId, 'c')
    assert.equal(result.evidence, 'table')
    assert.deepEqual(levelSetCardAction(local, maps, [], false, [], refreshing), {
      kind: 'update', label: 'common.action.update', tone: 'moderate', mapId: 'c', tag: 'C',
    })
  }
})

test('completed trusted update precedes pending; pending precedes unknown and equality', () => {
  const ready = online('ready', '2'), waiting = online('waiting', '9'), unknown = online('unknown', '')
  const refreshing = hint([], { queued: new Set(['waiting']) })
  assert.equal(aggregate([unknown, waiting, ready], installed(), refreshing).order, 'update_available')
  assert.equal(aggregate([unknown, waiting], installed(), refreshing).order, 'refreshing')
  assert.equal(aggregate([online('same', '1'), unknown]).order, 'unknown')
  assert.equal(aggregate([online('same', '1'), online('old', '0.9')]).order, 'latest')
  assert.equal(aggregate([online('old', '0.9')]).order, 'ahead')
  assert.equal(aggregate([]).order, 'unbound')
})

test('disabled local updates retain exact target map id/tag and dependency source remains unchanged', () => {
  const old = online('old', '1'), newer = online('new', '2', { latest_tag: 'actual-install-tag' })
  const local = installed({ enabled: false })
  const action = levelSetCardAction(local, [old, newer], [], false)
  assert.deepEqual([action.kind, action.mapId, action.tag], ['update', 'new', 'actual-install-tag'])
  assert.equal(mapForLevelSet(local, [old, newer]), old)
  assert.equal(levelSetCardTags(local, [old, newer], [], false).at(-1).mapId, 'new')
})

test('each source card describes ahead, same, or incomparable and offers explicit installation', () => {
  const local = installed({ version: '2' }), older = online('old', '1'), same = online('same', '2'), unknown = online('unknown', '')
  for (const [map, state, action, label] of [[older, 'ahead', 'install', 'mapVersion.installThisVersion'], [same, 'latest', 'reinstall', 'common.action.reinstall'], [unknown, 'unknown', 'install', 'mapVersion.chooseThisVersion']]) {
    assert.equal(mapCardState(map, [local], [older, same, unknown]), state)
    assert.equal(mapCardAction(map, [local]), action)
    assert.equal(mapCardActionLabel(map, [local]), label)
  }
})

test('地图安装的所有动态动作在五语下实际翻译，不显示裸 key 或 MOD 文案', () => {
  const local = installed()
  const cases = [
    [online('new'), [], '安装'],
    [online('same', '1'), [local], '重装'],
    [online('newer', '2'), [local], '更新'],
    [online('unread', '2', { latest_tag: '' }), [], '刷新'],
    [online('failed', '2', { last_error: 'mapSource.noRelease' }), [local], '刷新'],
    [online('older', '0.9'), [local], '安装此版本'],
    [online('unknown', ''), [local], '主动安装此版本'],
  ]
  for (const [map, sets, chinese] of cases) {
    const key = mapCardActionLabel(map, sets)
    for (const locale of LOCALE_CODES) {
      assert.ok(i18n.global.te(key, locale), `${locale} 缺少 ${key}`)
      const text = i18n.global.t(key, {}, { locale })
      assert.notEqual(text, key)
      assert.doesNotMatch(text, /MOD|ModRepository|模组|模組/)
      if (locale === 'zh-CN') assert.equal(text, chinese)
    }
  }
})

test('page actions consume captured mapId/tag and all-source highlights retain local anchors', () => {
  const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
  const view = read('src/views/LevelsView.vue'), engine = read('src/composables/useMapSources.ts')
  assert.match(view, /maps\.value\.find\(map => map\.id === action\.mapId\)/)
  assert.match(view, /cachedInstallRelease\(map\.cached_releases, action\.tag\)/)
  assert.match(view, /planGate\.run\(mapRelationSubject\(map, targetTag\), targetTag\)/)
  assert.match(view, /candidates\.some\(candidate => candidate\.mapId === requested\)/)
  assert.match(engine, /sourceOrder: sourceNodes\.value\.map\(node => node\.id\)/)
  assert.match(engine, /refresh: \(\) => refreshReleases\(maps\.value\.filter\(map => installed\.some/)
  assert.match(engine, /replaceMap\(item\.id, snapshot\)/)
  assert.doesNotMatch(engine, /finalize:[\s\S]{0,80}loadMaps/)
})
