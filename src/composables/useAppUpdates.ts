import { computed } from 'vue'
import { useDialogReactiveList } from 'naive-ui'
import {
  appUpdateState, beginAppUpdateCheck, finishAppUpdateCheck, finishBackgroundAppUpdate,
  requireAppUpdateRecoveryCheck, retryBackgroundAppUpdate, setAppDistribution,
  setBackgroundAppUpdateProgress, setCheckingUpdateRecovery,
} from '../appUpdateState.ts'
import { activeDownloadProgress, downloadTaskState } from '../downloadTask'
import { appModalOpen } from '../dialogBusy'
import { useModChanges } from '../modChanges/store'
import { api } from '../services/tauri'
import { demoActive } from '../tutorial/demo'

export function useAppUpdatePromptMode() {
  const dialogs = useDialogReactiveList()
  return computed(() => {
    const stage = appUpdateState.stage
    if (stage === 'recovery' || stage === 'openingRecovery') return 'recovery'
    if (appUpdateState.notesPending && (stage === 'ready' || stage === 'applying')
      && !appModalOpen.value && !dialogs.value.length
      && !demoActive.value && !useModChanges().state.value
      && !activeDownloadProgress.value && !downloadTaskState.failure) return 'notes'
    return null
  })
}

let startupRecovery: Promise<void> | undefined
let check: Promise<void> | undefined
let openingStore: Promise<void> | undefined
const REATTACH_INTERVAL_MS = 2000
const REATTACH_ATTEMPTS = 60

async function stageUpdate(channel: string): Promise<void> {
  for (let attempt = 0; appUpdateState.stage === 'downloading'; attempt += 1) {
    try {
      const update = await api.stageAppUpdate(channel, setBackgroundAppUpdateProgress)
      finishBackgroundAppUpdate({ update })
      return
    } catch (reason) {
      const error = typeof reason === 'string' ? reason : reason instanceof Error ? reason.message : 'update.stageFailed'
      if (error !== 'update.inProgress' || attempt >= REATTACH_ATTEMPTS) {
        finishBackgroundAppUpdate({ error: error || 'update.stageFailed' })
        if (error === 'update.installFailedManual') await refreshAppUpdateRecovery()
        return
      }
      await new Promise(resolve => setTimeout(resolve, REATTACH_INTERVAL_MS))
    }
  }
}

export function retryAppUpdateDownload(): void {
  const channel = retryBackgroundAppUpdate()
  if (channel) void stageUpdate(channel)
}

function openStoreUpdates(): Promise<void> {
  return openingStore ??= api.openUpdateRepository().then(result => {
    if (!result.ok) throw new Error(result.message)
  }).finally(() => { openingStore = undefined })
}

async function runCheck(force: boolean): Promise<void> {
  await startupRecovery
  if (appUpdateState.distribution === 'microsoft_store') {
    setCheckingUpdateRecovery(false)
    if (force) await openStoreUpdates()
    return
  }
  if (!beginAppUpdateCheck()) return
  const result = await api.checkAppUpdate('zh-CN', force).catch(() => undefined)
  const channel = finishAppUpdateCheck(result)
  setCheckingUpdateRecovery(false)
  if (channel) void stageUpdate(channel)
  else if (force && result?.distribution === 'microsoft_store') await openStoreUpdates()
}

export function checkForAppUpdate(force = true): Promise<void> {
  return check ??= runCheck(force).finally(() => { check = undefined })
}

export async function refreshAppUpdateRecovery(): Promise<void> {
  requireAppUpdateRecoveryCheck()
  try {
    await check
    await checkForAppUpdate()
  }
  finally { setCheckingUpdateRecovery(false) }
}

export function initializeAppUpdates(): void {
  void api.getAppDistribution().then(setAppDistribution).catch(() => undefined)
  startupRecovery ??= api.getPendingAppUpdateRecovery()
    .then(setCheckingUpdateRecovery).catch(() => undefined)
  void checkForAppUpdate(false)
}
