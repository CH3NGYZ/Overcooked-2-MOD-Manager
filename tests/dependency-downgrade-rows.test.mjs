import assert from 'node:assert/strict'
import test from 'node:test'
import { dependencyRowFromRelation, dependencyRowsFor, installedModDependencyRows,
  mapDependencyRows, levelSetDependencyRows, mergedModDependencyRows } from '../src/dependencyRows.ts'
import { relationsForSubject } from '../src/modChanges/evaluationProjection.ts'
import { resolveRelation } from '../src/modRelations.ts'
import { evaluationSnapshot, clearEvaluation } from './helpers/mod-change-evaluations.mjs'

const relation = { kind: 'requires', guid: 'dev.runtime', version: '>=3.10.2,<4' }
const owner = { guid: 'dev.owner', name: 'Owner', dll_file: 'Owner.dll', enabled: true, version: '1.0', relations: [relation] }
const target = { guid: 'dev.runtime', name: 'Runtime', dll_file: 'Runtime.dll', enabled: true, version: '4.0' }
const online = { id: 'repo', relations: [relation], latest_tag: 'v1', name: 'Online' }
const map = { id: 'map', relations: [relation], latest_tag: 'v1', name: 'Map', level_set_name: 'Map', author: 'A' }
const localMap = { dir_name: 'Map', name: 'Map', author: 'A', relations: [relation] }
const options = { mods: [owner, target], repositories: [{ id: 'runtime', name: 'Runtime', plugin_guid: relation.guid,
  cached_releases: [{ tag_name: 'v3.10.2' }, { tag_name: 'v4.0' }] }], jumpToRepository: true }

test('absent backend evaluation stays unassessed even when local data appears to satisfy a range', () => {
  clearEvaluation()
  const compatible = { ...options, mods: [{ ...target, version: '3.10.2' }] }
  for (const row of [dependencyRowFromRelation(relation, compatible), installedModDependencyRows(owner, compatible)[0],
    levelSetDependencyRows(localMap, { ...compatible, maps: [map] })[0], mapDependencyRows(map, compatible)[0]]) {
    assert.equal(row.requirement, relation.version)
    assert.equal(row.status, undefined)
    assert.equal(row.tone, undefined)
    assert.equal(row.jump, undefined)
  }
})

for (const [state, text] of [['missing', 'modsOnline.missingDep'], ['disabled', 'modChanges.relation.disabled'],
  ['version_mismatch', 'modChanges.relation.versionMismatch'], ['unknown', 'modChanges.relation.unknown'], ['satisfied', 'tools.satisfied']]) {
  test(`backend ${state} is projected without choosing a release or inventing a downgrade`, () => {
    evaluationSnapshot([{ id: 'mod:owner', dlls: ['Owner.dll'], rows: [{ relation, state, targets: ['mod:runtime'] }] }],
      [{ node_id: 'mod:runtime', dll_files: ['Runtime.dll'] }])
    const row = installedModDependencyRows(owner, options)[0]
    assert.equal(row.status, text)
    assert.equal(row.jump?.targetVersion, undefined)
    assert.equal(row.jump?.versionChange, undefined)
    assert.deepEqual(row.jump?.repair, ['satisfied', 'unknown'].includes(state) ? undefined : { node_id: 'mod:owner', enabled: true })
    clearEvaluation()
  })
}

test('all installed map and MOD rows use the same backend version-mismatch state', () => {
  const rows = [{ relation, state: 'version_mismatch' }]
  evaluationSnapshot([{ id: 'mod:owner', dlls: ['Owner.dll'], rows }, { id: 'map:Map', kind: 'map', dir: 'Map', rows },
    { id: 'subject:map:v1', kind: 'map', map: 'map', tag: 'v1', rows }])
  for (const result of [installedModDependencyRows(owner, options), levelSetDependencyRows(localMap, { ...options, maps: [map] }),
    mapDependencyRows(map, options)]) {
    assert.equal(result[0].status, 'modChanges.relation.versionMismatch')
    assert.equal(result[0].jump?.versionChange, undefined)
  }
  assert.equal(mapDependencyRows(map, options)[0].jump, undefined, 'online map versions are fixed in the first solver release')
  clearEvaluation()
})

test('an exact online tag never borrows the installed owner evaluation', () => {
  evaluationSnapshot([{ id: 'mod:repo', repo: 'repo', rows: [{ relation, state: 'satisfied' }] },
    { id: 'subject:repo:v1', repo: 'repo', tag: 'v1', candidates: [{ id: 'release:repo:v1', tag: 'v1' }],
      rows: [{ relation, state: 'missing' }] }])
  const old = dependencyRowsFor([], { ...options, ownerRepository: online, version: 'v1' })[0]
  assert.equal(old.status, 'modsOnline.missingDep')
  assert.deepEqual(old.jump.repair, { node_id: 'mod:repo', candidate_id: 'release:repo:v1', enabled: true })
  const absent = dependencyRowsFor([relation], { ...options, ownerRepository: online, version: 'v0' })[0]
  assert.equal(absent.status, undefined)
  assert.equal(absent.jump, undefined)
  clearEvaluation()
})

test('known empty backend rows replace stale source declarations and optional rows stay hidden', () => {
  evaluationSnapshot([{ id: 'mod:repo', repo: 'repo', rows: [] }])
  assert.deepEqual(dependencyRowsFor([relation], { ownerRepository: online }), [])
  evaluationSnapshot([{ id: 'mod:repo', repo: 'repo', rows: [{ relation: { ...relation, optional: true }, state: 'optional' }] }])
  assert.deepEqual(dependencyRowsFor([relation], { ownerRepository: online }), [])
  clearEvaluation()
})

test('disabled owners display backend readiness while enabled owners display runtime', () => {
  const snapshot = evaluationSnapshot([{ id: 'mod:owner', dlls: ['Owner.dll'], enabled: false,
    rows: [{ relation, state: 'disabled' }] }])
  snapshot.evaluations[0].runtime = [{ relation, state: 'satisfied' }]
  assert.equal(installedModDependencyRows(owner, options)[0].status, 'modChanges.relation.disabled')
  clearEvaluation()
})

test('duplicate physical owner matches are unknown and conflict targets follow physical inventory ownership', () => {
  const conflict = { ...relation, kind: 'conflicts' }
  evaluationSnapshot([{ id: 'mod:owner', repo: 'owner', dlls: ['Owner.dll'], rows: [{ relation: conflict, state: 'conflict', targets: ['mod:real'] }] }],
    [{ node_id: 'mod:real', dll_files: ['Real.dll'] }])
  const duplicate = { ...target, dll_file: 'Other.dll' }, real = { ...target, dll_file: 'Real.dll' }
  assert.equal(resolveRelation(conflict, [duplicate, real], [], { id: 'owner' }).installedMod, real)
  const snapshot = evaluationSnapshot([{ id: 'mod:a', dlls: ['Owner.dll'] }, { id: 'mod:b', dlls: ['Owner.dll'] }])
  assert.equal(relationsForSubject({ dll_file: 'Owner.dll' }), undefined)
  assert.equal(snapshot.nodes.length, 2)
  clearEvaluation()
})

test('package rows retain distinct member requirements without applying a frontend strictest-range rule', () => {
  const member2 = { ...owner, dll_file: 'Other.dll', relations: [{ ...relation, version: '<3' }] }
  evaluationSnapshot([{ id: 'mod:owner', dlls: ['Owner.dll'], rows: [{ relation, state: 'version_mismatch' }] },
    { id: 'mod:other', dlls: ['Other.dll'], rows: [{ relation: member2.relations[0], state: 'missing' }] }])
  const rows = mergedModDependencyRows([owner, member2], options)
  assert.equal(rows.length, 2)
  assert.notEqual(rows[0].key, rows[1].key)
  clearEvaluation()
})
