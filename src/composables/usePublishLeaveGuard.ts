import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useDialog } from 'naive-ui'
import { t } from '../i18n'
import { useNotify } from '../useNotify'
import { backendMessageOf } from '../locale/backend'
import { markDialogBusy } from '../dialogBusy'

/** 页面返回、路由离开和关窗共用一个确认/清理门，不在卸载后做异步删除。 */
export function usePublishLeaveGuard(
  busy: Readonly<Ref<boolean>>,
  uploadedName: Readonly<Ref<string>>,
  discard: () => Promise<boolean>,
) {
  const dialog = useDialog()
  const message = useNotify()
  const confirming = ref(false)
  let request: Promise<boolean> | null = null
  let cancelDialog: (() => void) | null = null
  function requestLeave(): Promise<boolean> {
    if (request) return request
    if (busy.value) return Promise.resolve(false)
    if (!uploadedName.value) return Promise.resolve(true)
    confirming.value = true
    request = new Promise<boolean>(resolve => {
      let settled = false
      const finish = (ok: boolean) => {
        if (settled) return
        settled = true
        confirming.value = false
        request = null
        cancelDialog = null
        resolve(ok)
      }
      const instance = dialog.warning({
        title: t('mapPublish.discardUploadTitle'),
        content: t('mapPublish.discardUploadConfirm', { name: uploadedName.value }),
        positiveText: t('mapPublish.deleteAndLeave'),
        negativeText: t('mapPublish.continueEditing'),
        onPositiveClick: async () => {
          if (instance.loading) return false
          instance.closeOnEsc = false
          markDialogBusy(instance)
          try {
            if (await discard()) { finish(true); return true }
          } catch (reason) { message.error(backendMessageOf(reason)) }
          instance.loading = false
          instance.closeOnEsc = true
          instance.closable = true
          instance.maskClosable = true
          instance.negativeButtonProps = { disabled: false }
          return false
        },
        onNegativeClick: () => { if (instance.loading) return false; finish(false) },
        onClose: () => { if (instance.loading) return false; finish(false) },
        onMaskClick: () => { if (!instance.loading) finish(false) },
        onEsc: () => { if (!instance.loading) finish(false) },
        onAfterLeave: () => { finish(false) },
      })
      cancelDialog = () => { finish(false); instance.destroy() }
    })
    return request
  }
  onBeforeRouteLeave(() => requestLeave())

  let disposed = false
  let unlisten: (() => void) | null = null
  let closing = false
  async function listenForClose() {
    try {
      const appWindow = getCurrentWindow()
      const stop = await appWindow.onCloseRequested(async event => {
        event.preventDefault()
        if (closing) return
        closing = true
        try {
          if (!await requestLeave() || disposed) return
          // 移除拦截后再 close，沿用现有关闭权限，不扩大到强制 destroy。
          await unlisten?.()
          unlisten = null
          try { await appWindow.close() }
          catch (reason) { message.error(backendMessageOf(reason)); await listenForClose() }
        } finally { closing = false }
      })
      if (disposed) stop()
      else unlisten = stop
    } catch (reason) { if (!disposed) message.error(backendMessageOf(reason)) }
  }
  function preventReload(event: BeforeUnloadEvent) {
    if (!busy.value && !uploadedName.value) return
    event.preventDefault()
    event.returnValue = ''
  }
  onMounted(() => {
    void listenForClose()
    window.addEventListener('beforeunload', preventReload)
  })
  onBeforeUnmount(() => {
    disposed = true
    unlisten?.()
    cancelDialog?.()
    window.removeEventListener('beforeunload', preventReload)
  })
  return { confirming, requestLeave }
}
