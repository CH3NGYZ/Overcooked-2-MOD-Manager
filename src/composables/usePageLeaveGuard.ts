import { onBeforeUnmount, onMounted, shallowRef, type Ref } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'
import { backendMessageOf } from '../locale/backend'
import { useNotify } from '../useNotify'
import { appCloseBlocked, registerPageLeaveGuard } from './useAppCloseGuard'

interface PageLeaveOptions {
  busy: Readonly<Ref<boolean>>
  dirty: Readonly<Ref<boolean>>
  confirm: () => Promise<boolean>
  cancelConfirm?: () => void
}

/** Pages own their confirmation policy, route/reload protection and cleanup.
 * The App-level close owner consults the same request without another listener. */
export function usePageLeaveGuard(options: PageLeaveOptions) {
  const message = useNotify()
  const confirming = shallowRef(false)
  let request: Promise<boolean> | null = null
  let cancelRequest: (() => void) | null = null
  let disposed = false
  let unregister: (() => void) | null = null
  const blocked = () => disposed || appCloseBlocked() || options.busy.value

  function requestLeave(): Promise<boolean> {
    if (disposed || appCloseBlocked()) return Promise.resolve(false)
    if (request) return request
    if (options.busy.value) return Promise.resolve(false)
    if (!options.dirty.value) return Promise.resolve(true)
    confirming.value = true
    request = new Promise<boolean>(resolve => {
      let settled = false
      const finish = (ok: boolean) => {
        if (settled) return
        settled = true
        confirming.value = false
        request = null
        cancelRequest = null
        resolve(ok && !blocked())
      }
      cancelRequest = () => finish(false)
      // Start after request is assigned, so an immediate resolution cannot leave
      // a stale in-flight promise behind.
      void Promise.resolve().then(() => blocked() ? false : options.confirm()).then(finish, reason => {
        if (!disposed) message.error(backendMessageOf(reason))
        finish(false)
      })
    })
    return request
  }
  onBeforeRouteLeave(() => requestLeave())

  function preventReload(event: BeforeUnloadEvent) {
    if (!options.busy.value && !options.dirty.value) return
    event.preventDefault()
    event.returnValue = ''
  }
  onMounted(() => {
    unregister = registerPageLeaveGuard({ requestLeave, blocked })
    window.addEventListener('beforeunload', preventReload)
  })
  onBeforeUnmount(() => {
    disposed = true
    unregister?.()
    cancelRequest?.()
    options.cancelConfirm?.()
    window.removeEventListener('beforeunload', preventReload)
  })
  return { confirming, requestLeave }
}
