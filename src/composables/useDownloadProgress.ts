import { ref } from 'vue'
import type { DownloadProgress } from '../types'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { formatBytes } from '../formatting'
import type { TutorialProgressState } from '../tutorial/useTutorialRuntime'

/**
 * Shared receiver for the backend download progress event. Only the download
 * that was explicitly begun accepts events; late events after cancel or
 * completion are ignored so stale progress bars cannot reappear.
 */
export function useDownloadProgress(options: { isUnmounted: () => boolean }) {
  const progress = ref<TutorialProgressState>({ label: '', percent: 0, speed: '', status: '' })
  const downloadCancelRequested = ref(false)
  let requestContext: Pick<DownloadProgress, 'route' | 'source' | 'attempt' | 'attempt_total'> = {}
  let active = false

  function clear() {
    active = false
    requestContext = {}
    progress.value = { label: '', percent: 0, speed: '', status: '' }
  }

  function begin() {
    clear()
    downloadCancelRequested.value = false
    active = true
  }

  function receive(value: DownloadProgress) {
    if (!active || downloadCancelRequested.value || options.isUnmounted()) return
    if (progress.value.label !== value.label) requestContext = {}
    if (value.route) requestContext = {
      route: value.route, source: value.source,
      attempt: value.attempt, attempt_total: value.attempt_total,
    }
    const route = requestContext.route === 'proxy'
      ? t('代理：{name}').replace('{name}', requestContext.source || t('自动代理'))
      : requestContext.route === 'direct' ? t('直连') : ''
    const attempt = requestContext.attempt && requestContext.attempt_total
      ? ` (${requestContext.attempt}/${requestContext.attempt_total})` : ''
    const total = value.total && value.total > 0 ? value.total : undefined
    const waiting = !value.finished && value.downloaded === 0
      && (value.phase === 'connecting' || value.phase === 'switching')
    const stage = value.finished ? t('下载完成，正在处理')
      : waiting && value.phase === 'switching' ? t('正在切换下载渠道')
      : waiting ? t('正在请求，等待响应') : ''
    const parallel = !value.finished && (value.parallel_sources || 0) > 1
      ? t('并行下载源：{count}').replace('{count}', String(value.parallel_sources)) : ''
    progress.value = {
      label: value.label,
      percent: value.finished ? 100 : total ? Math.min(100, value.downloaded / total * 100) : 0,
      speed: value.finished || waiting ? '' : `${formatBytes(value.speed_bytes)}/s`,
      status: [stage, parallel, route && `${route}${attempt}`].filter(Boolean).join(' · '),
      indeterminate: !value.finished && (waiting || !total),
    }
  }

  async function cancelDownload() {
    downloadCancelRequested.value = true
    clear()
    try { await api.cancelDownload() } catch { /* 下载任务会自行结束，进度条已隐藏 */ }
  }

  return { progress, downloadCancelRequested, clear, begin, receive, cancelDownload }
}
