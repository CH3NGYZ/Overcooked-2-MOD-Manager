import assert from 'node:assert/strict'
import test from 'node:test'
import { assetRelationOverride, buildOverrides, emptyRelationTables, overrideDrafts } from '../src/mapPublishRelations.ts'
import { metadataForStep } from '../src/mapPublishRelease.ts'
import { pickReleaseRelations } from '../src/modRelations.ts'

const relation = guid => ({ kind: 'requires', guid, version: '>=1', optional: true, prerelease: false })
test('单资产编辑复制生效关系而不收窄宽区间；同下限的等值覆盖优先且只影响该版', () => {
  const defaults = { ...emptyRelationTables(), requires: [{ guid: 'default', range: '*' }] }
  const drafts = overrideDrafts([{ version: '>=1', relations: [relation('range')] }])
  assetRelationOverride(defaults, drafts, '1')
  assert.equal(drafts[0].version, '==1')
  assert.equal(drafts[1].version, '>=1')
  assert.equal(drafts[0].requires[0].guid, 'range')
  drafts[0].requires[0].guid = 'specific'
  const saved = buildOverrides(drafts)
  assert.equal(pickReleaseRelations([], saved, '1')[0].guid, 'specific')
  assert.equal(pickReleaseRelations([], saved, '2')[0].guid, 'range')
  assert.equal(saved[0].relations[0].optional, true)
  assert.equal(saved[0].relations[0].prerelease, false)
  assert.equal(defaults.requires[0].guid, 'default')
  assetRelationOverride(defaults, drafts, '1.0')
  assert.equal(drafts.length, 2, '语义相同的等值覆盖复用，不生成重复项')
  assert.equal(drafts[0].requires[0].guid, 'specific')
})

test('没有覆盖时从默认表深复制，兄弟资产独立编辑', () => {
  const defaults = { ...emptyRelationTables(), compatible: [{ guid: 'base', range: '' }] }
  const drafts = []
  assetRelationOverride(defaults, drafts, '1')
  assetRelationOverride(defaults, drafts, '2')
  drafts[0].compatible[0].guid = 'changed'
  assert.equal(drafts[1].compatible[0].guid, 'base')
  assert.equal(defaults.compatible[0].guid, 'base')
})

test('地图资料与版本保存互不夹带草稿，初次提交包含完整块', () => {
  const saved = { asset: 'explicit', name_i18n: { en: 'saved' }, requires: { base: '*' },
    latest_mapset_version: '1', latest_info_version: '0.9', asset_versions: [{ mapset_version: '1', info_version: '0.9' }],
    release_relations: [{ version: '>=1', relations: [relation('saved')] }] }
  const draft = { ...saved, asset: 'ignored', name_i18n: { en: 'draft' }, requires: { changed: '*' },
    latest_mapset_version: '2', latest_info_version: '2', asset_versions: [{ mapset_version: '2', info_version: '2' }],
    release_relations: [{ version: '==2', relations: [relation('draft')] }] }
  const info = metadataForStep('metadata', saved, draft)
  assert.equal(info.name_i18n.en, 'draft')
  assert.deepEqual(info.requires, draft.requires)
  assert.deepEqual(info.asset_versions, saved.asset_versions)
  assert.deepEqual(info.release_relations, saved.release_relations)
  const versions = metadataForStep('versions', saved, draft)
  assert.equal(versions.name_i18n.en, 'saved')
  assert.deepEqual(versions.requires, saved.requires)
  assert.deepEqual(versions.asset_versions, draft.asset_versions)
  assert.deepEqual(versions.release_relations, draft.release_relations)
  assert.equal(versions.asset, 'explicit')
  assert.deepEqual(metadataForStep('versions', null, draft), draft)
})
