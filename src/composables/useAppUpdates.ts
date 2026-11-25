import { computed } from 'vue'
import { useDialogReactiveList } from 'naive-ui'
import {
  appUpdateExitInProgress, appUpdateState, beginAppUpdateCheck, beginDeferringAppUpdate, finishAppUpdateCheck, finishBackgroundAppUpdate,
  finishDeferringAppUpdate, isRequiredAppUpdate, setAppDistribution, setBackgroundAppUpdateProgress,
} from '../appUpdateState.ts'
import { appModalOpen } from '../dialogBusy'
import { backendMessageOf, backendPayloadOf } from '../locale/backend'
import { useModChanges } from '../modChanges/store'
import { isChangePlanLocked } from '../modChanges/workflow'
import { api } from '../services/tauri'
import { demoActive } from '../tutorial/demo'

export function useAppUpdatePromptOpen() {
  const dialogs = useDialogReactiveList()
  const changes = useModChanges()
  return computed(() => {
    if (appUpdateState.resolvingExitBlock || isChangePlanLocked(changes.state.value?.phase)) return false
    if (isRequiredAppUpdate() || appUpdateState.stage === 'deferring') return true
    if (appUpdateExitInProgress()) {
      return appUpdateState.stage !== 'applying' || appUpdateState.notesRequested || appUpdateState.update?.install_mode === 'restartAsUser'
    }
    return appUpdateState.stage === 'ready' && appUpdateState.notesRequested && !appModalOpen.value && !dialogs.value.length
      && !demoActive.value && !changes.state.value
  })
}

let check: Promise<void> | undefined
let openingStore: Promise<void> | undefined
let distribution: Promise<void> | undefined
const REATTACH_INTERVAL_MS = 2000
const REATTACH_ATTEMPTS = 60

async function stageUpdate(channel: string): Promise<void> {
  for (let attempt = 0; appUpdateState.stage === 'downloading'; attempt += 1) {
    try {
      const update = await api.stageAppUpdate(channel, setBackgroundAppUpdateProgress)
      finishBackgroundAppUpdate({ update })
      return
    } catch (reason) {
      const error = typeof reason === 'string' ? reason : reason instanceof Error ? reason.message : backendMessageOf(reason)
      const tag = backendPayloadOf(reason)?.tag ?? error
      if (tag !== 'update.inProgress' || attempt >= REATTACH_ATTEMPTS) {
        finishBackgroundAppUpdate({ error: error || 'update.stageFailed', tag })
        return
      }
      await new Promise(resolve => setTimeout(resolve, REATTACH_INTERVAL_MS))
    }
  }
}

export async function deferAppUpdate(): Promise<void> {
  const version = beginDeferringAppUpdate()
  if (!version) return
  try {
    await api.deferStagedAppUpdate(version)
    finishDeferringAppUpdate()
  } catch (reason) {
    const error = typeof reason === 'string' ? reason : reason instanceof Error ? reason.message : backendMessageOf(reason)
    finishDeferringAppUpdate(error || 'update.stagedUnavailable', backendPayloadOf(reason)?.tag)
  }
}

export function openAppUpdatePage(version?: string): Promise<void> {
  if (isRequiredAppUpdate()) return Promise.reject(new Error('update.requiredToContinue'))
  const open = (targetVersion?: string) => api.openUpdateRepository(targetVersion).then(result => {
    if (!result.ok) throw new Error(result.message)
  })
  if (appUpdateState.distribution !== 'microsoft_store') return open(version)
  return openingStore ??= open().finally(() => { openingStore = undefined })
}

async function runCheck(force: boolean): Promise<void> {
  await distribution
  if (appUpdateState.distribution === 'microsoft_store') {
    if (force) await openAppUpdatePage()
    return
  }
  if (!beginAppUpdateCheck()) return
  const result = await api.checkAppUpdate(force).catch(() => undefined)
  const channel = finishAppUpdateCheck(result)
  if (channel) void stageUpdate(channel)
  else if (force && result?.distribution === 'microsoft_store') await openAppUpdatePage()
}

export function checkForAppUpdate(force = true): Promise<void> {
  return check ??= runCheck(force).finally(() => { check = undefined })
}

export function initializeAppUpdates(): void {
  distribution ??= api.getAppDistribution().then(setAppDistribution).catch(() => undefined)
  void checkForAppUpdate(false)
}
