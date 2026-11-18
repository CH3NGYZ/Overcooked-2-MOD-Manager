import { t } from './i18n.ts'
import { formatFileSize } from './repositoryAssets.ts'
import type { DownloadProgress } from './types'

export function downloadPercent({ downloaded, total, phase }: Pick<DownloadProgress, 'downloaded' | 'total' | 'phase'>): number | null {
  if (!total || (downloaded === 0 && phase !== 'downloading')) return null
  return Math.min(100, downloaded / total * 100)
}

export function downloadAmount({ downloaded, total }: Pick<DownloadProgress, 'downloaded' | 'total'>): string {
  return `${downloaded === 0 ? '0 B' : formatFileSize(downloaded)} / ${total ? formatFileSize(total) : t('tools.downloadTotalUnknown')}`
}

export function downloadRate({ phase, speed_bytes }: Pick<DownloadProgress, 'phase' | 'speed_bytes'>): string {
  if (speed_bytes === 0 && phase !== 'downloading') return ''
  const mib = speed_bytes / (1024 * 1024)
  return mib >= 1 ? `${mib.toFixed(1)} MB/s` : `${Math.round(speed_bytes / 1024)} KB/s`
}

export function downloadRoute({ route, source }: Pick<DownloadProgress, 'route' | 'source'>): string {
  const kind = route === 'proxy' ? t('common.proxy') : route === 'direct' ? t('common.direct') : ''
  return [kind, source].filter(Boolean).join(' · ')
}

export function downloadRace({ parallel_sources, attempt, attempt_total }: Pick<DownloadProgress, 'parallel_sources' | 'attempt' | 'attempt_total'>): string {
  const parts: string[] = []
  if (parallel_sources && parallel_sources > 1) parts.push(t('tools.sourcesRacing', { count: parallel_sources }))
  if (attempt && attempt_total && attempt_total > 1) parts.push(t('tools.attempt', { current: attempt, total: attempt_total }))
  return parts.join(' · ')
}
