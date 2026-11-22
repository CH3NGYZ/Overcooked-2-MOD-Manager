import { onBeforeUnmount, onMounted, shallowRef } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  appUpdateBlocksOperations, appUpdateState, beginAppUpdateRecovery, beginStagedAppUpdate,
  failAppUpdateRecovery, finishStagedAppUpdate, type UpdateRecoveryDestination,
} from '../appUpdateState'
import { useModChanges } from '../modChanges/store'
import { isChangePlanLocked } from '../modChanges/workflow'
import { backendMessageOf } from '../locale/backend'
import { isTauri } from '../runtime'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { refreshAppUpdateRecovery } from './useAppUpdates'

interface PageCloseGuard {
  requestLeave: () => Promise<boolean>
  blocked: () => boolean
}

const pageGuards: PageCloseGuard[] = []

export const appCloseBlocked = () => appUpdateBlocksOperations()
  || isChangePlanLocked(useModChanges().state.value?.phase)

/** Route transitions can briefly overlap page lifetimes. The newest mounted
 * page owns confirmation; removing an older registration must not remove it. */
export function registerPageLeaveGuard(guard: PageCloseGuard): () => void {
  pageGuards.push(guard)
  return () => {
    const index = pageGuards.indexOf(guard)
    if (index >= 0) pageGuards.splice(index, 1)
  }
}

/** App owns the only native close listener, including routes without a draft.
 * exitApp restores pending map files before exiting; failure keeps this window
 * and listener alive, so users can resolve the error and retry. */
export function useAppCloseGuard() {
  const message = useNotify()
  const confirmingClose = shallowRef(false)
  let disposed = false
  let closing = false
  const unlisten: (() => void)[] = []

  function registerUnlisten(stop: () => void) {
    if (disposed) stop()
    else unlisten.push(stop)
  }

  async function requestExit(restart: boolean) {
    if (disposed || closing || appCloseBlocked()) return
    const guard = pageGuards.at(-1)
    if (guard?.blocked()) return
    closing = true
    let applyingStaged = false
    try {
      if (guard) {
        confirmingClose.value = true
        if (!await guard.requestLeave()) return
      }
      // A route change, save or update may start while its dialog is open.
      if (disposed || appCloseBlocked() || guard !== pageGuards.at(-1) || guard?.blocked()) return
      applyingStaged = (restart || !appUpdateState.notesPending) && beginStagedAppUpdate()
      if (restart && !applyingStaged) return
      if (restart) await api.applyStagedAppUpdate()
      else await api.exitApp(applyingStaged)
      if (applyingStaged) finishStagedAppUpdate()
    } catch (reason) {
      const rejected = reason ?? (applyingStaged ? 'update.installFailedManual' : 'update.inProgress')
      if (applyingStaged) finishStagedAppUpdate(rejected)
      const tag = rejected instanceof Error ? rejected.message : rejected
      if (tag === 'update.installFailedManual') {
        void refreshAppUpdateRecovery().catch(error => { if (!disposed) message.error(backendMessageOf(error)) })
      } else if (!disposed && !(applyingStaged && appUpdateState.error === 'update.stagedUnavailable')) {
        message.error(backendMessageOf(rejected))
      }
    } finally {
      confirmingClose.value = false
      closing = false
    }
  }

  async function applyUpdate() {
    if (!isTauri() || appUpdateState.stage !== 'ready') return
    await requestExit(true)
  }

  async function recoverUpdate(destination: UpdateRecoveryDestination) {
    if (!isTauri() || disposed || closing || !beginAppUpdateRecovery()) return
    closing = true
    try { await api.finishAppUpdateRecovery(destination) }
    catch (reason) { failAppUpdateRecovery(backendMessageOf(reason)) }
    finally { closing = false }
  }

  async function listenForClose() {
    if (!isTauri()) return
    try {
      await Promise.all([
        getCurrentWindow().onCloseRequested(async event => {
          event.preventDefault()
          await requestExit(false)
        }).then(registerUnlisten),
        listen('app-quit-requested', () => { void requestExit(false) }).then(registerUnlisten),
      ])
    } catch (reason) {
      if (!disposed) message.error(backendMessageOf(reason))
    }
  }

  onMounted(() => { void listenForClose() })
  onBeforeUnmount(() => {
    disposed = true
    unlisten.splice(0).forEach(stop => stop())
  })
  return { applyUpdate, recoverUpdate, confirmingClose }
}
