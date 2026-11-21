import type { ChangeNode, DependencyRepairTarget, EvaluationSnapshot, EvaluationSubject, RelationEvaluation } from './types'
import type { InstalledMod, RepositoryRelation } from '../types'

// Pure consumers keep their Node entry point. The app supplies its reactive snapshot.
let readSnapshot: () => EvaluationSnapshot | null = () => null
export function setEvaluationSnapshotReader(reader: () => EvaluationSnapshot | null) { readSnapshot = reader }
export function evaluationSnapshot() { return readSnapshot() }

export function repairTarget(ownerId: string): DependencyRepairTarget | undefined {
  const node = readSnapshot()?.nodes.find(entry => entry.node_id === ownerId)
  if (!node) return undefined
  if (!node.subject_tag) {
    if (node.kind === 'mod' && !node.enabled) return undefined
    return node.dll_files.length || node.map_dir_name ? { node_id: node.node_id } : undefined
  }
  if (node.kind === 'map') return undefined
  if (!node.repository_id) return undefined
  const candidate = node.candidates.find(entry => entry.tag === node.subject_tag)
  return candidate ? { node_id: `mod:${node.repository_id}`, candidate_id: candidate.id } : undefined
}

export function sameRelation(left: RepositoryRelation, right: RepositoryRelation) {
  return left.guid.trim().toLowerCase() === right.guid.trim().toLowerCase()
    && (left.kind ?? 'requires') === (right.kind ?? 'requires')
    && (left.version ?? '') === (right.version ?? '')
    && left.prerelease === right.prerelease && !!left.optional === !!right.optional
}

function modRuntimeMatchesScan(node: ChangeNode, dll: string, mods: readonly InstalledMod[]) {
  // Installed views pass the full scan, but a caller with no local owner data
  // can still use the backend snapshot.
  if (!mods.some(mod => mod.dll_file === dll)) return true
  if (node.members?.length) {
    return node.members.every(member => mods.find(mod => mod.dll_file === member.dll_file)?.enabled === member.enabled)
  }
  return mods.some(mod => mod.enabled && node.dll_files.includes(mod.dll_file))
}

/** Missing or ambiguous subjects stay unknown; the client never evaluates ranges. */
export function relationsForSubject(subject: EvaluationSubject, mods: readonly InstalledMod[]) {
  const snapshot = readSnapshot()
  if (!snapshot) return undefined
  const matching = snapshot.nodes.filter(node => {
    if (subject.dll_file) return !node.subject_tag && node.dll_files.includes(subject.dll_file)
    if (subject.map_dir_name) return !node.subject_tag && node.map_dir_name === subject.map_dir_name
    if (subject.map_id) return !!subject.tag && node.map_id === subject.map_id && node.subject_tag === subject.tag
    return !!subject.repository_id && !!subject.tag && node.repository_id === subject.repository_id && node.subject_tag === subject.tag
  })
  if (matching.length !== 1) return undefined
  const node = matching[0]!
  const result = snapshot.evaluations.find(entry => entry.node_id === node.node_id)
  if (!result) return undefined
  // Until a fresh evaluation arrives, changes to any package member can
  // invalidate runtime verdicts, including which DLL declared a conflict.
  const declarationsOnly = !!subject.dll_file && node.kind === 'mod' && (!result.enabled
    || !modRuntimeMatchesScan(node, subject.dll_file, mods))
  const rows = result.enabled && !declarationsOnly ? result.runtime : result.readiness
  // No known declarations is different from an authoritative empty relation table.
  return rows.length || result.metadata_known ? { rows, assessed: !declarationsOnly } : undefined
}

export function targetMod(evaluation: RelationEvaluation, mods: readonly InstalledMod[]) {
  const inventory = readSnapshot()
  const targets = inventory?.nodes.filter(node => evaluation.targets.includes(node.node_id)) ?? []
  const guid = evaluation.relation.guid.trim().toLowerCase()
  const matches = mods.filter(mod => mod.guid?.trim().toLowerCase() === guid
    && targets.some(node => node.dll_files.includes(mod.dll_file)))
  if (evaluation.state !== 'conflict' || evaluation.relation.kind !== 'conflicts') return matches[0]

  // The evaluation lists installed nodes, not the exact conflicting DLL. Never
  // offer a standalone uninstall if the conflict might belong to another node
  // or to a different member of the same package.
  const target = targets[0]
  return targets.length === 1 && evaluation.targets.length === 1 && target?.kind === 'mod'
    && target.node_id !== evaluation.owner_id && matches.length === 1 && matches[0]?.enabled
    ? matches[0] : undefined
}
