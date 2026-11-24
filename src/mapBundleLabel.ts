export function mapBundleLabel(name: {
  label: string
  name_i18n?: Readonly<Record<string, string>>
  name_zh?: string | null
}, locale: string) {
  const localized = name.name_i18n?.[locale]?.trim()
  if (localized) return localized
  const english = name.label.trim()
  const chinese = name.name_zh?.trim()
  const names = locale.startsWith('zh-') ? [chinese, english] : [english, chinese]
  return [...new Set(names.filter(Boolean))].join(' · ')
}
