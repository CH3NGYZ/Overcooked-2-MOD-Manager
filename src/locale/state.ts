import { ref, watch } from 'vue'
import { isLocale, type Locale } from './types.ts'

const LOCALE_KEY = 'oc2-locale'
const DEFAULT_LOCALE: Locale = 'zh-CN'

function storedLocale(): Locale | undefined {
  const value = globalThis.localStorage?.getItem(LOCALE_KEY)
  return isLocale(value) ? value : undefined
}

export const locale = ref<Locale>(storedLocale() ?? DEFAULT_LOCALE)

watch(locale, (value) => {
  if (typeof document !== 'undefined') document.documentElement.lang = value
}, { immediate: true, flush: 'sync' })

export function hasStoredLocale(): boolean {
  return storedLocale() !== undefined
}

export function setLocale(value: Locale): void {
  locale.value = value
  globalThis.localStorage?.setItem(LOCALE_KEY, value)
}

function systemLocale(tag: string): Locale | undefined {
  try {
    const { baseName, language, region, script } = new Intl.Locale(tag)
    if (isLocale(baseName)) return baseName
    if (language === 'zh') {
      return script === 'Hant' || (!script && ['TW', 'HK', 'MO'].includes(region ?? '')) ? 'zh-TW' : 'zh-CN'
    }
    if (language === 'es') return 'es-ES'
    if (language === 'pt') return 'pt-BR'
    return isLocale(language) ? language : undefined
  } catch {
    return undefined
  }
}

export function preselectLocale(installLanguage: string | null | undefined, preferredLanguages: readonly string[]): void {
  if (hasStoredLocale()) return
  const suggested = isLocale(installLanguage) ? installLanguage : preferredLanguages.map(systemLocale).find(Boolean)
  locale.value = suggested ?? DEFAULT_LOCALE
}
