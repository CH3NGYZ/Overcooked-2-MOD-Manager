import assert from 'node:assert/strict'
import test from 'node:test'
import { assetRelationOverride, withAssetRelations, buildOverrides, emptyRelationTables, overrideDrafts } from '../src/mapPublishRelations.ts'
import { metadataForStep } from '../src/mapPublishRelease.ts'
import { pickReleaseRelations } from '../src/modRelations.ts'

const relation = guid => ({ kind: 'requires', guid, version: '>=1', optional: true, prerelease: false })
test('弹窗读取当前生效关系的副本，打开、编辑或取消都不修改原声明', () => {
  const defaults = { ...emptyRelationTables(), requires: [{ guid: 'default', range: '*' }] }
  const drafts = overrideDrafts([{ version: '>=1', relations: [relation('range')] }])
  const snapshot = structuredClone(drafts)
  const entry = assetRelationOverride(defaults, drafts, '1')
  assert.equal(entry.version, '==1')
  assert.equal(entry.requires[0].guid, 'range')
  entry.requires[0].guid = 'specific'
  assert.deepEqual(drafts, snapshot)
  assert.equal(defaults.requires[0].guid, 'default')
  const savedDrafts = withAssetRelations(drafts, '1', entry)
  assert.equal(savedDrafts[1].version, '>=1')
  const saved = buildOverrides(savedDrafts)
  assert.equal(pickReleaseRelations([], saved, '1')[0].guid, 'specific')
  assert.equal(pickReleaseRelations([], saved, '2')[0].guid, 'range')
  assert.equal(saved[0].relations[0].optional, true)
  assert.equal(saved[0].relations[0].prerelease, false)
  entry.requires[0].guid = 'changed-after-confirm'
  assert.equal(savedDrafts[0].requires[0].guid, 'specific')
  const reopened = assetRelationOverride(defaults, savedDrafts, '1.0')
  const replaced = withAssetRelations(savedDrafts, '1.0', reopened)
  assert.equal(replaced.length, 2, '语义相同的等值覆盖替换，不生成重复项')
  assert.equal(replaced[0].requires[0].guid, 'specific')
})

test('没有覆盖时复制默认表，各资产独立编辑，恢复继承只移除该版等值项', () => {
  const defaults = { ...emptyRelationTables(), compatible: [{ guid: 'base', range: '' }] }
  const first = assetRelationOverride(defaults, [], '1')
  const second = assetRelationOverride(defaults, [], '2')
  first.compatible[0].guid = 'changed'
  assert.equal(second.compatible[0].guid, 'base')
  assert.equal(defaults.compatible[0].guid, 'base')
  const range = overrideDrafts([{ version: '>=1', relations: [relation('wide')] }])
  const drafts = withAssetRelations(withAssetRelations(range, '1', first), '2', second)
  const inherited = withAssetRelations(drafts, '1.0', null)
  assert.deepEqual(inherited.map(entry => entry.version), ['==2', '>=1'])
  assert.equal(pickReleaseRelations([], buildOverrides(inherited), '1')[0].guid, 'wide')
  assert.equal(drafts.length, 3)
  assert.deepEqual(withAssetRelations([], '1', null), [])
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
