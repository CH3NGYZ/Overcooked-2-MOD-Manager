import type { DownloadProgress, MapInstallOrigin, RepositoryRelation } from '../types'

/** Rust mod_changes/model.rs is the authority for relation and action semantics. */
export interface RootIntent { node_id: string; candidate_id?: string; enabled: boolean; member_key?: string; asset_name?: string }
export interface ChangeRequest { roots: RootIntent[]; map_import?: { zip_path: string; selected: string[]; origin?: MapInstallOrigin | null } }
export type ChangeActionKind = 'install' | 'upgrade' | 'downgrade' | 'replace' | 'enable' | 'disable'
export interface ChangeAction {
  node_id: string
  member_key?: string | null
  kind: ChangeActionKind
  from_candidate_id?: string | null
  to_candidate_id?: string | null
  from_tag?: string | null
  to_tag?: string | null
  reason: string
  caused_by: string[]
}
export interface RelationEvaluation {
  owner_id: string
  relation: RepositoryRelation
  scope: 'runtime' | 'readiness'
  state: 'satisfied' | 'missing' | 'disabled' | 'version_mismatch' | 'conflict' | 'unknown' | 'optional'
  targets: string[]
  reason: string
}
export interface NodeEvaluation {
  node_id: string
  enabled: boolean
  metadata_known: boolean
  runtime: RelationEvaluation[]
  readiness: RelationEvaluation[]
}
export interface ChangeDiagnostic {
  code: string
  owner_id?: string | null
  target_guid?: string | null
  message: string
  incomplete: boolean
}
export interface ChangePlan {
  plan_id: string
  status: 'ready' | 'unsatisfiable' | 'incomplete'
  actions: ChangeAction[]
  evaluations: NodeEvaluation[]
  blockers: ChangeDiagnostic[]
  impacts: string[]
  roots: { node_id: string; label: string; tag?: string | null }[]
  nodes: ChangeNode[]
  prepared: boolean
}
export interface EvaluationSnapshot {
  revision: string
  nodes: ChangeNode[]
  evaluations: NodeEvaluation[]
}
export interface ChangeNode {
  node_id: string
  label: string
  kind: string
  enabled: boolean
  repository_id?: string | null
  dll_files: string[]
  map_dir_name?: string | null
  map_id?: string | null
  subject_tag?: string | null
  package_id?: string | null
  members?: { key: string; dll_file: string; enabled: boolean; guid?: string | null }[]
  candidates: { id: string; tag?: string | null }[]
}
export interface EvaluationSubject { repository_id?: string; tag?: string; map_id?: string; dll_file?: string; map_dir_name?: string }
export interface ChangeExecutionResult { ok: boolean; message: string }
export interface ChangePreparationItem {
  node_id: string
  candidate_id: string
  label: string
  tag?: string | null
  stage: 'queued' | 'downloading' | 'verifying' | 'ready' | 'failed'
  download?: DownloadProgress | null
}
/** Full task-local snapshot, delivered only by the active preparation IPC. */
export interface ChangePreparationProgress {
  plan_id: string
  stage: 'resolving' | 'downloading' | 'verifying' | 'finalizing' | 'ready' | 'failed'
  active_node_id?: string | null
  actions: ChangeAction[]
  items: ChangePreparationItem[]
  error?: string | null
}
export interface ChangePreparationOutcome {
  plan: ChangePlan | null
  error: string | null
  progress: ChangePreparationProgress
}
export interface ChangeBackend {
  plan(request: ChangeRequest): Promise<ChangePlan>
  prepare(planId: string): Promise<ChangePlan>
  execute(planId: string): Promise<ChangeExecutionResult>
  cancel(planId: string): Promise<void>
}
