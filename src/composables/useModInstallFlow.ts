/** Local DLL, ZIP, URL and dropped files join the same atomic change plan. */
import { ref } from 'vue'
import { useGameOperationGate } from './useGameOperationGate'
import { useDropRouting, type InstallSourceEntry } from './useDropRouting'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { beginDownloadTask, endDownloadTask, failDownloadTask, updateDownloadProgress } from '../downloadTask'
import { useModChanges } from '../modChanges/store'
import type { InstallSource, ModPreview } from '../types'

export function useModInstallFlow() {
  const message = useNotify()
  const { blockModOperation } = useGameOperationGate()
  const { acceptDrop } = useDropRouting()
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
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      await api.cancelModPreview(next.preview_id).catch(() => undefined)
    }
  }

  async function handleSourceSelect(source: InstallSource, entry: InstallSourceEntry = 'picker') {
    if (sourceBusy.value || (entry !== 'drop' && blockModOperation())) return
    sourceBusy.value = true
    const token = source.kind === 'url' ? beginDownloadTask({
      label: t('direct.onlineFile'), retry: () => { void handleSourceSelect(source) },
    }) : null
    try {
      if (source.kind === 'local' && entry !== 'routed' && !(await acceptDrop(source.value, 'mod'))) return
      if (entry === 'drop' && blockModOperation()) return
      if (entry !== 'picker') sourceOpen.value = true
      const next = await api.analyzeModSource(source.kind, source.value,
        token === null ? undefined : progress => updateDownloadProgress(progress, token))
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
