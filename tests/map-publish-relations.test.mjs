import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { buildOverrides, overrideDrafts, relationsToRecord, relationIssue, emptyRelationTables } from '../src/mapPublishRelations.ts'

const original = [{ version: '>=0.9', relations: [
  { kind: 'requires', guid: 'dev.gua.overcooked.diylevel', version: '>=0.10.0', optional: true },
  { kind: 'compatible', guid: 'compat', version: '*', prerelease: false },
  { kind: 'conflicts', guid: 'bad', version: '>=2.0', prerelease: true },
] }]

test('版本关系草稿隔离默认表、各分类与不同覆盖项，保存保留额外标志', () => {
  const drafts = overrideDrafts(original)
  assert.equal(relationIssue(emptyRelationTables(), drafts), null)
  drafts[0].requires[0].range = '>= 0.11.0'
  const saved = buildOverrides(drafts)
  assert.equal(saved[0].relations[0].version, '>=0.11.0')
  assert.equal(saved[0].relations[0].optional, true)
  assert.equal(saved[0].relations[1].prerelease, false)
  assert.equal(saved[0].relations[2].prerelease, true)
  assert.equal(original[0].relations[0].version, '>=0.10.0')
  assert.equal(overrideDrafts(saved)[0].requires[0].range, '>=0.11.0')
})

test('空版本条件表示不限，写成 * 而不是悄悄丢掉关系', () => {
  assert.deepEqual(relationsToRecord([{ guid: ' dep ', range: ' ' }]), { dep: '*' })
  assert.deepEqual(overrideDrafts(), [])
})

test('关系编辑拒绝空 GUID、重复、坏区间及无范围或无内容的覆盖，定位到真实字段', () => {
  const tables = emptyRelationTables()
  tables.requires.push({ guid: '', range: '' })
  assert.equal(relationIssue(tables, []), 'relation-default-requires-0-guid')
  tables.requires[0].guid = 'test'
  tables.requires.push({ guid: 'TEST', range: '>=1' })
  assert.equal(relationIssue(tables, []), 'relation-default-requires-1-guid')
  tables.requires.pop()
  tables.requires[0].range = '~=1'
  assert.equal(relationIssue(tables, []), 'relation-default-requires-0-range')
  for (const range of ['', '*', '~=0.9']) {
    const overrides = overrideDrafts(original)
    overrides[0].version = range
    assert.equal(relationIssue(emptyRelationTables(), overrides), 'relation-override-0-version')
  }
  assert.equal(relationIssue(emptyRelationTables(), [{ version: '==0.9', ...emptyRelationTables() }]), 'relation-override-0-version')
})

test('IPC、草稿重置与保存都携带 release_relations，进度用每次请求的回调开关', () => {
  const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
  const draft = read('src/composables/useMapPublishDraft.ts')
  assert.match(draft, /overrideDrafts\(metadata\?\.release_relations\)/)
  assert.match(draft, /release_relations: buildOverrides\(draft.releaseRelations\)/)
  const upload = read('src/composables/useMapPublish.ts').split('async function uploadAsset(')[1].split('async function inspectZip(')[0]
  assert.match(upload, /let acceptingProgress = true/)
  assert.match(upload, /!disposed && acceptingProgress/)
  assert.match(upload, /phase === 'done' \? null : progress/)
  assert.match(upload, /finally \{\s*acceptingProgress = false\s*uploadProgress.value = null\s*uploading.value = false/)
  const css = read('src/styles/publish.css')
  assert.match(css, /grid-template-columns: auto minmax\(0, 1fr\) auto/)
  assert.match(css, /publish-upload-progress > span \{ white-space: nowrap;/)
})
