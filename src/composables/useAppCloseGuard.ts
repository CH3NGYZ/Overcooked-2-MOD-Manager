import { onBeforeUnmount, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { appUpdateState } from '../appUpdateState'
import { backendMessageOf } from '../locale/backend'
import { isTauri } from '../runtime'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'

interface PageCloseGuard {
  requestLeave: () => Promise<boolean>
  blocked: () => boolean
}

const pageGuards: PageCloseGuard[] = []

export const appCloseBlocked = () => appUpdateState.stage === 'installing' || appUpdateState.stage === 'handoff'

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
  let disposed = false
  let closing = false
  let unlisten: (() => void) | null = null

  async function listenForClose() {
    if (!isTauri()) return
    try {
      const stop = await getCurrentWindow().onCloseRequested(async event => {
        event.preventDefault()
        if (disposed || closing || appCloseBlocked()) return
        const guard = pageGuards.at(-1)
        if (guard?.blocked()) return
        closing = true
        try {
          if (guard && !await guard.requestLeave()) return
          // A route change, save or update may start while its dialog is open.
          if (disposed || appCloseBlocked() || guard !== pageGuards.at(-1) || guard?.blocked()) return
          await api.exitApp()
        } catch (reason) {
          if (!disposed) message.error(backendMessageOf(reason))
        } finally { closing = false }
      })
      if (disposed) stop()
      else unlisten = stop
    } catch (reason) {
      if (!disposed) message.error(backendMessageOf(reason))
    }
  }

  onMounted(() => { void listenForClose() })
  onBeforeUnmount(() => {
    disposed = true
    unlisten?.()
    unlisten = null
  })
}
