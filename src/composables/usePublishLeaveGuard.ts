import type { Ref } from 'vue'
import { useDialog } from 'naive-ui'
import { t } from '../i18n'
import { usePageLeaveGuard } from './usePageLeaveGuard'

/** 页面返回、路由离开和关窗共用草稿确认；已发布附件只由显式删除操作处理。 */
export function usePublishLeaveGuard(
  busy: Readonly<Ref<boolean>>,
  dirty: Readonly<Ref<boolean>>,
) {
  const dialog = useDialog()
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
        title: t('mapPublish.discardDraftTitle'),
        content: t('mapPublish.discardDraftConfirm'),
        positiveText: t('mapPublish.discardAndLeave'),
        negativeText: t('mapPublish.continueEditing'),
        onPositiveClick: () => { finish(true) },
        onNegativeClick: () => { finish(false) },
        onClose: () => { finish(false) },
        onMaskClick: () => { finish(false) },
        onEsc: () => { finish(false) },
        onAfterLeave: () => { finish(false) },
      })
      cancelDialog = () => { finish(false); instance.destroy() }
    })
  }
  return usePageLeaveGuard({
    busy,
    dirty,
    confirm,
    cancelConfirm: () => cancelDialog?.(),
  })
}
