import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { DatabaseSync } from 'node:sqlite'
import { test } from 'node:test'
import { ADMIN_API, ADMIN_ORIGIN, noticeAssets, PUBLIC_API, PUBLIC_ORIGIN, serveAdmin, servePublic } from '../workers/announcements/handlers.mjs'

const migration = readFileSync(new URL('../workers/announcements/migrations/0001_notice_feed.sql', import.meta.url), 'utf8')
const email = 'notice-admin@example.test'
const mime = ['application', 'json'].join('/')
const asset = '<h1>Announcements</h1>'
const publicAssets = noticeAssets(asset, 'body {}', 'export {}', '/notice-public.js', 'export {}')
const adminAssets = noticeAssets('<h1>Editor</h1>', 'body {}', 'export {}', '/notice-admin.js', 'export {}')
const ctx = { access: { getIdentity: async () => ({ email }) } }
const original = { language: 'zh-CN', title: '公告', body: '正文', translations: { en: { title: 'Notice', body: 'Body' } } }

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

test('origin, path and method constrain the public feed and static assets', async t => {
  const { env } = fixture(t)
  const page = await getPublic(env, '/')
  assert.equal(await page.text(), asset)
  assert.match(page.headers.get('Content-Security-Policy'), /frame-ancestors 'none'/)
  const localeScript = await getPublic(env, '/notice-locales.js')
  assert.match(await localeScript.text(), /export const LOCALE_LABELS.*"pt-BR"/)
  for (const response of [page, localeScript]) assert.equal(response.headers.get('Cloudflare-CDN-Cache-Control'), null)
  for (const path of [`${PUBLIC_API}?lang=en`, `${PUBLIC_API}/`, ADMIN_API, '/notice-admin.js']) {
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
