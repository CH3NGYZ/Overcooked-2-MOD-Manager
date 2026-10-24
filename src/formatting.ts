import type { ReleaseAsset } from './types'
import { t, type Locale } from './i18n'

export function formatBytes(value: number) { if (!value) return '0 B'; const units = ['B', 'KB', 'MB', 'GB']; const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1); return `${(value / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}` }

export function formatDate(value: string | undefined, locale: Locale) { if (!value) return t('未知时间'); const language = ({ 'zh-CN': 'zh-CN', 'zh-TW': 'zh-TW', en: 'en-US', ko: 'ko-KR', ja: 'ja-JP' } as Record<Locale, string>)[locale]; return new Intl.DateTimeFormat(language, { year: 'numeric', month: 'short', day: 'numeric' }).format(new Date(value)) }

export function formatAssetMetadata(asset: ReleaseAsset) {
  const size = formatBytes(asset.size)
  return asset.download_count == null ? size : `${size} · ${t('下载次数：{count}').replace('{count}', String(asset.download_count))}`
}
