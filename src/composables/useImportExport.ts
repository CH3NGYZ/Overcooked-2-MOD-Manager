import { ref } from 'vue'
import { t } from '../i18n'

/**
 * Repository TOML import/export: clipboard and file import with preview and
 * conflict strategies, plus copy-to-clipboard and file export.
 */
export function useImportExport(options: {
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  requestConfirmation: (request: { title: string; message: string; danger?: boolean }) => Promise<boolean>
  loadDashboard: () => Promise<void>
  api: typeof import('../services/tauri').api
}) {
  const { run, requestConfirmation, loadDashboard, api } = options
  const showImport = ref(false)
  const showExport = ref(false)
  const importText = ref('')
  const importPreview = ref<{ valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] } | null>(null)

  function exportRepositories() {
    showExport.value = true
  }

  async function continueExportRepositories() {
    const path = await api.chooseExportFile()
    if (!path || Array.isArray(path)) return
    await run('正在导出仓库 TOML', () => api.exportFile(path, null))
    showExport.value = false
  }

  async function copyRepositories() {
    await run('正在复制仓库 TOML', () => api.copyRepositories(null))
    showExport.value = false
  }

  async function importFromClipboard() { importText.value = await api.readClipboard(); await previewImport() }

  async function importFromFile() {
    const path = await api.chooseImportFile()
    if (!path || Array.isArray(path)) return
    importText.value = await api.readTextFile(path)
    await previewImport()
  }

  async function previewImport() { importPreview.value = await run('正在检查仓库 TOML', () => api.previewImport(importText.value)) }

  async function applyImport(conflict: string) {
    if (conflict === 'overwrite' && !await requestConfirmation({ title: t('导入仓库配置'), message: t('本地的仓库会直接覆盖成粘贴的内容，并且无法恢复，是否确定？'), danger: true })) return
    await run('正在导入仓库配置', async () => { await api.importRepositories(importText.value, conflict); showImport.value = false; await loadDashboard() })
  }

  return { showImport, showExport, importText, importPreview, exportRepositories, continueExportRepositories, copyRepositories, importFromClipboard, importFromFile, previewImport, applyImport }
}

export type ImportExport = ReturnType<typeof useImportExport>
