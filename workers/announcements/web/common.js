import { LOCALE_LABELS, NOTICE_LIMITS } from '/notice-locales.js'

export { LOCALE_LABELS, NOTICE_LIMITS }

export const byId = id => document.getElementById(id)

export function localizedNotice(announcement, locale) {
  const translation = Object.hasOwn(announcement.translations, locale) && announcement.translations[locale]
  return translation ? { ...translation, language: locale } : announcement
}

export function noticeDate(value, locale) {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

export function renderNotice(article, announcement, locale) {
  const { title, body, language } = localizedNotice(announcement, locale)
  const heading = document.createElement('h2')
  heading.textContent = title
  heading.lang = language
  const content = document.createElement('p')
  content.textContent = body
  content.lang = language
  if (announcement.published_at) {
    const date = document.createElement('time')
    date.dateTime = announcement.published_at
    date.textContent = noticeDate(announcement.published_at, locale)
    article.replaceChildren(heading, date, content)
  } else {
    article.replaceChildren(heading, content)
  }
}

export class NoticeRequestError extends Error {
  constructor(status) {
    super(`Announcement request failed: HTTP ${status}`)
    this.status = status
  }
}

export async function requestSnapshot(path, mutation) {
  const response = await fetch(path, {
    redirect: 'error',
    signal: AbortSignal.timeout(10_000),
    ...(mutation && { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(mutation) }),
  })
  if (response.status !== 200) throw new NoticeRequestError(response.status)
  const snapshot = await response.json()
  if (!Number.isSafeInteger(snapshot?.revision) || snapshot.revision < 0 || !Array.isArray(snapshot.announcements)
    || snapshot.announcements.length > NOTICE_LIMITS.maxAnnouncements) {
    throw new Error('Invalid announcement response')
  }
  return snapshot
}
