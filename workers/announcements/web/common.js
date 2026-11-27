import { LOCALE_LABELS, NOTICE_LIMITS } from '/notice-locales.js'

export { LOCALE_LABELS, NOTICE_LIMITS }

export const byId = id => document.getElementById(id)

export function noticeDate(value, locale) {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

export function renderNotice(article, announcement, locale) {
  const { title, body, language } = announcement
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
  constructor(status, details) {
    super(`Announcement request failed: HTTP ${status}`)
    this.status = status
    this.code = details?.error
    this.upstreamStatus = details?.upstream_status
  }
}

export async function requestNoticeJson(path, mutation, { timeout = 10_000, signal } = {}) {
  const deadline = AbortSignal.timeout(timeout)
  const response = await fetch(path, {
    redirect: 'error',
    signal: signal ? AbortSignal.any([signal, deadline]) : deadline,
    ...(mutation && { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(mutation) }),
  })
  if (response.status !== 200) {
    const details = response.headers.get('Content-Type')?.startsWith('application/json')
      ? await response.json().catch(() => null) : null
    throw new NoticeRequestError(response.status, details)
  }
  return response.json()
}

export async function requestSnapshot(path, mutation) {
  const snapshot = await requestNoticeJson(path, mutation)
  if (!Number.isSafeInteger(snapshot?.revision) || snapshot.revision < 0 || !Array.isArray(snapshot.announcements)
    || snapshot.announcements.length > NOTICE_LIMITS.maxAnnouncements) {
    throw new Error('Invalid announcement response')
  }
  return snapshot
}
