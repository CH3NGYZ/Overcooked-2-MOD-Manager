import { onBeforeUnmount, onMounted, shallowRef } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  appUpdateBlocksOperations, appUpdateState, beginAppUpdateExit, beginDeferredAppUpdateExit, canExitForAppUpdate, finishAppUpdateExit,
  isRequiredAppUpdate, presentAppUpdate,
  type UpdateExitAction,
} from '../appUpdateState'
import { useModChanges } from '../modChanges/store'
import { isChangePlanLocked } from '../modChanges/workflow'
import { backendMessageOf, backendPayloadOf } from '../locale/backend'
import { isTauri } from '../runtime'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'

interface PageCloseGuard {
  requestLeave: (allowRequiredUpdate?: boolean) => Promise<boolean>
  blocked: (allowRequiredUpdate?: boolean) => boolean
}

const pageGuards: PageCloseGuard[] = []

export const appCloseBlocked = (allowRequiredUpdate = false) => appUpdateBlocksOperations(allowRequiredUpdate)
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

  async function requestExit(action?: UpdateExitAction) {
    const fromUpdate = action !== undefined
    if (disposed || closing || appCloseBlocked(fromUpdate)) return
    if (!fromUpdate && appUpdateState.resolvingExitBlock && isRequiredAppUpdate()) {
      presentAppUpdate()
      return
    }
    if (action && !canExitForAppUpdate(action)) return
    const chosenUpdate = fromUpdate ? appUpdateState.update : null
    const guard = pageGuards.at(-1)
    if (guard?.blocked(fromUpdate)) return
    closing = true
    let startedUpdateExit = false
    let previousStage = appUpdateState.stage
    try {
      if (guard) {
        confirmingClose.value = true
        if (!await guard.requestLeave(fromUpdate)) return
      }
      confirmingClose.value = false
      // A route change, save or update may start while its dialog is open.
      if (disposed || appCloseBlocked(fromUpdate) || guard !== pageGuards.at(-1) || guard?.blocked(fromUpdate)) return
      if (action) {
        previousStage = appUpdateState.stage
        if (!chosenUpdate || chosenUpdate !== appUpdateState.update || !beginAppUpdateExit(action)) return
        startedUpdateExit = true
        if (action === 'download' || action === 'website') {
          await api.openAppUpdateReleaseAndExit(chosenUpdate.version, action === 'website' ? 'website' : undefined)
        }
        else if (action === 'reboot') await api.exitApp()
        else if (chosenUpdate.install_mode === 'restartAsUser') await api.restartUpdateAsUser()
        else await api.applyStagedAppUpdate()
      } else {
        previousStage = appUpdateState.stage
        startedUpdateExit = beginDeferredAppUpdateExit()
        await api.exitApp()
      }
      if (startedUpdateExit) finishAppUpdateExit(previousStage)
    } catch (reason) {
      const rejected = reason ?? (startedUpdateExit ? 'update.installFailedManual' : 'update.inProgress')
      const error = typeof rejected === 'string' ? rejected : rejected instanceof Error ? rejected.message : backendMessageOf(rejected)
      const tag = backendPayloadOf(rejected)?.tag ?? error
      if (startedUpdateExit) finishAppUpdateExit(previousStage, error, tag)
      else if (!disposed) message.error(backendMessageOf(rejected))
    } finally {
      confirmingClose.value = false
      closing = false
    }
  }

  async function updateExit(action: UpdateExitAction) {
    if (isTauri()) await requestExit(action)
  }

  async function listenForClose() {
    if (!isTauri()) return
    try {
      await Promise.all([
        getCurrentWindow().onCloseRequested(async event => {
          event.preventDefault()
          await requestExit()
        }).then(registerUnlisten),
        listen('app-quit-requested', () => { void requestExit() }).then(registerUnlisten),
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
  return { updateExit, confirmingClose }
}
