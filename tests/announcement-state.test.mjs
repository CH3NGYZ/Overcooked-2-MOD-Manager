import assert from 'node:assert/strict'
import { test } from 'node:test'
import protocol from '../src/announcements/protocol.json' with { type: 'json' }

let instance = 0
const freshState = () => import(`../src/announcements/state.ts?app=${++instance}`)
const idOf = suffix => `bfbed70d-3958-4959-9772-${String(suffix).padStart(12, '0')}`
const snapshot = (revision, ...ids) => ({ revision, announcements: ids.map(id => ({
  id: idOf(id), published_at: '2026-11-25T12:00:00.000Z',
  language: 'zh-CN', title: '公告', body: '这段正文不应落盘。', translations: {},
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

test('initialization starts only one fetch and does not acknowledge the result', async context => {
  const storage = useStorage(context)
  const { initializeAnnouncements, announcementSnapshot, announcementsUnread, markAnnouncementsRead } = await freshState()
  let requests = 0
  let finish
  const pending = new Promise(resolve => { finish = resolve })
  const start = initializeAnnouncements(() => { requests += 1; return pending })
  assert.equal(initializeAnnouncements(() => assert.fail('unexpected extra fetch')), start)
  await Promise.resolve()
  assert.equal(requests, 1)
  assert.equal(announcementSnapshot.value, undefined)
  assert.equal(announcementsUnread.value, false)
  markAnnouncementsRead()
  assert.equal(storage.size, 0)
  finish(snapshot(1, 1, 2))
  await start
  assert.equal(announcementsUnread.value, true)
  assert.equal(storage.size, 0)
  markAnnouncementsRead()
  assert.equal(announcementsUnread.value, false)
  assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(1), idOf(2)]])
})

test('new IDs alert after restart; editing the same ID does not', async context => {
  const storage = useStorage(context)
  const before = await freshState()
  await before.initializeAnnouncements(async () => snapshot(1, 1, 2))
  before.markAnnouncementsRead()

  const after = await freshState()
  const edited = snapshot(2, 1, 2)
  edited.announcements[0].title = '已修改'
  await after.initializeAnnouncements(async () => edited)
  assert.equal(after.announcementsUnread.value, false)

  const next = await freshState()
  await next.initializeAnnouncements(async () => snapshot(3, 3, 1, 2))
  assert.equal(next.announcementsUnread.value, true)
  assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(1), idOf(2)]])
  next.markAnnouncementsRead()
  assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(3), idOf(1), idOf(2)]])
})

test('empty, failed, and rejected reads cannot overwrite read IDs or show old content', async context => {
  const storage = useStorage(context)
  const previous = await freshState()
  await previous.initializeAnnouncements(async () => snapshot(1, 1))
  previous.markAnnouncementsRead()

  for (const [load, expected] of [
    [async () => snapshot(2), snapshot(2)],
    [async () => null, null],
    [async () => { throw new Error('IPC unavailable') }, null],
  ]) {
    const state = await freshState()
    await state.initializeAnnouncements(load)
    assert.deepEqual(state.announcementSnapshot.value, expected)
    assert.equal(state.announcementsUnread.value, false)
    state.markAnnouncementsRead()
    assert.deepEqual([...storage.values()].map(JSON.parse), [[idOf(1)]])
  }
})

test('malformed or inaccessible storage cannot prevent acknowledgements in the window', async context => {
  const storage = useStorage(context, { readValue: '{', writeError: () => { throw new Error('denied') } })
  const state = await freshState()
  await state.initializeAnnouncements(async () => snapshot(1, 1))
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
    await state.initializeAnnouncements(async () => snapshot(suffix, suffix))
    state.markAnnouncementsRead()
  }
  assert.deepEqual([...storage.values()].map(JSON.parse), [Array.from({ length: historySize }, (_, index) => idOf(historySize + 1 - index))])
})
