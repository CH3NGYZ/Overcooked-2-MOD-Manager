/** Stable domain diagnostics are translated here, never displayed as internal codes. */
export const changeReasonKeys: Record<string, string> = {
  version_required_by_plan: 'modChanges.reason.version',
  enable_required_plugin: 'modChanges.reason.enable',
  disable_conflict_or_dependent: 'modChanges.reason.disable',
  member_state_required_by_plan: 'modChanges.reason.member',
  root_missing: 'modChanges.targetChanged', root_member_missing: 'modChanges.targetChanged',
  root_candidate_missing: 'modChanges.reason.candidateMissing',
  root_candidate_not_installable: 'modChanges.reason.candidateUnavailable',
  contradictory_roots: 'modChanges.reason.contradictoryRoots',
  root_ownership_unsafe: 'modChanges.reason.ownership',
  duplicate_active_identity: 'modChanges.reason.ownership',
  candidate_relations_unknown: 'modChanges.reason.metadata',
  relation_has_no_solution: 'modChanges.reason.noSolution',
  search_exhausted: 'modChanges.reason.searchLimit',
}
