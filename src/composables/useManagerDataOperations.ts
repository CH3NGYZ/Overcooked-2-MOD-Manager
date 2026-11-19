import { computed, h, onBeforeUnmount, onMounted, shallowRef } from 'vue'
import { NAlert, useDialog } from 'naive-ui'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'

type DataAction = 'clear' | 'migrate'

/** One gate owns both exit-time data operations, including directory selection. */
export function useManagerDataOperations() {
  const dialog = useDialog()
  const message = useNotify()
  const managerDataDirectory = shallowRef('')
  const dataDirectoryError = shallowRef('')
  const configConfirmOpen = shallowRef(false)
  const clearingConfig = shallowRef(false)
  const migratingData = shallowRef(false)
  const selectingDataDirectory = shallowRef(false)
  const managerDataBusy = computed(() => configConfirmOpen.value || clearingConfig.value
    || migratingData.value || selectingDataDirectory.value)
  const managerDataDescription = computed(() => managerDataDirectory.value || dataDirectoryError.value
    || t('tools.managerDataDirectoryLoading'))
  let directoryRequest: Promise<string> | undefined
  let disposed = false

  onBeforeUnmount(() => { disposed = true })

  function loadManagerDataDirectory(): Promise<string> {
    if (managerDataDirectory.value) return Promise.resolve(managerDataDirectory.value)
    if (directoryRequest) return directoryRequest
    dataDirectoryError.value = ''
    directoryRequest = api.getManagerDataDirectory().then(directory => {
      if (!disposed) managerDataDirectory.value = directory
      return directory
    }).catch(reason => {
      if (!disposed) dataDirectoryError.value = backendMessageOf(reason)
      throw reason
    }).finally(() => {
      directoryRequest = undefined
    })
    return directoryRequest
  }

  onMounted(() => { void loadManagerDataDirectory().catch(() => {}) })

  function showConfirmation(action: DataAction, directory: string, parentDirectory?: string) {
    const migrating = action === 'migrate'
    // This is presentation only; the backend derives and validates the target.
    const target = parentDirectory === undefined ? '' : `${parentDirectory.replace(/[\\/]+$/, '')}${parentDirectory.includes('\\') ? '\\' : '/'}OC2ModManager`
    const path = (value: string) => h('code', {
      style: { display: 'block', overflowWrap: 'anywhere', whiteSpace: 'pre-wrap' },
    }, value)
    const actionError = shallowRef('')
    let active = true
    const confirmation = dialog[migrating ? 'warning' : 'error']({
      title: t(migrating ? 'tools.migrateData' : 'tools.clearAllConfig'),
      content: () => h('div', { class: 'manager-data-confirm',
        style: { maxHeight: 'calc(100vh - 250px)', display: 'flex', flexDirection: 'column' } }, [
        actionError.value ? h(NAlert, { type: 'error', 'data-testid': 'manager-data-action-error', role: 'alert',
          style: { flexShrink: '0', overflowWrap: 'anywhere' } }, { default: () => actionError.value }) : null,
        h('div', { class: 'manager-data-confirm-body', style: { overflowY: 'auto', minHeight: '0' } }, [
          h('p', t(migrating ? 'tools.migrateDataConfirm' : 'tools.clearAllConfigConfirm')),
          h('p', t(migrating ? 'tools.migrateDataSource' : 'tools.managerDataDirectory')),
          path(directory),
          ...(migrating ? [h('p', t('tools.migrateDataTarget')), path(target)] : []),
          h('p', t(migrating ? 'tools.migrateDataPreservesGame' : 'tools.clearAllConfigPreservesGame')),
          h('p', { style: { fontWeight: '600' } }, t(migrating ? 'tools.migrateDataExit' : 'tools.clearAllConfigExit')),
        ]),
      ]),
      positiveText: t(migrating ? 'tools.migrateDataAndExit' : 'tools.clearAllConfigAndExit'),
      negativeText: t('recycle.cancel'),
      positiveButtonProps: { type: migrating ? 'warning' : 'error' },
      maskClosable: false,
      onAfterLeave: () => { active = false; configConfirmOpen.value = false },
      onPositiveClick: async () => {
        if (!active || disposed || clearingConfig.value || migratingData.value) return false
        const executing = migrating ? migratingData : clearingConfig
        actionError.value = ''
        executing.value = true
        confirmation.loading = true
        confirmation.closable = false
        confirmation.closeOnEsc = false
        confirmation.negativeButtonProps = { disabled: true }
        try {
          // A successful call exits; the current session must stay locked.
          if (migrating && parentDirectory !== undefined) await api.migrateManagerData(parentDirectory)
          else await api.clearAllConfig()
        } catch (reason) {
          if (active && !disposed) actionError.value = backendMessageOf(reason)
          executing.value = false
          confirmation.loading = false
          confirmation.closable = true
          confirmation.closeOnEsc = true
          confirmation.negativeButtonProps = { disabled: false }
        }
        return false
      },
    })
  }

  async function confirmClearAllConfig() {
    if (disposed || managerDataBusy.value) return
    configConfirmOpen.value = true
    try {
      const directory = await loadManagerDataDirectory()
      if (!disposed) showConfirmation('clear', directory)
    } catch (reason) {
      configConfirmOpen.value = false
      if (!disposed) message.error(backendMessageOf(reason))
    }
  }

  async function confirmMigrateManagerData() {
    if (disposed || managerDataBusy.value) return
    selectingDataDirectory.value = true
    try {
      const parentDirectory = await api.chooseManagerDataParentDirectory()
      if (!parentDirectory || disposed) return
      const directory = await loadManagerDataDirectory()
      if (disposed) return
      configConfirmOpen.value = true
      showConfirmation('migrate', directory, parentDirectory)
    } catch (reason) {
      configConfirmOpen.value = false
      if (!disposed) message.error(backendMessageOf(reason))
    } finally {
      selectingDataDirectory.value = false
    }
  }

  return { managerDataDescription, clearingConfig, migratingData, selectingDataDirectory, managerDataBusy,
    confirmClearAllConfig, confirmMigrateManagerData }
}
