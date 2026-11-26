import assert from 'node:assert/strict'
import { test } from 'node:test'
import protocol from '../src/announcements/protocol.json' with { type: 'json' }

let instance = 0
const freshState = () => import(`../src/announcements/state.ts?app=${++instance}`)
const idOf = suffix => `bfbed70d-3958-4959-9772-${String(suffix).padStart(12, '0')}`
const snapshot = (revision, ...ids) => ({ revision, announcements: ids.map(id => ({
  id: idOf(id), published_at: '2026-11-25T12:00:00.000Z',
  language: 'zh-CN', title: '公告', body: '这段正文不应落盘。',
})) })

function useStorage(context, options = {}) {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
  const entries = new Map()
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: {
    getItem: key => options.readError ? options.readError() : options.readValue ?? entries.get(key) ?? null,
    setItem: (key, value) => options.writeError ? options.writeError() : entries.set(key, value),
  } })
  context.after(() => {
    if (original) Object.defineProperty(globalThis, 'localStorage', original)
    else delete globalThis.localStorage
  })
  return entries
}

test('starting a language load shares pending work and does not acknowledge the result', async context => {
  const storage = useStorage(context)
  const { loadAnnouncements, announcementSnapshot, announcementsUnread, markAnnouncementsRead } = await freshState()
  const { promise, resolve } = Promise.withResolvers()
  const requests = []
  const start = loadAnnouncements('zh-CN', (language, fresh) => { requests.push([language, fresh]); return promise })
  assert.equal(loadAnnouncements('zh-CN', () => assert.fail('unexpected extra fetch')), start)
  await Promise.resolve()
  assert.deepEqual(requests, [['zh-CN', false]])
  assert.equal(announcementSnapshot.value, undefined)
  assert.equal(announcementsUnread.value, false)
  markAnnouncementsRead()
  assert.equal(storage.size, 0)
  resolve(snapshot(1, 1, 2))
  await start
  assert.equal(announcementsUnread.value, true)
  assert.equal(storage.size, 0)
  markAnnouncementsRead()
  assert.equal(announcementsUnread.value, false)
  assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(1), idOf(2)]])
})

test('manual reload uses the current language, supersedes a cached load, and only reads once per click burst', async context => {
  useStorage(context)
  const { loadAnnouncements, announcementSnapshot } = await freshState()
  const cached = Promise.withResolvers()
  const freshRead = Promise.withResolvers()
  const calls = []
  const load = (language, fresh) => {
    calls.push([language, fresh])
    return fresh ? freshRead.promise : cached.promise
  }
  const start = loadAnnouncements('ja', load)
  const refresh = loadAnnouncements('ja', load, true)
  assert.notEqual(refresh, start)
  assert.equal(loadAnnouncements('ja', load, true), refresh)
  assert.equal(loadAnnouncements('ja', load), refresh)
  freshRead.resolve(snapshot(3, 3))
  await refresh
  cached.resolve(snapshot(2, 2))
  await start
  assert.deepEqual(calls, [['ja', false], ['ja', true]])
  assert.equal(announcementSnapshot.value.revision, 3)
  await loadAnnouncements('ja', async (language, fresh) => {
    assert.deepEqual([language, fresh], ['ja', true])
    return snapshot(4, 4)
  }, true)
  assert.equal(announcementSnapshot.value.revision, 4)
})

test('only the selected language may replace the snapshot even when requests resolve out of order', async context => {
  useStorage(context)
  const { loadAnnouncements, announcementSnapshot } = await freshState()
  await loadAnnouncements('zh-CN', async () => snapshot(1, 1))
  const stale = Promise.withResolvers()
  const other = loadAnnouncements('zh-CN', () => stale.promise, true)
  const failed = loadAnnouncements('pt-BR', async () => null)
  assert.equal(announcementSnapshot.value, undefined)
  await failed
  stale.resolve(snapshot(2, 1))
  await other
  assert.equal(announcementSnapshot.value, null, 'failed locale must not show another locale response')

  const french = Promise.withResolvers()
  const german = Promise.withResolvers()
  const toFrench = loadAnnouncements('fr', () => french.promise)
  const toGerman = loadAnnouncements('de', () => german.promise)
  assert.equal(loadAnnouncements('fr', () => assert.fail('already pending in French')), toFrench)
  german.reject(new Error('delayed failure'))
  await toGerman
  assert.equal(announcementSnapshot.value, undefined)
  french.resolve(snapshot(3, 3))
  await toFrench
  assert.equal(announcementSnapshot.value.revision, 3)
})

test('new IDs alert after restart; editing the same ID does not', async context => {
  const storage = useStorage(context)
  const before = await freshState()
  await before.loadAnnouncements('zh-CN', async () => snapshot(1, 1, 2))
  before.markAnnouncementsRead()

  const after = await freshState()
  const edited = snapshot(2, 1, 2)
  edited.announcements[0].title = '已修改'
  await after.loadAnnouncements('en', async () => edited)
  assert.equal(after.announcementsUnread.value, false)

  const next = await freshState()
  await next.loadAnnouncements('en', async () => snapshot(3, 3, 1, 2))
  assert.equal(next.announcementsUnread.value, true)
  assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(1), idOf(2)]])
  next.markAnnouncementsRead()
  assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(3), idOf(1), idOf(2)]])
})

test('empty, failed, and rejected reads cannot overwrite read IDs or show old content', async context => {
  const storage = useStorage(context)
  const previous = await freshState()
  await previous.loadAnnouncements('zh-CN', async () => snapshot(1, 1))
  previous.markAnnouncementsRead()

  for (const [load, expected] of [
    [async () => snapshot(2), snapshot(2)],
    [async () => null, null],
    [async () => { throw new Error('IPC unavailable') }, null],
  ]) {
    const state = await freshState()
    await state.loadAnnouncements('en', load)
    assert.deepEqual(state.announcementSnapshot.value, expected)
    assert.equal(state.announcementsUnread.value, false)
    state.markAnnouncementsRead()
    assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(1)]])
  }
})

test('malformed or inaccessible storage cannot prevent acknowledgements in the window', async context => {
  const storage = useStorage(context, { readValue: '{', writeError: () => { throw new Error('denied') } })
  const state = await freshState()
  await state.loadAnnouncements('zh-CN', async () => snapshot(1, 1))
  assert.equal(state.announcementsUnread.value, true)
  state.markAnnouncementsRead()
  assert.equal(state.announcementsUnread.value, false)
  assert.equal(storage.size, 0)
})

test('read ID history stays bounded and keeps the most recent acknowledgements', async context => {
  const storage = useStorage(context)
  const historySize = protocol.maxItems * 4
  for (let suffix = 1; suffix <= historySize + 1; suffix += 1) {
    const state = await freshState()
    await state.loadAnnouncements('zh-CN', async () => snapshot(suffix, suffix))
    state.markAnnouncementsRead()
  }
  assert.deepEqual([...storage.values()].map(JSON.parse), [Array.from({ length: historySize }, (_, index) => idOf(historySize + 1 - index))])
})
