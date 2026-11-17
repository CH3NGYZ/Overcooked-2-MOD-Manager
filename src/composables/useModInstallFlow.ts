/** Local DLL, ZIP, URL and dropped files join the same atomic change plan. */
import { ref } from 'vue'
import { useGameOperationGate } from './useGameOperationGate'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import { useModChanges } from '../modChanges/store'
import type { InstallSource, ModPreview } from '../types'

export function useModInstallFlow(onChanged?: () => void | Promise<void>) {
  const message = useNotify()
  const { blockModOperation } = useGameOperationGate()
  const changes = useModChanges()
  const sourceOpen = ref(false)
  const sourceBusy = ref(false)

  function openSource() {
    if (!blockModOperation()) sourceOpen.value = true
  }

  async function openPreview(next: ModPreview) {
    try {
      if (blockModOperation()) return
      const result = await changes.run({ roots: [{ node_id: `preview:${next.preview_id}`, enabled: true }] })
      if (!result) return
      if (result.message) message.success(translateBackendMessage(result.message))
      await onChanged?.()
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      await api.cancelModPreview(next.preview_id).catch(() => undefined)
    }
  }

  async function handleSourceSelect(source: InstallSource) {
    if (blockModOperation() || sourceBusy.value) return
    sourceBusy.value = true
    const token = source.kind === 'url' ? beginDownloadTask({
      label: t('direct.onlineFile'), retry: () => { void handleSourceSelect(source) },
    }) : null
    try {
      const next = await api.analyzeModSource(source.kind, source.value)
      if (token !== null) endDownloadTask(token)
      sourceOpen.value = false
      await openPreview(next)
    } catch (reason) {
      if (token !== null) failDownloadTask(token, reason)
      message.error(backendMessageOf(reason))
    } finally {
      sourceBusy.value = false
    }
  }

  return { sourceOpen, sourceBusy, openSource, openPreview, handleSourceSelect,
    pickSourceFile: () => api.chooseModFile() }
}
