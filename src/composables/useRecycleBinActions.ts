import { useDialog } from 'naive-ui'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useNotify } from '../useNotify'
import { useGameOperationGate } from './useGameOperationGate'
import { useRecycleBin } from './useRecycleBin'
import type { ModRecycleEntry, OperationResult } from '../types'

/** All entry points use the same confirmation, operation gate and read-back. */
export function useRecycleBinActions() {
  const bin = useRecycleBin()
  const dialog = useDialog()
  const message = useNotify()
  const { blockModOperation } = useGameOperationGate()

  async function run(kind: 'restore' | 'delete' | 'clear', action: () => Promise<OperationResult>, id?: string) {
    if (bin.busy.value || (kind === 'restore' && blockModOperation())) return
    bin.acting.value = { kind, id }
    try {
      const result = await action()
      if (result.ok) message.success(translateBackendMessage(result.message) || t(kind === 'restore' ? 'overlays.restore'
        : kind === 'clear' ? 'recycle.recycleBinEmptied' : 'recycle.deletePermanently'))
      else message.error(translateBackendMessage(result.message))
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      // A failed deletion may have removed part of the payload: always re-read.
      await bin.reload()
      bin.acting.value = null
    }
  }

  function confirmClearAll() {
    if (bin.busy.value || !bin.loaded.value || !bin.count.value) return
    dialog.error({
      title: t('recycle.emptyRecycleBin'),
      content: t('recycle.allRecycledEntriesAndTheirFilesWillBePermanentlyDeletedThisCannotBeUndoneEmptyTheBin', { count: bin.count.value }),
      positiveText: t('recycle.deleteAllPermanently'),
      negativeText: t('recycle.cancel'),
      onPositiveClick: () => run('clear', api.deleteAllRecycledMods),
    })
  }

  return {
    restore: (entry: ModRecycleEntry) => run('restore', () => api.restoreRecycledMod(entry.id), entry.id),
    remove: (entry: ModRecycleEntry) => run('delete', () => api.deleteRecycledMod(entry.id), entry.id),
    confirmClearAll,
  }
}
