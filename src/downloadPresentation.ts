import { t } from './i18n.ts'
import { formatFileSize } from './repositoryAssets.ts'
import type { DownloadProgress, DownloadRaceSource } from './types'

type SourcePresentationPhase = DownloadProgress['phase'] | DownloadRaceSource['phase']

export function downloadPercent({ downloaded, total, phase }: Pick<DownloadProgress, 'downloaded' | 'total'> & { phase?: SourcePresentationPhase }): number | null {
  if (!total || (downloaded === 0 && phase !== 'downloading')) return null
  return Math.min(100, downloaded / total * 100)
}

export function downloadAmount({ downloaded, total }: Pick<DownloadProgress, 'downloaded' | 'total'>): string {
  const amount = downloaded === 0 ? '0 B' : formatFileSize(downloaded)
  return total ? `${amount} / ${formatFileSize(total)}` : t('tools.downloadReceived', { amount })
}

export function downloadRate({ phase, speed_bytes, race }: Pick<DownloadProgress, 'race'> & { phase?: SourcePresentationPhase; speed_bytes: number | null }): string {
  if (phase !== 'downloading') return ''
  const primary = race?.sources.find(source => source.id === race.primary_id)
  const speed = primary ? primary.speed_bytes : speed_bytes
  if (speed === null) return t('tools.downloadRace.measuring')
  const mib = speed / (1024 * 1024)
  return mib >= 1 ? `${mib.toFixed(1)} MB/s` : `${Math.round(speed / 1024)} KB/s`
}

export function downloadRoute({ route, source }: Pick<DownloadProgress, 'route' | 'source'>): string {
  const kind = route === 'proxy' ? t('common.proxy') : route === 'direct' ? t('common.direct') : ''
  return [kind, source].filter(Boolean).join(' · ')
}
