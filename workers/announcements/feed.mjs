import { isLocale } from '../../src/locale/types.ts'
import protocol from '../../src/announcements/protocol.json' with { type: 'json' }

export const NOTICE_LIMITS = Object.freeze({
  titleBytes: protocol.maxTitleBytes,
  bodyBytes: protocol.maxBodyBytes,
  maxAnnouncements: protocol.maxItems,
  payloadBytes: protocol.maxResponseBytes,
})

const encoder = new TextEncoder()
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/
const TITLE_CONTROLS = /[\u0000-\u001f\u007f-\u009f]/u
const BODY_CONTROLS = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/u

export const byteLength = value => encoder.encode(value).byteLength
export const isRecord = value => value !== null && typeof value === 'object' && !Array.isArray(value)
export const isRevision = value => Number.isSafeInteger(value) && value >= 0
const hasOnlyKeys = (record, allowed) => Object.keys(record).every(key => allowed.includes(key))

function text(value, maxBytes, controls) {
  if (typeof value !== 'string' || !value.isWellFormed() || controls.test(value) || byteLength(value) > maxBytes) return null
  return value.trim() || null
}

function content(value) {
  const title = text(value.title, NOTICE_LIMITS.titleBytes, TITLE_CONTROLS)
  const body = text(typeof value.body === 'string' ? value.body.replaceAll('\r\n', '\n') : value.body, NOTICE_LIMITS.bodyBytes, BODY_CONTROLS)
  return title && body ? { title, body } : null
}

export function parseTranslationInput(value) {
  if (!isRecord(value) || !hasOnlyKeys(value, ['language', 'title', 'body', 'targets'])) return null
  const { language, targets } = value
  if (!isLocale(language) || !Array.isArray(targets) || !targets.length
    || targets.some(target => !isLocale(target) || target === language) || new Set(targets).size !== targets.length) return null
  const original = content(value)
  return original ? { language, ...original, targets } : null
}

export function parseTranslationResult(value, targets) {
  if (!isRecord(value) || !hasOnlyKeys(value, ['translations'])) return null
  const { translations } = value
  if (!isRecord(translations) || Object.keys(translations).length !== targets.length || !hasOnlyKeys(translations, targets)) return null
  const normalized = {}
  for (const target of targets) {
    const entry = translations[target]
    if (!isRecord(entry) || !hasOnlyKeys(entry, ['title', 'body'])) return null
    const translated = content(entry)
    if (!translated) return null
    normalized[target] = translated
  }
  return { translations: normalized }
}

function draft(value) {
  if (!isLocale(value.language) || !isRecord(value.translations)) return null
  const original = content(value)
  if (!original) return null
  const translations = {}
  for (const [language, entry] of Object.entries(value.translations)) {
    if (!isLocale(language) || language === value.language || !isRecord(entry) || !hasOnlyKeys(entry, ['title', 'body'])) return null
    const translation = content(entry)
    if (!translation) return null
    translations[language] = translation
  }
  return { language: value.language, ...original, translations }
}

export function parseEdit(value) {
  if (!isRecord(value) || !isRevision(value.revision)) return null
  const { revision, action, id } = value
  if (action === 'withdraw') {
    return typeof id === 'string' && UUID.test(id) && hasOnlyKeys(value, ['revision', 'action', 'id'])
      ? { revision, action, id } : null
  }
  if (action !== 'publish' && action !== 'edit') return null
  if (action === 'edit' ? typeof id !== 'string' || !UUID.test(id) : Object.hasOwn(value, 'id')) return null
  if (!hasOnlyKeys(value, ['revision', 'action', 'id', 'language', 'title', 'body', 'translations'])) return null
  const normalized = draft(value)
  return normalized ? { revision, action, id, draft: normalized } : null
}

export function parseSnapshot(revision, stored) {
  if (!isRevision(revision) || typeof stored !== 'string' || byteLength(stored) > NOTICE_LIMITS.payloadBytes) return null
  const parsed = JSON.parse(stored)
  if (!Array.isArray(parsed) || parsed.length > NOTICE_LIMITS.maxAnnouncements) return null
  const announcements = []
  const seen = new Set()
  let previous = Infinity
  for (const value of parsed) {
    if (!isRecord(value) || !hasOnlyKeys(value, ['id', 'published_at', 'language', 'title', 'body', 'translations'])) return null
    const { id, published_at } = value
    const timestamp = typeof published_at === 'string' ? Date.parse(published_at) : NaN
    if (typeof id !== 'string' || !UUID.test(id) || seen.has(id) || !Number.isFinite(timestamp)
      || new Date(timestamp).toISOString() !== published_at || timestamp > previous) return null
    const normalized = draft(value)
    if (!normalized) return null
    seen.add(id)
    previous = timestamp
    announcements.push({ id, published_at, ...normalized })
  }
  const snapshot = { revision, announcements }
  return byteLength(JSON.stringify(snapshot)) <= NOTICE_LIMITS.payloadBytes ? snapshot : null
}

export function localizeSnapshot({ revision, announcements }, locale) {
  return { revision, announcements: announcements.map(({ id, published_at, language, title, body, translations }) => {
    const translation = translations[locale]
    return translation
      ? { id, published_at, language: locale, title: translation.title, body: translation.body }
      : { id, published_at, language, title, body }
  }) }
}
