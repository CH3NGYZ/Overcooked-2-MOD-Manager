import { readonly, shallowReactive } from 'vue'
import type { AppDistribution, AppUpdateCheckResult, AppUpdateInfo, DownloadProgress } from './types.ts'

export type UpdateStage = 'idle' | 'checking' | 'current' | 'checkFailed' | 'downloading' | 'stageFailed'
  | 'ready' | 'deferring' | 'applying' | 'openingRelease' | 'openingWebsite' | 'exitingForReboot' | 'handoff' | 'systemReboot' | 'systemApplying' | 'restartFailed'
export type UpdateExitAction = 'apply' | 'download' | 'website' | 'reboot'

const exitStages = { apply: 'applying', download: 'openingRelease', website: 'openingWebsite', reboot: 'exitingForReboot' } as const
const pendingExitStages: readonly UpdateStage[] = Object.values(exitStages)

const state = shallowReactive<{
  distribution: AppDistribution | null
  update: AppUpdateInfo | null
  stage: UpdateStage
  error: string
  progress: DownloadProgress | null
  recheckingStatus: boolean
  deferred: boolean
  notesRequested: boolean
  resolvingExitBlock: boolean
}>({
  distribution: null, update: null, stage: 'idle', error: '', progress: null,
  recheckingStatus: false, deferred: false, notesRequested: false, resolvingExitBlock: false,
})

export const appUpdateState = readonly(state)

export function isRequiredAppUpdate(): boolean {
  return state.distribution !== 'microsoft_store' && !!state.update
    && (['stageFailed', 'systemApplying', 'systemReboot'].includes(state.stage)
      || (state.stage === 'ready' && state.update.install_mode !== 'deferred'))
}

export function appUpdateExitInProgress(): boolean {
  return pendingExitStages.includes(state.stage)
}

export function appUpdateBlocksOperations(allowRequiredUpdate = false): boolean {
  return state.stage === 'handoff' || state.stage === 'deferring' || appUpdateExitInProgress()
    || (!allowRequiredUpdate && !state.resolvingExitBlock && isRequiredAppUpdate())
}

export function presentAppUpdate(): void {
  if (!state.resolvingExitBlock) return
  state.resolvingExitBlock = false
  if (state.stage === 'ready') state.notesRequested = true
}

export function setAppDistribution(distribution: AppDistribution): void {
  state.distribution = distribution
}

export function canCheckAppUpdate(): boolean {
  return !state.recheckingStatus && ['idle', 'current', 'checkFailed', 'systemReboot', 'systemApplying', 'restartFailed'].includes(state.stage)
}

export function beginAppUpdateCheck(): boolean {
  if (state.distribution === 'microsoft_store' || !canCheckAppUpdate()) return false
  if (isRequiredAppUpdate()) state.recheckingStatus = true
  else state.stage = 'checking'
  state.error = ''
  return true
}

export function finishAppUpdateCheck(result?: AppUpdateCheckResult): string | undefined {
  if (state.stage !== 'checking' && !state.recheckingStatus) return
  state.recheckingStatus = false
  if (result?.distribution) setAppDistribution(result.distribution)
  if (state.distribution === 'microsoft_store') {
    state.update = null
    state.stage = 'idle'
    return
  }
  if (!result || (result.all_failed && !result.update)) {
    state.error = 'dialogs.updateCheckFailed'
    if (!isRequiredAppUpdate()) state.stage = 'checkFailed'
    return
  }
  const update = result.update ?? state.update
  state.update = update
  if (result.installation_status === 'reboot_required') state.stage = 'systemReboot'
  else if (result.installation_status === 'applying') state.stage = 'systemApplying'
  else if (!update) state.stage = result.installation_status === 'restart_failed' ? 'restartFailed' : 'current'
  else if (update.install_mode === 'restartAsUser' || update.install_mode === 'downloadAndExit') setReadyUpdate(update)
  else if (result.installation_status === 'restart_failed' || result.recovery_required) {
    finishPreparationWithError(result.recovery_required ? 'update.installFailedManual' : 'update.restartFailed')
  } else if (update.install_mode === 'deferred' && update.source_channel) return startBackgroundAppUpdate(update.source_channel)
  else finishPreparationWithError('update.manualUpdateRequired')
}

function startBackgroundAppUpdate(channel: string): string {
  state.error = ''
  state.progress = null
  state.stage = 'downloading'
  return channel
}

function finishPreparationWithError(error: string, tag = error): void {
  state.error = error
  state.progress = null
  state.deferred = false
  state.notesRequested = false
  state.resolvingExitBlock = false
  state.stage = tag === 'update.rebootRequired' ? 'systemReboot'
    : tag === 'update.inProgress' ? 'systemApplying' : 'stageFailed'
}

export function setBackgroundAppUpdateProgress(progress: DownloadProgress): void {
  if (state.stage === 'downloading') state.progress = progress
}

export function finishBackgroundAppUpdate(outcome: { update: AppUpdateInfo | null } | { error: string; tag?: string }): void {
  if (state.stage !== 'downloading') return
  state.progress = null
  if ('error' in outcome) finishPreparationWithError(outcome.error, outcome.tag)
  else if (outcome.update) setReadyUpdate(outcome.update)
  else finishPreparationWithError('update.stagedUnavailable')
}

function setReadyUpdate(update: AppUpdateInfo): void {
  state.update = update
  if (update.install_mode === 'manual') finishPreparationWithError('update.manualUpdateRequired')
  else {
    state.stage = 'ready'
    state.notesRequested = update.install_mode === 'deferred' && !state.deferred
  }
}

export function canExitForAppUpdate(action: UpdateExitAction): boolean {
  if (state.distribution === 'microsoft_store' || !state.update || state.recheckingStatus) return false
  if (action === 'reboot') return state.stage === 'systemReboot'
  if (action === 'download' || action === 'website') {
    return state.stage === 'stageFailed' || (state.stage === 'ready' && state.update.install_mode !== 'deferred')
  }
  return state.stage === 'ready' && (state.update?.install_mode === 'deferred' || state.update?.install_mode === 'restartAsUser')
}

export function beginDeferringAppUpdate(): string | undefined {
  if (state.stage !== 'ready' || state.update?.install_mode !== 'deferred' || state.resolvingExitBlock) return
  state.stage = 'deferring'
  return state.update.version
}

export function finishDeferringAppUpdate(error?: string, tag = error): void {
  if (state.stage !== 'deferring') return
  if (error) finishPreparationWithError(error, tag)
  else {
    state.stage = 'ready'
    state.deferred = true
    state.notesRequested = false
    state.error = ''
  }
}

export function beginDeferredAppUpdateExit(): boolean {
  return state.deferred && beginAppUpdateExit('apply')
}

export function beginAppUpdateExit(action: UpdateExitAction): boolean {
  if (!canExitForAppUpdate(action)) return false
  state.error = ''
  state.resolvingExitBlock = false
  if (action !== 'apply') state.progress = null
  state.stage = exitStages[action]
  return true
}

export function finishAppUpdateExit(previousStage: UpdateStage, error?: string, tag = error): void {
  if (!pendingExitStages.includes(state.stage)) return
  if (error === undefined) {
    state.stage = 'handoff'
    return
  }
  if (tag === 'assetBundle.recoveryPending' || tag === 'modChanges.recoveryRequired' || tag === 'levelSet.recoveryRequired') {
    state.stage = previousStage
    state.error = error
    state.notesRequested = false
    state.resolvingExitBlock = true
    return
  }
  const whileApplying = state.stage === 'applying'
  if (whileApplying || tag === 'update.rebootRequired' || tag === 'update.inProgress') {
    finishPreparationWithError(error, tag)
  } else {
    state.stage = previousStage
    state.error = error
  }
}
