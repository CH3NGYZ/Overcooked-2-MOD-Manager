import { isLocale, LOCALE_LABELS } from '../../src/locale/types.ts'
import { announcements, reloadAnnouncementPage } from '../../src/locale/announcements.ts'
import protocol from '../../src/announcements/protocol.json' with { type: 'json' }
import { localizeSnapshot, NOTICE_LIMITS, parseEdit, parseTranslationInput } from './feed.mjs'
import { assetResponse, error, json, readJson } from './http.mjs'
import { editSnapshot, readSnapshot } from './store.mjs'
import { translateNotice, translationStatus } from './translate.mjs'

export const PUBLIC_ORIGIN = protocol.origin
export const ADMIN_ORIGIN = 'https://oc2modmanager-notice-admin.ch3ng.top'
export const PUBLIC_API = protocol.path
export const ADMIN_API = '/api/admin/announcements'
const TRANSLATE_API = '/api/admin/translate'
const READ_METHODS = { Allow: 'GET, HEAD' }
const EDIT_METHODS = { Allow: 'GET, HEAD, POST' }

const localeModule = Object.freeze({
  body: [
    `export const LOCALE_LABELS = ${JSON.stringify(LOCALE_LABELS)}`,
    `export const NOTICE_LIMITS = ${JSON.stringify(NOTICE_LIMITS)}`,
    `export const PUBLIC_API = ${JSON.stringify(PUBLIC_API)}`,
    `export const NOTICE_COPY = ${JSON.stringify({ ...announcements, reload: reloadAnnouncementPage })}`,
  ].join('\n'),
  type: 'text/javascript; charset=utf-8',
})

function asset(path, assets, method) {
  return assetResponse(path === '/notice-locales.js' ? localeModule : assets.get(path), method)
}

function publicQuery(url) {
  if (!url.search) return { locale: 'en', fresh: false }
  const locale = url.searchParams.get('lang')
  if (!isLocale(locale)) return null
  const key = `?lang=${locale}`
  if (url.search === key) return { locale, fresh: false }
  if (url.search === `${key}&fresh=1`) return { locale, fresh: true }
  return null
}

export async function servePublic(request, env, assets) {
  const url = new URL(request.url)
  const head = request.method === 'HEAD'
  if (url.origin !== PUBLIC_ORIGIN || url.username || url.password) return error(404, head)
  if (url.pathname !== PUBLIC_API) return asset(url.pathname, assets, request.method)
  if (request.method !== 'GET' && !head) return error(405, false, READ_METHODS)
  const query = publicQuery(url)
  if (!query) return error(400, head)
  try {
    const snapshot = localizeSnapshot(await readSnapshot(env.DB), query.locale)
    return json(snapshot, { head, publicSnapshot: !query.fresh })
  } catch {
    return error(503, head)
  }
}

async function authorized(ctx, email) {
  if (!ctx?.access || typeof email !== 'string' || !email.trim()) return false
  try {
    const identity = await ctx.access.getIdentity()
    return typeof identity?.email === 'string' && identity.email.toLowerCase() === email.trim().toLowerCase()
  } catch {
    return false
  }
}

async function parseRequest(request) {
  const contentType = request.headers.get('Content-Type')?.split(';', 1)[0]?.trim().toLowerCase()
  if (contentType !== 'application/json') return null
  try {
    return await readJson(request, NOTICE_LIMITS.payloadBytes)
  } catch {
    return null
  }
}

export async function serveAdmin(request, env, ctx, assets) {
  const url = new URL(request.url)
  const head = request.method === 'HEAD'
  if (url.origin !== ADMIN_ORIGIN || url.username || url.password) return error(404, head)
  if (!await authorized(ctx, env.ADMIN_EMAIL)) return error(403, head)
  const translation = url.pathname === TRANSLATE_API
  if (url.pathname !== ADMIN_API && !translation) return asset(url.pathname, assets, request.method)
  if (url.search) return error(404, head)
  try {
    if (request.method === 'GET' || head) return json(translation ? translationStatus(env) : await readSnapshot(env.DB), { head })
    if (request.method !== 'POST') return error(405, false, EDIT_METHODS)
    if (request.headers.get('Origin') !== ADMIN_ORIGIN) return error(403)
    const value = await parseRequest(request)
    if (translation) {
      const input = parseTranslationInput(value)
      return input ? await translateNotice(env, input) : error(400)
    }
    const change = parseEdit(value)
    if (!change) return error(400)
    const result = await editSnapshot(env.DB, change)
    return result.snapshot ? json(result.snapshot) : error(result.status)
  } catch {
    return error(503, head)
  }
}

export function noticeAssets(html, stylesheet, common, pagePath, script) {
  return new Map([
    ['/', { body: html, type: 'text/html; charset=utf-8' }],
    ['/notice.css', { body: stylesheet, type: 'text/css; charset=utf-8' }],
    ['/notice-common.js', { body: common, type: 'text/javascript; charset=utf-8' }],
    [pagePath, { body: script, type: 'text/javascript; charset=utf-8' }],
  ])
}
