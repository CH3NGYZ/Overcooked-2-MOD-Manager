import type { DownloadProgress, MapInstallOrigin, RepositoryRelation } from '../types'

/** Rust mod_changes/model.rs is the authority for relation and action semantics. */
export interface RootIntent { node_id: string; candidate_id?: string; enabled: boolean; member_key?: string; asset_name?: string }
export type DependencyRepairTarget = Pick<RootIntent, 'node_id' | 'candidate_id'>
export type ChangeRequest = {
  roots: RootIntent[]
  repair_dependencies_of?: never
  map_import?: { zip_path: string; selected: string[]; origin?: MapInstallOrigin | null }
  /** Local scan seen at click and trusted inventory used to resolve its root. */
  toggle_scope?: { generation: number; revision: string }
  bulk_toggle?: boolean
} | {
  repair_dependencies_of: string
  roots?: never
  map_import?: never
  toggle_scope?: never
  bulk_toggle?: never
}
export type ChangeActionKind = 'install' | 'upgrade' | 'downgrade' | 'replace' | 'enable' | 'disable'
/** These actions stage installation payloads, even when their files are already local. */
export const isPackageAction = ({ kind }: Pick<ChangeAction, 'kind'>) => kind !== 'enable' && kind !== 'disable'
export interface ChangeActionReason {
  kind: 'requested' | 'required_by' | 'compatible_upgrade' | 'dependency_version' | 'dependency_disabled' | 'dependency_missing'
    | 'dependency_unconfirmed' | 'compatibility_unconfirmed' | 'conflicts_with' | 'incompatible_with'
  related_node_ids: string[]
  target_guid?: string | null
  requirement?: string | null
  prerelease?: boolean | null
  condition_on_related?: boolean
}
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
  reason_details?: ChangeActionReason[]
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
export interface DiagnosticStep {
  owner_id: string
  owner_version?: string | null
  relation?: RepositoryRelation | null
  targets: { node_id: string; version?: string | null; enabled: boolean }[]
  state: RelationEvaluation['state']
  reason: string
}
export interface ChangeDiagnostic {
  code: string
  owner_id?: string | null
  target_guid?: string | null
  message: string
  incomplete: boolean
  /** Each path follows one tried branch and ends at the relation that blocked it. */
  traces?: DiagnosticStep[][]
  traces_truncated?: boolean
}
/** Source text selected against the same trusted inventory as the plan. */
export interface ChangeInstallNote {
  id: string
  name: string
  name_i18n: Record<string, string>
  note: string
  note_i18n: Record<string, string>
}
export interface RetiredModCopy { member_key: string; label: string; version: string; dll_file: string }
export interface ChangeLabel { label: string; name_i18n?: Record<string, string> }
export interface ChangePlan {
  plan_id: string
  status: 'ready' | 'unsatisfiable' | 'incomplete'
  actions: ChangeAction[]
  evaluations: NodeEvaluation[]
  blockers: ChangeDiagnostic[]
  impacts: string[]
  roots: { node_id: string; label: string; tag?: string | null }[]
  nodes: ChangeNode[]
  relation_labels?: Record<string, ChangeLabel>
  /** Effectively enabled members before a whole-node disable. */
  previous_enabled_members?: Record<string, string[]>
  /** Independently installed older copies removed as part of an approved upgrade. */
  retired_mod_copies?: Record<string, RetiredModCopy[]>
  install_notes: ChangeInstallNote[]
  prepared: boolean
}
export interface EvaluationSnapshot {
  revision: string
  nodes: ChangeNode[]
  evaluations: NodeEvaluation[]
}
export type ChangePresentationKind = 'mod_package' | 'plugin' | 'dependency'
export interface ChangeNode extends ChangeLabel {
  node_id: string
  kind: string
  presentation_kind?: ChangePresentationKind
  map_name_zh?: string | null
  enabled: boolean
  repository_id?: string | null
  dll_files: string[]
  map_dir_name?: string | null
  map_id?: string | null
  subject_tag?: string | null
  package_id?: string | null
  members?: { key: string; dll_file: string; enabled: boolean; guid?: string | null; role?: import('../types').ModPreviewRole }[]
  candidates: { id: string; tag?: string | null }[]
}
export interface EvaluationSubject { repository_id?: string; tag?: string; map_id?: string; dll_file?: string; map_dir_name?: string }
export interface ChangeExecutionResult { ok: boolean; message: string }
export interface ChangeExecutionOutcome extends ChangeExecutionResult { progress: ChangeExecutionProgress }
export type ChangeExecutionStage = 'applying' | 'processed' | 'applied' | 'failed' | 'rolled_back' | 'not_applied'
/** Changes since the previous update of this execute call, keyed by the trusted action's target. */
export interface ChangeExecutionProgress {
  plan_id: string
  items: { node_id: string; member_key?: string | null; stage: ChangeExecutionStage }[]
}
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
  /** Let a visible preparation surface show its terminal state before replacing it. */
  presentPrepared?(): Promise<void> | void
  execute(planId: string): Promise<ChangeExecutionResult>
  cancel(planId: string): Promise<void>
}
