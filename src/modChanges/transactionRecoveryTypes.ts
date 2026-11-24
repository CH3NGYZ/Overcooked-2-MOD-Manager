export type TransactionRecoveryState = 'automatic' | 'paused' | 'abandoned'
export type TransactionRecoveryPathKind = 'target' | 'backup' | 'staging' | 'parent' | 'journal'
export interface TransactionRecoveryEntry {
  id: string
  state: TransactionRecoveryState
  archived: boolean
  application_error: string | null
  recovery_error: string | null
  legacy_error: string | null
  paths: { kind: TransactionRecoveryPathKind; path: string }[]
  scope_known: boolean
  can_decide: boolean
  unknown_scope_acknowledged: boolean
  revision: string
}

export function isTransactionRecoveryError(tag: unknown) {
  return tag === 'modChanges.recoveryRequired' || tag === 'modChanges.recoveryChanged'
    || tag === 'modChanges.recoveryConflictChanged' || tag === 'modChanges.recoveryMaterialProtected'
}
