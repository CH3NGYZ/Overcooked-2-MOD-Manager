import { computed, type Ref } from 'vue'
import { useDialog } from 'naive-ui'
import { t } from '../i18n'
import { useNotify } from '../useNotify'
import { backendMessageOf } from '../locale/backend'
import { markDialogBusy } from '../dialogBusy'
import { usePageLeaveGuard } from './usePageLeaveGuard'

/** 页面返回、路由离开和关窗共用一个确认/清理门，不在卸载后做异步删除。 */
export function usePublishLeaveGuard(
  busy: Readonly<Ref<boolean>>,
  uploadedName: Readonly<Ref<string>>,
  discard: () => Promise<boolean>,
  dirty: Readonly<Ref<boolean>>,
) {
  const dialog = useDialog()
  const message = useNotify()
  let cancelDialog: (() => void) | null = null
  function confirm(): Promise<boolean> {
    return new Promise<boolean>(resolve => {
      let settled = false
      const finish = (ok: boolean) => {
        if (settled) return
        settled = true
        cancelDialog = null
        resolve(ok)
      }
      const instance = dialog.warning({
        title: uploadedName.value ? t('mapPublish.discardUploadTitle') : t('mapPublish.discardDraftTitle'),
        content: uploadedName.value ? t('mapPublish.discardUploadConfirm', { name: uploadedName.value }) : t('mapPublish.discardDraftConfirm'),
        positiveText: uploadedName.value ? t('mapPublish.deleteAndLeave') : t('mapPublish.discardAndLeave'),
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
  }
  return usePageLeaveGuard({
    busy,
    dirty: computed(() => Boolean(uploadedName.value) || dirty.value),
    confirm,
    cancelConfirm: () => cancelDialog?.(),
  })
}
