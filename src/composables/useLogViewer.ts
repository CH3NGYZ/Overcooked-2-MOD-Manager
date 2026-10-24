import { ref } from 'vue'
import { api } from '../services/tauri'

export type LogViewerKind = 'rust' | 'frontend' | 'crash'

/**
 * Reader for the three log files under the user log directory. Reading never
 * appends to or rewrites the files; failures surface as plain content.
 */
export function useLogViewer(options: { run: <T>(label: string, action: () => Promise<T>) => Promise<T> }) {
  const showRustLog = ref(false)
  const rustLog = ref({ path: '', content: '' })
  const logViewerKind = ref<LogViewerKind>('rust')

  async function refresh() {
    try {
      rustLog.value = logViewerKind.value === 'rust'
        ? await api.readRustLog()
        : logViewerKind.value === 'frontend'
          ? await api.readFrontendLog()
          : await api.readCrashLog()
    } catch (reason) {
      rustLog.value = { path: '', content: String(reason) }
    }
  }

  async function open(kind: LogViewerKind) {
    logViewerKind.value = kind
    showRustLog.value = true
    await refresh()
  }

  function close() {
    showRustLog.value = false
  }

  const openRustLog = () => open('rust')
  const openFrontendLog = () => open('frontend')
  const openCrashLog = () => open('crash')
  const openDeveloperConsole = () => options.run('正在打开开发者控制台', api.openDevtools)

  return { showRustLog, rustLog, logViewerKind, refresh, open, close, openRustLog, openFrontendLog, openCrashLog, openDeveloperConsole }
}
