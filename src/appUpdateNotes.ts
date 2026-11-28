import { LOCALE_CODES, LOCALE_LABELS, type Locale } from './locale/types.ts'
import { markdownHeadingSections } from './markdown.ts'
import type { AppUpdateInfo } from './types.ts'

const languageNameAliases: Record<Locale, readonly string[]> = {
  en: ['英语', '英語'],
  de: ['German', '德语', '德語'],
  fr: ['French', '法语', '法語'],
  it: ['Italian', '意大利语', '義大利語', '義大利文'],
  ko: ['Korean', '한국말', '韩语', '韓語', '韓文'],
  'es-ES': ['Español', 'Español de España', 'Spanish', 'Spanish (Spain)', '西班牙语', '西班牙語'],
  'zh-CN': ['Simplified Chinese', 'Chinese (Simplified)', '简体', '简中'],
  'zh-TW': ['Traditional Chinese', 'Chinese (Traditional)', '繁體', '繁中'],
  ru: ['Russian', 'Русский язык', '俄语', '俄語'],
  ja: ['Japanese', '日语', '日語', '日文'],
  pl: ['Polish', '波兰语', '波蘭語'],
  'pt-BR': ['Português', 'Português do Brasil', 'Portuguese', 'Brazilian Portuguese', 'Portuguese (Brazil)', '巴西葡萄牙语', '巴西葡萄牙語'],
}
const languageCodeAliases: Partial<Record<Locale, readonly string[]>> = {
  'es-ES': ['es'], 'zh-CN': ['zh-Hans'], 'zh-TW': ['zh-Hant'], 'pt-BR': ['pt'],
}

const normalizeHeading = (heading: string) => heading.normalize('NFKC').trim().toLowerCase().replace(/\s+/g, ' ')
const localesByHeading = new Map<string, { locale: Locale; isCode: boolean }>(LOCALE_CODES.flatMap(locale => [
  ...[LOCALE_LABELS[locale], ...languageNameAliases[locale]].map(heading => [normalizeHeading(heading), { locale, isCode: false }] as const),
  ...[locale, ...(languageCodeAliases[locale] ?? [])].map(heading => [normalizeHeading(heading), { locale, isCode: true }] as const),
]))

/** 每个语言段独立渲染；引用式链接的定义也应写在同一语言段。 */
export function selectReleaseNotes(source: string, locale: Locale, mode: 'manager' | 'package' = 'manager'): string {
  if (!source.trim()) return mode === 'package' ? source : ''
  for (const depth of mode === 'package' ? [1, 2] as const : [1] as const) {
    const sections = markdownHeadingSections(source, depth).map(({ heading, body }) => ({
      language: localesByHeading.get(normalizeHeading(heading)), body,
    }))
    if (!sections.some(section => section.language)) continue
    const matching = sections.filter(section => section.language?.locale === locale && section.body.trim())
    const named = mode === 'package' ? matching.find(section => section.language?.isCode === false) : undefined
    return (named ?? matching[0])?.body ?? (mode === 'package' ? source : '')
  }
  return source
}

export function updateNotesForLocale(
  update: Pick<AppUpdateInfo, 'body' | 'changelog'> | null,
  locale: Locale,
): { version?: string; notes: string }[] {
  if (!update) return []
  if (update.changelog?.length) {
    return update.changelog.map(({ version, notes }) => ({ version, notes: selectReleaseNotes(notes, locale) }))
  }
  return [{ notes: selectReleaseNotes(update.body ?? '', locale) }]
}
