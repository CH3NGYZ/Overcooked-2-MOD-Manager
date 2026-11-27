import { LOCALE_CODES, LOCALE_LABELS, type Locale } from './locale/types.ts'
import { markdownHeadingSections } from './markdown.ts'
import type { AppUpdateInfo } from './types.ts'

const languageAliases: Record<Locale, readonly string[]> = {
  en: ['英语', '英語'],
  de: ['German', '德语', '德語'],
  fr: ['French', '法语', '法語'],
  it: ['Italian', '意大利语', '義大利語', '義大利文'],
  ko: ['Korean', '한국말', '韩语', '韓語', '韓文'],
  'es-ES': ['es', 'Español', 'Español de España', 'Spanish', 'Spanish (Spain)', '西班牙语', '西班牙語'],
  'zh-CN': ['zh-Hans', 'Simplified Chinese', 'Chinese (Simplified)', '简体', '简中'],
  'zh-TW': ['zh-Hant', 'Traditional Chinese', 'Chinese (Traditional)', '繁體', '繁中'],
  ru: ['Russian', 'Русский язык', '俄语', '俄語'],
  ja: ['Japanese', '日语', '日語', '日文'],
  pl: ['Polish', '波兰语', '波蘭語'],
  'pt-BR': ['pt', 'Português', 'Português do Brasil', 'Portuguese', 'Brazilian Portuguese', 'Portuguese (Brazil)', '巴西葡萄牙语', '巴西葡萄牙語'],
}

const normalizeHeading = (heading: string) => heading.normalize('NFKC').trim().toLowerCase().replace(/\s+/g, ' ')
const localesByHeading = new Map(LOCALE_CODES.flatMap(locale =>
  [locale, LOCALE_LABELS[locale], ...languageAliases[locale]].map(heading => [normalizeHeading(heading), locale] as const),
))

/** 每个语言段独立渲染；引用式链接的定义也应写在同一语言段。 */
export function selectReleaseNotes(source: string, locale: Locale, mode: 'manager' | 'package' = 'manager'): string {
  if (!source.trim()) return mode === 'package' ? source : ''
  for (const depth of mode === 'package' ? [1, 2] as const : [1] as const) {
    const sections = markdownHeadingSections(source, depth).map(({ heading, body }) => ({
      locale: localesByHeading.get(normalizeHeading(heading)), body,
    }))
    if (sections.some(section => section.locale)) {
      return sections.find(section => section.locale === locale && section.body.trim())?.body
        ?? (mode === 'package' ? source : '')
    }
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
