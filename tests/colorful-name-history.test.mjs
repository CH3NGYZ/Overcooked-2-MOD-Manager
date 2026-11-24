import assert from 'node:assert/strict'
import test from 'node:test'
import { createNameHistory, NAME_HISTORY_LIMIT } from '../src/rename/history.ts'
import { DEFAULT_COLOR, DEFAULT_SIZE, buildShortestRichText, styleName } from '../src/colorfulName.ts'

function draft(name, color = DEFAULT_COLOR, size = DEFAULT_SIZE) {
  const chars = [...name]
  return { name, colors: chars.map(() => color), sizes: chars.map(() => size) }
}

function fixture(seed) {
  let raw = seed
  const storage = { getItem: () => raw ?? null, setItem: (_, value) => { raw = value }, removeItem: () => { raw = undefined } }
  return { history: createNameHistory(() => storage), reopen: () => createNameHistory(() => storage) }
}

const success = async () => ({ ok: true })
const names = history => history.entries.value.map(item => item.name)

test('history records the exact successful snapshot, not edits during a pending operation', async () => {
  const { history, reopen } = fixture()
  const original = draft('厨👩‍🍳A', '#FFA339', 42.5)
  const expected = buildShortestRichText(styleName(original.name, original.colors, original.sizes))
  let finish
  const inflight = history.submit('copy', original, code => {
    assert.equal(code, expected)
    return new Promise(resolve => { finish = resolve })
  })
  assert.deepEqual(names(history), [])
  assert.equal(history.busy.value, 'copy')
  original.name = 'Next draft'
  original.colors[0] = '#ff0000'
  assert.equal(await history.submit('push', original, () => assert.fail('concurrent operation sent')), undefined)
  finish({ ok: true })
  await inflight
  assert.deepEqual(history.entries.value, [{ ...draft('厨👩‍🍳A', '#ffa339', 42.5), key: history.entries.value[0].key }])
  assert.deepEqual(reopen().entries.value, history.entries.value)
  assert.equal(history.busy.value, undefined)
})

test('failed copy/push, rejected IPC, empty input and over-budget rich text do not add history', async () => {
  const { history } = fixture()
  for (const action of ['copy', 'push']) {
    assert.deepEqual(await history.submit(action, draft('厨'), async () => ({ ok: false })), { ok: false })
    await assert.rejects(history.submit(action, draft('厨'), async () => { throw new Error('IPC failed') }), /IPC failed/)
    assert.equal(history.busy.value, undefined)
  }
  await history.submit('copy', draft('  '), () => assert.fail('empty name sent'))
  await history.submit('copy', draft('A'.repeat(119), '#123456'), () => assert.fail('overlength code sent'))
  await history.submit('copy', draft('🧑'.repeat(60)), () => assert.fail('UTF-16 ceiling ignored'))
  assert.deepEqual(names(history), [])
})

test('recently successful matching styles move first, retaining distinct styles and at most 20 records', async () => {
  const { history, reopen } = fixture()
  for (let index = 0; index <= NAME_HISTORY_LIMIT; index++) await history.submit('copy', draft(`Chef ${index}`), success)
  assert.equal(history.entries.value.length, 20)
  assert.equal(names(history).at(-1), 'Chef 1')
  await history.submit('push', draft('Chef 3', '#5ba3dc'), success)
  await history.submit('copy', draft('Chef 3'), success)
  assert.deepEqual(names(history).slice(0, 3), ['Chef 3', 'Chef 3', 'Chef 20'])
  assert.deepEqual(history.entries.value.slice(0, 2).map(entry => entry.colors[0]), ['#000000', '#5ba3dc'])
  assert.equal(history.entries.value.length, 20)
  assert.deepEqual(reopen().entries.value, history.entries.value)
})

test('deleting a row or clearing history cannot be undone by a late success', async () => {
  const { history, reopen } = fixture()
  const first = draft('Chef')
  await history.submit('copy', draft('Other'), success)
  await history.submit('copy', first, success)
  let finish
  const push = history.submit('push', first, () => new Promise(resolve => { finish = resolve }))
  history.remove(history.entries.value[0].key)
  finish({ ok: true })
  await push
  assert.deepEqual(names(reopen()), ['Other'])
  const copy = history.submit('copy', draft('New'), () => new Promise(resolve => { finish = resolve }))
  history.clear()
  finish({ ok: true })
  await copy
  assert.deepEqual(names(reopen()), [])
  await history.submit('copy', first, success)
  assert.deepEqual(names(reopen()), ['Chef'])
})

test('storage rejects malformed data, retains literal HTML as text and normalizes color case', () => {
  const html = draft('<img>文字🍔', '#FFa339', 19.5)
  const seed = JSON.stringify([null, {}, { ...html, colors: ['red'] }, draft('A', 'red'), draft('B', '#123456', 0),
    { ...draft('wrong length'), sizes: [] }, draft('💥'.repeat(60)), draft('x'.repeat(119), '#123456'),
    html, draft(html.name, '#ffa339', 19.5), ...Array.from({ length: 21 }, (_, index) => draft(`Chef ${index}`))])
  const { history } = fixture(seed)
  assert.equal(history.entries.value.length, 20)
  assert.equal(history.entries.value[0].name, html.name)
  assert.equal(history.entries.value[0].colors[0], '#ffa339')
  assert.equal(history.entries.value[0].sizes[0], 19.5)
  assert.equal(history.entries.value[1].name, 'Chef 0')
  assert.deepEqual(names(fixture('invalid JSON').history), [])
})

test('blocked local storage does not turn a successful copy into an error', async () => {
  const history = createNameHistory(() => { throw new Error('storage disabled') })
  assert.deepEqual(await history.submit('copy', draft('Chef'), success), { ok: true })
  assert.deepEqual(names(history), ['Chef'])
  history.clear()
  assert.deepEqual(names(history), [])
})
