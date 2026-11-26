import { byId, LOCALE_LABELS, renderNotice, requestSnapshot } from '/notice-common.js'
import { NOTICE_COPY, PUBLIC_API } from '/notice-locales.js'

function pageLocale() {
  for (const language of navigator.languages) {
    try {
      const locale = new Intl.Locale(language)
      if (Object.hasOwn(LOCALE_LABELS, locale.baseName)) return locale.baseName
      if (locale.language === 'zh') return locale.maximize().script === 'Hant' ? 'zh-TW' : 'zh-CN'
      const related = Object.keys(LOCALE_LABELS).find(code => code.split('-')[0] === locale.language)
      if (related) return related
    } catch {
      continue
    }
  }
  return 'en'
}

const locale = pageLocale()
const heading = NOTICE_COPY.title[locale]
const status = byId('status')
document.documentElement.lang = locale
document.title = `${heading} · OC2 MOD Manager`
byId('page-title').textContent = heading
status.textContent = NOTICE_COPY.loading[locale]

try {
  const { announcements } = await requestSnapshot(PUBLIC_API)
  const articles = announcements.map(announcement => {
    const article = document.createElement('article')
    article.className = 'notice'
    renderNotice(article, announcement, locale)
    return article
  })
  byId('notices').replaceChildren(...articles)
  status.hidden = articles.length > 0
  status.textContent = articles.length ? '' : NOTICE_COPY.empty[locale]
} catch {
  status.dataset.tone = 'error'
  status.textContent = `${NOTICE_COPY.unavailable[locale]} ${NOTICE_COPY.reload[locale]}`
}
