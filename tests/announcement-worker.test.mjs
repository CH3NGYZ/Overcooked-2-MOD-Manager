import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { DatabaseSync } from 'node:sqlite'
import { test } from 'node:test'
import { ADMIN_API, ADMIN_ORIGIN, noticeAssets, PUBLIC_API, PUBLIC_ORIGIN, serveAdmin, servePublic } from '../workers/announcements/handlers.mjs'
import { LOCALE_LABELS } from '../src/locale/types.ts'

const migration = readFileSync(new URL('../workers/announcements/migrations/0001_notice_feed.sql', import.meta.url), 'utf8')
const email = 'notice-admin@example.test'
const mime = ['application', 'json'].join('/')
const asset = '<h1>Announcements</h1>'
const publicAssets = noticeAssets(asset, 'body {}', 'export {}', '/notice-public.js', 'export {}')
const adminAssets = noticeAssets('<h1>Editor</h1>', 'body {}', 'export {}', '/notice-admin.js', 'export {}')
const ctx = { access: { getIdentity: async () => ({ email }) } }
const original = { language: 'zh-CN', title: '公告', body: '正文', translations: { en: { title: 'Notice', body: 'Body' } } }
const translatePath = '/api/admin/translate'
const translationEnv = {
  ADMIN_EMAIL: email,
  NOTICE_TRANSLATION_ENDPOINT: 'https://provider.example.test/v1/chat/completions',
  NOTICE_TRANSLATION_API_KEY: 'fixture-only-key',
}
const translationInput = { language: 'zh-CN', title: '公告', body: '第一行\nOC2 MOD Manager', targets: ['en', 'zh-TW'] }

function fixture(t) {
  const sqlite = new DatabaseSync(':memory:')
  sqlite.exec(migration)
  t.after(() => sqlite.close())
  let updates = 0
  return {
    env: { ADMIN_EMAIL: email, DB: { prepare(query) {
      const statement = sqlite.prepare(query)
      return {
        first: async () => statement.get() ?? null,
        bind: (...args) => ({ run: async () => {
          updates += 1
          return { success: true, meta: { changes: statement.run(...args).changes } }
        } }),
      }
    } } },
    sqlite,
    get updates() { return updates },
  }
}

function getPublic(env, path = PUBLIC_API, method = 'GET') {
  return servePublic(new Request(PUBLIC_ORIGIN + path, { method }), env, publicAssets)
}

function getAdmin(env, context = ctx, path = ADMIN_API) {
  return serveAdmin(new Request(ADMIN_ORIGIN + path), env, context, adminAssets)
}

function postAdmin(env, value, { origin = ADMIN_ORIGIN, destination = ADMIN_ORIGIN + ADMIN_API, context = ctx, headers = {}, raw } = {}) {
  return serveAdmin(new Request(destination, {
    method: 'POST', headers: { 'Content-Type': mime, ...(origin ? { Origin: origin } : {}), ...headers },
    body: raw ?? JSON.stringify(value),
  }), env, context, adminAssets)
}

const publish = (revision = 0, changes = {}) => ({ revision, action: 'publish', ...original, ...changes })

function postTranslation(env, value = translationInput, options = {}) {
  return postAdmin(env, value, { destination: ADMIN_ORIGIN + translatePath, ...options })
}

function completion(translations) {
  return Response.json({ choices: [{ message: { content: JSON.stringify({ translations }) }, finish_reason: 'stop' }] })
}

test('valid public snapshots alone receive the 300-second edge policy, including empty data', async t => {
  const { env, sqlite } = fixture(t)
  const response = await getPublic(env)
  assert.equal(response.status, 200)
  assert.equal(response.headers.get('Cache-Control'), 'no-store')
  assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), 'public, s-maxage=300')
  assert.match(response.headers.get('Content-Type'), /^application\/json\b/)
  assert.deepEqual(await response.json(), { revision: 0, announcements: [] })
  const head = await getPublic(env, PUBLIC_API, 'HEAD')
  assert.equal(head.status, 200)
  assert.equal(await head.text(), '')

  sqlite.exec("UPDATE notice_feed SET announcements = 'broken' WHERE singleton = 1")
  const corrupt = await getPublic(env)
  assert.equal(corrupt.status, 503)
  assert.deepEqual(await corrupt.json(), { error: 'unavailable' })
  assert.equal(corrupt.headers.get('Cache-Control'), 'no-store')
  assert.equal(corrupt.headers.get('Cloudflare-CDN-Cache-Control'), null)
})

test('public responses select one translation or the original without exposing the translation map', async t => {
  const { env } = fixture(t)
  const saved = await postAdmin(env, publish(0, {
    translations: { ...original.translations, de: { title: 'Hinweis', body: 'Meldung' } },
  }))
  assert.equal(saved.status, 200)
  const { id, published_at } = (await saved.json()).announcements[0]
  for (const [query, language, title, body] of [
    ['', 'en', 'Notice', 'Body'],
    ['?lang=en', 'en', 'Notice', 'Body'],
    ['?lang=de', 'de', 'Hinweis', 'Meldung'],
    ['?lang=zh-CN', 'zh-CN', '公告', '正文'],
    ['?lang=fr', 'zh-CN', '公告', '正文'],
  ]) {
    const response = await getPublic(env, PUBLIC_API + query)
    assert.equal(response.status, 200, query)
    assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), 'public, s-maxage=300', query)
    assert.deepEqual(await response.json(), { revision: 1, announcements: [{ id, published_at, language, title, body }] }, query)
  }
  const snapshot = await (await getAdmin(env)).json()
  assert.deepEqual(snapshot.announcements[0].translations, {
    en: { title: 'Notice', body: 'Body' }, de: { title: 'Hinweis', body: 'Meldung' },
  })
})

test('manual refresh reads the new revision in its own uncached URL after an ordinary feed read', async t => {
  const { env } = fixture(t)
  const ordinary = `${PUBLIC_API}?lang=en`
  const fresh = `${ordinary}&fresh=1`
  const cached = await getPublic(env, ordinary)
  assert.equal(cached.headers.get('Cloudflare-CDN-Cache-Control'), 'public, s-maxage=300')
  assert.deepEqual(await cached.json(), { revision: 0, announcements: [] })
  assert.equal((await postAdmin(env, publish())).status, 200)
  for (const method of ['GET', 'HEAD']) {
    const response = await getPublic(env, fresh, method)
    assert.equal(response.status, 200)
    assert.equal(response.headers.get('Cache-Control'), 'no-store')
    assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), null)
    if (method === 'HEAD') assert.equal(await response.text(), '')
    else {
      const snapshot = await response.json()
      assert.equal(snapshot.revision, 1)
      assert.equal(snapshot.announcements[0].title, 'Notice')
    }
  }
  const response = await getPublic(env, ordinary)
  assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), 'public, s-maxage=300')
})

test('only canonical locale and manual-refresh queries can reach the public feed', async () => {
  for (const query of ['?lang=', '?lang=EN', '?lang=en-US', '?lang=en&lang=de', '?lang=%65n', '?%6cang=en', '?lang=en&',
    '?lang=en&other=1', '?fresh=1', '?fresh=1&lang=en', '?lang=en&fresh=0', '?lang=en&fresh=1&fresh=1']) {
    const response = await getPublic({}, PUBLIC_API + query)
    assert.equal(response.status, 400, query)
    assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), null)
  }
})

test('origin, path and method constrain the public feed and static assets', async t => {
  const { env } = fixture(t)
  const page = await getPublic(env, '/')
  assert.equal(await page.text(), asset)
  assert.match(page.headers.get('Content-Security-Policy'), /frame-ancestors 'none'/)
  const localeScript = await getPublic(env, '/notice-locales.js')
  assert.match(await localeScript.text(), /export const LOCALE_LABELS.*"pt-BR"/)
  for (const response of [page, localeScript]) assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), null)
  for (const path of [`${PUBLIC_API}/`, ADMIN_API, translatePath, '/notice-admin.js']) {
    assert.equal((await getPublic(env, path)).status, 404)
  }
  assert.equal((await getPublic(env, PUBLIC_API, 'POST')).status, 405)
  assert.equal((await servePublic(new Request('https://unintended.workers.dev' + PUBLIC_API), env, publicAssets)).status, 404)
})

test('admin validates platform identity, public host, and same-origin writes', async t => {
  const { env } = fixture(t)
  for (const context of [{}, { access: { getIdentity: async () => ({ email: 'other@example.test' }) } },
    { access: { getIdentity: async () => { throw new Error('Access failure') } } }]) {
    assert.equal((await getAdmin(env, context, '/')).status, 403)
    assert.equal((await postAdmin(env, publish(), { context, headers: { 'Cf-Access-Authenticated-User-Email': email } })).status, 403)
  }
  assert.equal((await getAdmin({ ...env, ADMIN_EMAIL: '  ' })).status, 403)
  assert.equal((await getAdmin(env, ctx, '/notice-public.js')).status, 404)
  for (const origin of [null, 'https://another.example.test', 'null']) {
    assert.equal((await postAdmin(env, publish(), { origin })).status, 403)
  }
  assert.equal((await postAdmin(env, publish(), { destination: `https://unintended.workers.dev${ADMIN_API}` })).status, 404)
  const valid = await getAdmin(env)
  assert.equal(valid.status, 200)
  assert.deepEqual(await valid.json(), { revision: 0, announcements: [] })
  assert.equal(valid.headers.get('Cache-Control'), 'no-store')
  assert.equal(valid.headers.get('Cloudflare-CDN-Cache-Control'), null)
})

test('authenticated translation is opt-in and returns exactly the requested canonical locales', async t => {
  const translations = { en: { title: 'Notice', body: 'First line\nOC2 MOD Manager' }, 'zh-TW': { title: '公告', body: '第一行\nOC2 MOD Manager' } }
  const upstream = t.mock.method(globalThis, 'fetch', async (url, init) => {
    assert.equal(String(url), translationEnv.NOTICE_TRANSLATION_ENDPOINT)
    assert.equal(init.method, 'POST')
    assert.equal(init.redirect, 'manual')
    assert.equal(init.headers.Authorization, `Bearer ${translationEnv.NOTICE_TRANSLATION_API_KEY}`)
    assert.ok(init.signal instanceof AbortSignal)
    const payload = JSON.parse(init.body)
    assert.equal(payload.model, 'gpt-6.2-sol')
    assert.equal(payload.messages[0].role, 'system')
    assert.match(payload.messages[0].content, /en.*English/)
    assert.match(payload.messages[0].content, /zh-TW.*繁體中文/)
    assert.deepEqual(JSON.parse(payload.messages[0].content.split('\n').at(-1)), {
      translations: { en: { title: '', body: '' }, 'zh-TW': { title: '', body: '' } },
    })
    assert.equal(payload.messages[1].role, 'user')
    assert.deepEqual(JSON.parse(payload.messages[1].content), translationInput)
    return completion(translations)
  })
  const off = await getAdmin({ ADMIN_EMAIL: email }, ctx, translatePath)
  assert.equal(off.status, 200)
  assert.deepEqual(await off.json(), { enabled: false, model: 'gpt-6.2-sol', error: 'translation_unavailable' })
  const on = await getAdmin(translationEnv, ctx, translatePath)
  assert.equal(on.status, 200)
  assert.deepEqual(await on.json(), { enabled: true, model: 'gpt-6.2-sol' })
  assert.equal(on.headers.get('Cache-Control'), 'no-store')
  assert.equal(upstream.mock.callCount(), 0)
  const translated = await postTranslation(translationEnv)
  assert.equal(translated.status, 200)
  assert.equal(translated.headers.get('Cache-Control'), 'no-store')
  assert.equal(translated.headers.get('Cloudflare-CDN-Cache-Control'), null)
  assert.deepEqual(await translated.json(), { translations })
  assert.equal(upstream.mock.callCount(), 1)
})

test('all eleven missing languages are translated in one request with an exact JSON example', async context => {
  const targets = Object.keys(LOCALE_LABELS).filter(locale => locale !== translationInput.language)
  assert.equal(targets.length, 11)
  const input = { ...translationInput, targets }
  const translations = Object.fromEntries(targets.map(locale => [locale, { title: `Title (${locale})`, body: `Body (${locale})` }]))
  const model = 'provider-test-model'
  const upstream = context.mock.method(globalThis, 'fetch', async () => completion(translations))
  const response = await postTranslation({ ...translationEnv, NOTICE_TRANSLATION_MODEL: model }, input)
  assert.equal(response.status, 200)
  assert.equal(upstream.mock.callCount(), 1)
  const request = upstream.mock.calls[0].arguments[1]
  const payload = JSON.parse(request.body)
  assert.equal(payload.model, model)
  assert.deepEqual(JSON.parse(payload.messages[1].content), input)
  assert.deepEqual(JSON.parse(payload.messages[0].content.split('\n').at(-1)), {
    translations: Object.fromEntries(targets.map(locale => [locale, { title: '', body: '' }])),
  })
  assert.deepEqual(await response.json(), { translations })
})

test('invalid or unauthorized translation requests never contact the provider', async t => {
  const upstream = t.mock.method(globalThis, 'fetch', async () => {
    throw new Error('Unexpected upstream request')
  })
  for (const context of [{}, { access: { getIdentity: async () => ({ email: 'other@example.test' }) } }]) {
    assert.equal((await getAdmin(translationEnv, context, translatePath)).status, 403)
    assert.equal((await postTranslation(translationEnv, translationInput, { context, headers: { 'Cf-Access-Authenticated-User-Email': email } })).status, 403)
  }
  assert.equal((await postTranslation(translationEnv, translationInput, { origin: null })).status, 403)
  assert.equal((await getAdmin(translationEnv, ctx, `${translatePath}?lang=en`)).status, 404)
  assert.equal((await postTranslation(translationEnv, translationInput, { headers: { 'Content-Type': 'text/plain' } })).status, 400)
  assert.equal((await postTranslation(translationEnv, translationInput, { raw: ' '.repeat(131073) })).status, 400)
  for (const value of [null, { ...translationInput, model: 'another-model' }, { ...translationInput, language: 'zh' },
    { ...translationInput, body: '  ' }, { ...translationInput, title: '\u0000blocked' }, { ...translationInput, targets: [] },
    { ...translationInput, targets: ['zh-CN'] }, { ...translationInput, targets: ['en', 'en'] }, { ...translationInput, targets: ['pt-PT'] }]) {
    assert.equal((await postTranslation(translationEnv, value)).status, 400)
  }
  for (const env of [{ ADMIN_EMAIL: email }, { ...translationEnv, NOTICE_TRANSLATION_ENDPOINT: 'http://provider.example.test/' }]) {
    const response = await postTranslation(env)
    assert.equal(response.status, 503)
    assert.deepEqual(await response.json(), { error: 'translation_unavailable' })
  }
  assert.equal(upstream.mock.callCount(), 0)
})

test('OpenCode Go text inference is unavailable without using its coding agent plan or changing billing', async t => {
  const model = 'deepseek-v4.1-flash'
  const endpoint = 'https://opencode.ai/inference/go/openai/v1/chat/completions'
  const env = { ...translationEnv, NOTICE_TRANSLATION_ENDPOINT: endpoint, NOTICE_TRANSLATION_MODEL: model }
  const upstream = t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 403 }))
  const status = await getAdmin(env, ctx, translatePath)
  assert.equal(status.status, 200)
  assert.deepEqual(await status.json(), { enabled: false, model, error: 'translation_opencode_go_unsupported' })
  const failure = await postTranslation(env)
  assert.equal(failure.status, 503)
  assert.deepEqual(await failure.json(), { error: 'translation_opencode_go_unsupported' })
  assert.equal(upstream.mock.callCount(), 0)
  const paidEnv = { ...env, NOTICE_TRANSLATION_ENDPOINT: 'https://opencode.ai/inference/openai/v1/chat/completions' }
  assert.deepEqual(await (await getAdmin(paidEnv, ctx, translatePath)).json(), { enabled: true, model })
})

test('translation reports only the failure stage and upstream status, never provider details or incomplete translations', async t => {
  const valid = { en: { title: 'Notice', body: 'Body' }, 'zh-TW': { title: '標題', body: '正文' } }
  const warn = t.mock.method(console, 'warn', () => {})
  const providerStatuses = [400, 401, 403, 404, 429, 502, 504]
  const redirectStatuses = [302, 307, 308]
  const invalidResponses = [
    () => new Response('x'.repeat(131073)),
    () => Response.json({ choices: [{ message: { content: '```json\n{}\n```' } }] }),
    () => Response.json({ choices: [{ message: { content: 'response started' }, finish_reason: 'length' }] }),
    () => completion({ en: valid.en }),
    () => completion({ ...valid, 'zh-CN': valid['zh-TW'] }),
    () => completion({ en: { title: '', body: '' }, 'zh-TW': { title: '', body: '' } }),
    () => completion({ ...valid, en: { ...valid.en, body: 'a'.repeat(1537) } }),
    () => completion({ ...valid, en: { ...valid.en, extra: 'x' } }),
  ]
  const upstream = t.mock.method(globalThis, 'fetch')
  async function assertFailure(status, details) {
    const result = await postTranslation(translationEnv)
    assert.equal(result.status, status)
    assert.equal(result.headers.get('Cache-Control'), 'no-store')
    assert.equal(result.headers.get('Location'), null)
    assert.deepEqual(await result.json(), details)
    assert.deepEqual(warn.mock.calls.at(-1).arguments, ['notice_translation', details])
  }
  for (const status of providerStatuses) {
    upstream.mock.mockImplementation(async () => new Response('Provider-only detail', { status }))
    await assertFailure(502, { error: 'translation_upstream_http', upstream_status: status })
  }
  for (const status of redirectStatuses) {
    upstream.mock.mockImplementation(async () => Response.redirect('https://other.example.test/next', status))
    await assertFailure(502, { error: 'translation_upstream_http', upstream_status: status })
  }
  for (const makeResponse of invalidResponses) {
    upstream.mock.mockImplementation(async () => makeResponse())
    await assertFailure(502, { error: 'translation_invalid_result' })
  }
  upstream.mock.mockImplementation(async () => { throw new Error('Provider-only connection detail') })
  await assertFailure(502, { error: 'translation_connection' })
  const timeout = t.mock.method(AbortSignal, 'timeout', () => AbortSignal.abort())
  upstream.mock.mockImplementation(async (_endpoint, { signal }) => signal.throwIfAborted())
  await assertFailure(504, { error: 'translation_timeout' })
  assert.deepEqual(timeout.mock.calls[0].arguments, [80_000])
  assert.equal(upstream.mock.callCount(), providerStatuses.length + redirectStatuses.length + invalidResponses.length + 2)
  assert.equal(warn.mock.callCount(), upstream.mock.callCount())
})

test('malformed, oversized, invalid Unicode and partially translated content never changes D1', async t => {
  const state = fixture(t)
  for (const value of [null, [], publish(-1), publish(0, { title: '中'.repeat(54) }), publish(0, { body: 'a'.repeat(1537) }),
    publish(0, { title: '\ud800' }), publish(0, { title: 'Bad\nTitle' }), publish(0, { body: '\u0000text' }),
    publish(0, { language: 'pt-PT' }), publish(0, { id: 'forbidden' }), publish(0, { translations: { en: { title: 'Only title' } } }),
    publish(0, { translations: { 'zh-CN': { title: 'Wrong locale', body: 'Original duplicated' } } })]) {
    const result = await postAdmin(state.env, value)
    assert.equal(result.status, 400, JSON.stringify(value).slice(0, 100))
  }
  for (const raw of ['{', ' '.repeat(131073), new Uint8Array([255, 254])]) {
    assert.equal((await postAdmin(state.env, null, { raw })).status, 400)
  }
  assert.equal((await postAdmin(state.env, publish(), { headers: { 'Content-Type': 'text/plain' } })).status, 400)
  assert.equal(state.updates, 0)
  assert.deepEqual(await (await getAdmin(state.env)).json(), { revision: 0, announcements: [] })
})

test('only one concurrent editor commits; the stale editor receives 409', async t => {
  const state = fixture(t)
  const edits = await Promise.all([
    postAdmin(state.env, publish()),
    postAdmin(state.env, publish(0, { title: '同一修订号' })),
  ])
  assert.deepEqual(edits.map(result => result.status).sort(), [200, 409])
  assert.equal(state.updates, 2)
  const after = await (await getPublic(state.env)).json()
  assert.equal(after.revision, 1)
  assert.equal(after.announcements.length, 1)
  const latest = await postAdmin(state.env, publish(0))
  assert.equal(latest.status, 409)
  assert.deepEqual(await latest.json(), { error: 'conflict' })
})

test('publish keeps the five newest; edit preserves ID, publication time and order; withdraw removes the target', async t => {
  const { env } = fixture(t)
  let saved
  for (let index = 0; index < 6; index += 1) {
    const result = await postAdmin(env, publish(index, { title: ` 公告 ${index} `, body: '第一行\r\n第二行' }))
    assert.equal(result.status, 200)
    saved = await result.json()
  }
  assert.equal(saved.revision, 6)
  assert.deepEqual(saved.announcements.map(entry => entry.title), ['公告 5', '公告 4', '公告 3', '公告 2', '公告 1'])
  const before = saved.announcements[2]
  assert.equal(before.body, '第一行\n第二行')
  const edited = await postAdmin(env, { revision: 6, action: 'edit', id: before.id, ...original, title: '编辑公告' })
  assert.equal(edited.status, 200)
  const data = await edited.json()
  assert.equal(data.revision, 7)
  assert.deepEqual(data.announcements[2], { ...before, ...original, title: '编辑公告' })
  const removed = await postAdmin(env, { revision: 7, action: 'withdraw', id: before.id })
  assert.equal(removed.status, 200)
  const result = await removed.json()
  assert.equal(result.revision, 8)
  assert.equal(result.announcements.length, 4)
  assert.ok(!result.announcements.some(entry => entry.id === before.id))
  assert.equal((await postAdmin(env, { revision: 8, action: 'withdraw', id: before.id })).status, 404)
})
