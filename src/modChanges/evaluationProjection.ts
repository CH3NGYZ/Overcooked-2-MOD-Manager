import type { EvaluationSnapshot, EvaluationSubject, RelationEvaluation, RootIntent } from './types'
import type { InstalledMod, RepositoryRelation } from '../types'

// Pure consumers keep their Node entry point. The app supplies its reactive snapshot.
let readSnapshot: () => EvaluationSnapshot | null = () => null
export function setEvaluationSnapshotReader(reader: () => EvaluationSnapshot | null) { readSnapshot = reader }
export function evaluationSnapshot() { return readSnapshot() }

export function repairRoot(ownerId: string): RootIntent | undefined {
  const node = readSnapshot()?.nodes.find(entry => entry.node_id === ownerId)
  if (!node) return undefined
  if (!node.subject_tag) return { node_id: node.node_id, enabled: true }
  if (node.kind === 'map') return undefined
  if (!node.repository_id) return undefined
  const candidate = node.candidates.find(entry => entry.tag === node.subject_tag)
  return candidate ? { node_id: `mod:${node.repository_id}`, candidate_id: candidate.id, enabled: true } : undefined
}

export function sameRelation(left: RepositoryRelation, right: RepositoryRelation) {
  return left.guid.trim().toLowerCase() === right.guid.trim().toLowerCase()
    && (left.kind ?? 'requires') === (right.kind ?? 'requires')
    && (left.version ?? '') === (right.version ?? '')
    && left.prerelease === right.prerelease && !!left.optional === !!right.optional
}

/** Missing or ambiguous subjects stay unknown; the client never evaluates ranges. */
export function relationsForSubject(subject: EvaluationSubject): RelationEvaluation[] | undefined {
  const snapshot = readSnapshot()
  if (!snapshot) return undefined
  const matching = snapshot.nodes.filter(node => {
    if (subject.dll_file) return !node.subject_tag && node.dll_files.includes(subject.dll_file)
    if (subject.map_dir_name) return node.map_dir_name === subject.map_dir_name
    if (subject.map_id) return node.map_id === subject.map_id && (!subject.tag || node.subject_tag === subject.tag)
    return node.repository_id === subject.repository_id && (subject.tag ? node.subject_tag === subject.tag : !node.subject_tag)
  })
  if (matching.length !== 1) return undefined
  const node = matching[0]!
  const result = snapshot.evaluations.find(entry => entry.node_id === node.node_id)
  if (!result) return undefined
  return result.enabled ? result.runtime : result.readiness
}

export function relationForSubject(relation: RepositoryRelation, subject?: EvaluationSubject) {
  return subject ? relationsForSubject(subject)?.find(entry => sameRelation(entry.relation, relation)) : undefined
}

export function targetMod(evaluation: RelationEvaluation, mods: readonly InstalledMod[]) {
  const inventory = readSnapshot()
  const targets = inventory?.nodes.filter(node => evaluation.targets.includes(node.node_id)) ?? []
  return mods.find(mod => mod.guid?.trim().toLowerCase() === evaluation.relation.guid.trim().toLowerCase()
    && targets.some(node => node.dll_files.includes(mod.dll_file)))
}
