import assert from 'node:assert/strict'
import test from 'node:test'
import { changeErrorEvidence, providerEvidenceAdvice, providerRows } from '../src/modChanges/evidence.ts'
import { backendMessageOf } from '../src/locale/backend.ts'
import { ChangePreparationError } from '../src/modChanges/types.ts'

const path = 'BepInEx/plugins/MixedCase 地图 & More/DIYLevel.dll.disabled'
const evidence = { guid: 'dev.gua.overcooked.diylevel', rows: [
  { node_id: 'disk:old', candidate_id: 'installed:old', member_key: 'opaque', path, version: '0.11.1', enabled: false, installed_enabled: false, origin: 'installed' },
  { node_id: 'mod:verified', candidate_id: 'verified:1', member_key: 'opaque', version: '0.11.1', enabled: true, origin: 'planned' },
] }
const tag = `framework.diyLevelAmbiguous?evidence=${encodeURIComponent(JSON.stringify(evidence))}`

test('wire copies survive planning, preparation and execution, preserving exact path and state', () => {
  for (const error of [tag, new ChangePreparationError(tag), { message: tag }]) {
    const detail = changeErrorEvidence(error)
    assert.deepEqual(detail.providers, evidence)
    assert.deepEqual(detail.rows, providerRows(evidence))
    assert.equal(detail.rows[0].path, path)
    assert.equal(detail.rows[0].locate, true)
    assert.equal(detail.rows[1].locate, false)
    assert.doesNotMatch(backendMessageOf(error), /\?evidence|candidate_id|framework\.diyLevelAmbiguous/)
  }
})

test('a nested provider conflict retains the parent error location and copy detail', () => {
  const parent = 'BepInEx/plugins/MixedCase 地图 & More'
  const error = { tag: 'modChanges.verifyInstallTarget', params: { path: parent, reason: tag } }
  assert.deepEqual(changeErrorEvidence(error), { providers: evidence, rows: [...providerRows(evidence), { path: parent }] })
})

test('structured path params are separately copyable and never reveal an unproven location', () => {
  const nested = `modChanges.verifyInstallTarget?path=${encodeURIComponent(path)}&reason=Fixture`
  const error = `modChanges.installRecoveryPending?reason=${encodeURIComponent(nested)}`
  assert.deepEqual(changeErrorEvidence(error).rows, [{ path }])
  assert.deepEqual(changeErrorEvidence({ tag: 'modChanges.installTargetClaimed', params: { path, manifest: 'manifests/Owner.toml' } }).rows,
    [{ path }, { path: 'manifests/Owner.toml' }])
  const active = path.slice(0, -'.disabled'.length)
  const conflict = `modChanges.installTargetHasBothStates?paths=${encodeURIComponent(`${active}\n${path}`)}`
  assert.deepEqual(changeErrorEvidence(conflict).rows, [{ path: active }, { path }])
  assert.deepEqual(changeErrorEvidence('File system is read only: C:\\Path').rows, [])
  assert.deepEqual(changeErrorEvidence('framework.diyLevelAmbiguous?evidence=%7B').rows, [])
})

test('a known DLL in the disabled map cache is copyable but not locatable', () => {
  const member = { ...evidence.rows[0], path: 'BepInEx/cache/modmanager_disabled_maps/Young/YoungLevels.dll.disabled' }
  assert.deepEqual(providerRows({ rows: [member] }), [{ path: member.path, locate: false, provider: member }])
})

test('path-only failures receive no cleanup advice and planned copies cannot trigger disk cleanup advice', () => {
  const location = changeErrorEvidence({ tag: 'modChanges.verifyInstallTarget', params: { path } })
  assert.equal(providerEvidenceAdvice(location.providers), undefined)
  assert.equal(providerEvidenceAdvice(evidence), 'modChanges.evidence.checkFramework')
  assert.equal(providerEvidenceAdvice({ ...evidence, guid: 'example.mod' }), 'modChanges.evidence.checkCopies')
  assert.equal(providerEvidenceAdvice({ ...evidence, rows: [evidence.rows[1]] }), 'modChanges.evidence.checkPlanned')
})
