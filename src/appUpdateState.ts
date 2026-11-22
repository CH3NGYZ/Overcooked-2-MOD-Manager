import { readonly, shallowReactive } from 'vue'
import type { AppDistribution, AppUpdateCheckResult, AppUpdateInfo, DownloadProgress } from './types.ts'

export type UpdateStage = 'idle' | 'checking' | 'current' | 'checkFailed' | 'downloading' | 'stageFailed'
  | 'ready' | 'applying' | 'handoff' | 'recovery' | 'openingRecovery' | 'systemReboot' | 'systemApplying' | 'restartFailed'
export type UpdateRecoveryDestination = 'website' | 'alternate'

const state = shallowReactive<{
  distribution: AppDistribution | null
  update: AppUpdateInfo | null
  stage: UpdateStage
  error: string
  progress: DownloadProgress | null
  checkingRecovery: boolean
  notesPending: boolean
}>({
  distribution: null, update: null, stage: 'idle', error: '', progress: null,
  checkingRecovery: false, notesPending: false,
})

export const appUpdateState = readonly(state)

export function appUpdateBusy(): boolean {
  return state.stage === 'applying' || state.stage === 'handoff' || state.stage === 'openingRecovery'
}

export function appUpdateBlocksOperations(): boolean {
  return state.checkingRecovery || state.stage === 'recovery' || appUpdateBusy()
}

export function setAppDistribution(distribution: AppDistribution): void {
  state.distribution = distribution
}

export function setCheckingUpdateRecovery(checking: boolean): void {
  state.checkingRecovery = checking
}

export function beginAppUpdateCheck(): boolean {
  if (state.distribution === 'microsoft_store' || !canCheckAppUpdate()) return false
  state.stage = 'checking'
  state.error = ''
  return true
}

export function canCheckAppUpdate(): boolean {
  return ['idle', 'current', 'checkFailed', 'stageFailed', 'systemReboot', 'systemApplying', 'restartFailed'].includes(state.stage)
}

export function finishAppUpdateCheck(result?: AppUpdateCheckResult): string | undefined {
  if (state.stage !== 'checking') return
  if (result?.distribution) setAppDistribution(result.distribution)
  const update = state.distribution === 'microsoft_store' || result?.all_failed ? null : result?.update ?? null
  state.update = update
  if (state.distribution === 'microsoft_store') state.stage = 'idle'
  else if (result?.installation_status === 'reboot_required') state.stage = 'systemReboot'
  else if (result?.installation_status === 'applying') state.stage = 'systemApplying'
  else if (result?.installation_status === 'restart_failed') state.stage = 'restartFailed'
  else if (!result || result.all_failed) state.stage = 'checkFailed'
  else if (!update) state.stage = 'current'
  else if (result.recovery_required) state.stage = 'recovery'
  else if (update.background_install && update.source_channel) return startBackgroundAppUpdate(update.source_channel)
  else {
    state.stage = 'stageFailed'
    state.error = 'update.deferredUnsupported'
  }
}

function startBackgroundAppUpdate(channel: string): string {
  state.error = ''
  state.progress = null
  state.stage = 'downloading'
  return channel
}

export function retryBackgroundAppUpdate(): string | undefined {
  if (!canRetryAppUpdateDownload()) return
  const channel = state.update?.source_channel
  if (channel) return startBackgroundAppUpdate(channel)
}

export function canRetryAppUpdateDownload(): boolean {
  return state.stage === 'stageFailed' && !['update.deferredUnsupported', 'update.installFailedManual'].includes(state.error)
    && !!state.update?.background_install && !!state.update.source_channel
}

export function setBackgroundAppUpdateProgress(progress: DownloadProgress): void {
  if (state.stage === 'downloading') state.progress = progress
}

export function finishBackgroundAppUpdate(outcome: { update: AppUpdateInfo | null } | { error: string }): void {
  if (state.stage !== 'downloading') return
  state.progress = null
  if ('error' in outcome) {
    state.error = outcome.error
    state.stage = outcome.error === 'update.rebootRequired' ? 'systemReboot'
      : outcome.error === 'update.restartFailed' ? 'restartFailed' : 'stageFailed'
    return
  }
  state.update = outcome.update
  state.stage = outcome.update ? 'ready' : 'current'
  state.notesPending = !!outcome.update && notesVersion(outcome.update) !== dismissedNotesVersion()
}

const dismissedNotesKey = 'oc2-update-dismissed-notes'
const notesVersion = (update: AppUpdateInfo) => `${update.current_version}:${update.version}`

function dismissedNotesVersion(): string | null {
  try { return globalThis.sessionStorage?.getItem(dismissedNotesKey) ?? null }
  catch { return null }
}

export function dismissAppUpdateNotes(): void {
  if (state.stage !== 'ready' || !state.update) return
  state.notesPending = false
  try { globalThis.sessionStorage?.setItem(dismissedNotesKey, notesVersion(state.update)) } catch { /* storage can be unavailable */ }
}

export function beginStagedAppUpdate(): boolean {
  if (state.stage !== 'ready') return false
  state.stage = 'applying'
  return true
}

export function finishStagedAppUpdate(error?: unknown): void {
  if (state.stage !== 'applying') return
  const reason = error instanceof Error ? error.message : error
  if (reason === 'update.stagedUnavailable' || reason === 'update.installFailedManual') {
    state.error = reason
    state.notesPending = false
    state.stage = 'stageFailed'
    return
  }
  state.stage = error === undefined ? 'handoff' : 'ready'
}

export function requireAppUpdateRecoveryCheck(): void {
  state.checkingRecovery = true
  state.error = 'update.installFailedManual'
  state.notesPending = false
  state.stage = 'stageFailed'
}

export function beginAppUpdateRecovery(): boolean {
  if (state.stage !== 'recovery') return false
  state.stage = 'openingRecovery'
  return true
}

export function failAppUpdateRecovery(error: string): void {
  if (state.stage !== 'openingRecovery') return
  state.error = error
  state.stage = 'recovery'
}
