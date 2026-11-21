import { setEvaluationSnapshotReader } from '../../src/modChanges/evaluationProjection.ts'

// A backend DTO fixture, deliberately containing no relation/version solver.
export function evaluationSnapshot(entries, extras = []) {
  const snapshot = { revision: 'fixture', nodes: entries.map(entry => ({
    node_id: entry.id, label: entry.label ?? entry.id, kind: entry.kind ?? 'mod',
    enabled: entry.enabled ?? true, dll_files: entry.dlls ?? [], members: entry.members,
    repository_id: entry.repo, map_dir_name: entry.dir, map_id: entry.map,
    subject_tag: entry.tag, candidates: entry.candidates ?? [],
  })).concat(extras), evaluations: entries.map(entry => {
    const rows = (entry.rows ?? []).map(row => ({ owner_id: entry.id, scope: entry.enabled === false ? 'readiness' : 'runtime',
      state: 'satisfied', targets: [], reason: 'fixture', ...row }))
    return { node_id: entry.id, enabled: entry.enabled ?? true, metadata_known: true,
      runtime: entry.enabled === false ? [] : rows, readiness: entry.enabled === false ? rows : [] }
  }) }
  setEvaluationSnapshotReader(() => snapshot)
  return snapshot
}

export function clearEvaluation() { setEvaluationSnapshotReader(() => null) }
