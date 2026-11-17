import assert from 'node:assert/strict'
import test from 'node:test'
import {
  dependencyRowFromRelation, installedModDependencyRows,
  mapDependencyRows, levelSetDependencyRows,
} from '../src/dependencyRows.ts'

const release = (tag, extra = {}) => ({
  tag_name: tag, version: tag, name: tag, body: '', assets: [], draft: false, prerelease: false, ...extra,
})
const source = (guid, releases, extra = {}) => ({
  id: guid, name: guid, plugin_guid: guid, download_mode: 'release',
  cached_releases: releases, ...extra,
})
const installed = (guid, version, extra = {}) => ({
  guid, version, name: `${guid}.dll`, dll_file: `${guid}.dll`, enabled: true, ...extra,
})
const options = (guid, current, releases, extra = {}) => ({
  mods: [installed(guid, current)], repositories: [source(guid, releases)],
  jumpToRepository: true, ...extra,
})

test('DIYLevel 0.11 exceeds an exclusive upper bound: downgrade to the matching cached release', () => {
  const row = dependencyRowFromRelation(
    { guid: 'dev.diy', version: '>=0.10.0,<0.11' },
    options('dev.diy', '0.11', [release('v0.11.0'), release('v0.10.6'), release('v0.10.0')]),
  )
  assert.equal(row.status, 'tools.downgradable')
  assert.equal(row.tone, 'moderate')
  assert.equal(row.jump.id, 'dev.diy')
  assert.deepEqual(row.jump.downgrade, { installedVersion: '0.11', targetVersion: 'v0.10.6' })
})

test('Web Runtime 4.0 requires a downgrade to 3.10.2, not an update to its newest release', () => {
  const row = dependencyRowFromRelation(
    { guid: 'dev.web', version: '>=3.10.2,<4' },
    options('dev.web', '4.0', [release('v4.0.0'), release('v3.10.2')]),
  )
  assert.equal(row.status, 'tools.downgradable')
  assert.deepEqual(row.jump.downgrade, { installedVersion: '4.0', targetVersion: 'v3.10.2' })
})

test('downgrade uses the full tag and established prerelease matching, not projected version metadata', () => {
  const row = dependencyRowFromRelation(
    { guid: 'dev.diy', version: '>=0.10.0,<0.11', prerelease: true },
    options('dev.diy', '0.11.0', [
      release('v0.10.9'),
      release('v0.10.8-beta.12', { version: '0.10.8', prerelease: true }),
    ]),
  )
  assert.equal(row.status, 'tools.downgradable')
  assert.equal(row.jump.downgrade.targetVersion, 'v0.10.8-beta.12')
})

test('downgrade direction follows numeric beta ordering; an API prerelease flag alone is not a lower version', () => {
  const beta = dependencyRowFromRelation(
    { guid: 'dev.fw', version: '<=2.0.0-beta.2' },
    options('dev.fw', '2.0.0-beta.10', [release('v2.0.0-beta.2')]),
  )
  assert.equal(beta.status, 'tools.downgradable')
  assert.deepEqual(beta.jump.downgrade, { installedVersion: '2.0.0-beta.10', targetVersion: 'v2.0.0-beta.2' })

  const sameNumber = dependencyRowFromRelation(
    { guid: 'dev.fw', version: '>=2.0.0', prerelease: true },
    options('dev.fw', '2.0.0', [release('v2.0.0', { apiPrerelease: true })], {
      mods: [installed('dev.fw', '2.0.0', { prerelease: false })],
    }),
  )
  assert.equal(sameNumber.status, 'tools.updatable')
  assert.equal(sameNumber.jump.downgrade, undefined)
})

test('a candidate without a tag keeps the actual release version as its downgrade target', () => {
  const row = dependencyRowFromRelation(
    { guid: 'dev.web', version: '>=3.10.2,<4' },
    options('dev.web', '4.0', [release('', { version: '3.10.2' })]),
  )
  assert.equal(row.status, 'tools.downgradable')
  assert.equal(row.jump.downgrade.targetVersion, '3.10.2')
})

test('a disabled installed mod still needs a downgrade; the row does not pretend it is absent', () => {
  const row = dependencyRowFromRelation(
    { guid: 'dev.web', version: '<4' },
    options('dev.web', '4.0', [release('v3.10.2')], { mods: [installed('dev.web', '4.0', { enabled: false })] }),
  )
  assert.equal(row.status, 'tools.downgradable')
  assert.equal(row.jump.downgrade.installedVersion, '4.0')
})

test('upgrade, absent, satisfied, unknown-version and missing-source rows do not carry downgrade confirmations', () => {
  const relation = { guid: 'dev.fw', version: '>=2.0,<3' }
  for (const [current, releases, extra, expected] of [
    ['1.0', [release('v2.5')], {}, 'tools.updatable'],
    [null, [release('v2.5')], { mods: [] }, 'tools.installable'],
    ['2.5', [release('v2.5')], {}, 'tools.satisfied'],
    ['unknown', [release('v2.5')], {}, 'tools.updatable'],
    ['3.0', [release('v4.0')], {}, 'modsOnline.missingDep'],
    ['3.0', [release('v2.5', { draft: true })], {}, 'modsOnline.missingDep'],
  ]) {
    const row = dependencyRowFromRelation(relation, options('dev.fw', current, releases, extra))
    assert.equal(row.status, expected, `${current}: status`)
    assert.equal(row.jump?.downgrade, undefined, `${current}: no downgrade prompt`)
  }
})

test('displaying a downgrade does not enable jumping inside non-navigable installation previews', () => {
  const row = dependencyRowFromRelation(
    { guid: 'dev.web', version: '<4' },
    options('dev.web', '4.0', [release('v3.10.2')], { jumpToRepository: false }),
  )
  assert.equal(row.status, 'tools.downgradable')
  assert.equal(row.jump, undefined)
})

test('map download, map management and installed MOD dependencies share downgrade target metadata', () => {
  const relation = { guid: 'dev.diy', version: '>=0.10.0,<0.11' }
  const onlineMap = { name: 'Map A', level_set_name: 'Map A', author: 'Author', relations: [relation] }
  const localMap = { name: 'Map A', author: 'Author', version: '1.0', relations: [] }
  const localOwner = installed('dev.owner', '1.0', { relations: [relation] })
  const opts = options('dev.diy', '0.11', [release('v0.11'), release('v0.10.6')])
  const expected = dependencyRowFromRelation(relation, opts)
  assert.deepEqual(mapDependencyRows(onlineMap, opts), [expected])
  assert.deepEqual(levelSetDependencyRows(localMap, { ...opts, maps: [onlineMap] }), [expected])
  assert.deepEqual(installedModDependencyRows(localOwner, opts), [expected])
})
