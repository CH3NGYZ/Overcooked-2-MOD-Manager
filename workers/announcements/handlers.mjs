import { LOCALE_LABELS } from '../../src/locale/types.ts'
import { announcements, reloadAnnouncementPage } from '../../src/locale/announcements.ts'
import protocol from '../../src/announcements/protocol.json' with { type: 'json' }
import { NOTICE_LIMITS, parseEdit } from './feed.mjs'
import { assetResponse, error, json } from './http.mjs'
import { editSnapshot, readSnapshot } from './store.mjs'

export const PUBLIC_ORIGIN = protocol.origin
export const ADMIN_ORIGIN = 'https://oc2modmanager-notice-admin.ch3ng.top'
export const PUBLIC_API = protocol.path
export const ADMIN_API = '/api/admin/announcements'
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

export async function servePublic(request, env, assets) {
  const url = new URL(request.url)
  const head = request.method === 'HEAD'
  if (url.origin !== PUBLIC_ORIGIN || url.username || url.password) return error(404, head)
  if (url.pathname !== PUBLIC_API) return asset(url.pathname, assets, request.method)
  if (url.search) return error(404, head)
  if (request.method !== 'GET' && !head) return error(405, false, READ_METHODS)
  try {
    return json(await readSnapshot(env.DB), { head, publicSnapshot: true })
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
  const declaredLength = request.headers.get('Content-Length')
  if (contentType !== 'application/json' || !request.body
      || (declaredLength !== null && (!/^\d+$/.test(declaredLength) || Number(declaredLength) > NOTICE_LIMITS.payloadBytes))) return null
  const reader = request.body.getReader()
  try {
    const decoder = new TextDecoder('utf-8', { fatal: true })
    let remaining = NOTICE_LIMITS.payloadBytes
    let text = ''
    for (;;) {
      const { done, value } = await reader.read()
      if (done) return parseEdit(JSON.parse(text + decoder.decode()))
      remaining -= value.byteLength
      if (remaining < 0) {
        await reader.cancel()
        return null
      }
      text += decoder.decode(value, { stream: true })
    }
  } catch {
    return null
  } finally {
    reader.releaseLock()
  }
}

export async function serveAdmin(request, env, ctx, assets) {
  const url = new URL(request.url)
  const head = request.method === 'HEAD'
  if (url.origin !== ADMIN_ORIGIN || url.username || url.password) return error(404, head)
  if (!await authorized(ctx, env.ADMIN_EMAIL)) return error(403, head)
  if (url.pathname !== ADMIN_API) return asset(url.pathname, assets, request.method)
  if (url.search) return error(404, head)
  try {
    if (request.method === 'GET' || head) return json(await readSnapshot(env.DB), { head })
    if (request.method !== 'POST') return error(405, false, EDIT_METHODS)
    if (request.headers.get('Origin') !== ADMIN_ORIGIN) return error(403)
    const change = await parseRequest(request)
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
