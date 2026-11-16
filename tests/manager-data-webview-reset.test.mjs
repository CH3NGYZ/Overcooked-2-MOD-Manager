import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'

// The exact function embedded by Rust; storage and page origin are isolated
// fixtures. No browser profile or actual manager data is touched.
const script = readFileSync(new URL('../src-tauri/src/manager_data/webview_reset.js', import.meta.url), 'utf8')
const generator = readFileSync(new URL('../src-tauri/src/manager_data/webview_reset.rs', import.meta.url), 'utf8')
const templateLiteral = generator.match(/format!\(("(?:\\.|[^"\\])*?\{SCRIPT\}(?:\\.|[^"\\])*")\)/)?.[1]
assert.ok(templateLiteral, 'read the actual native script format, including its concatenation prefix')
const template = JSON.parse(templateLiteral)
const generatedScript = payload => template.replace('{SCRIPT}', script).replace('{payload}', JSON.stringify(payload))
const marker = 'oc2-manager-data-reset-consumed'
const token = '642902af-ef5b-46f7-b2a1-59cb367b80df'
function fixture(initial = {}, location = { protocol: 'http:', host: 'tauri.localhost' }) {
  const data = new Map(Object.entries(initial)), calls = [], warnings = []
  const storage = {
    get length() { return data.size },
    key: index => [...data.keys()][index] ?? null,
    getItem: key => data.get(key) ?? null,
    setItem: (key, value) => { calls.push(['set', key]); data.set(key, value) },
    removeItem: key => { calls.push(['remove', key]); data.delete(key) },
  }
  const window = { localStorage: storage }
  window.top = window
  const context = vm.createContext({ window, location, console: { warn: value => warnings.push(value) } })
  return { data, calls, warnings, storage, window, run: (overrides = {}, precedingScript = '') => vm.runInContext(
    precedingScript + generatedScript({ token, marker, protocol: 'http:', host: 'tauri.localhost', ...overrides }), context) }
}

test('the native production prefix safely appends to a bridge IIFE without a trailing semicolon', () => {
  assert.equal(template, ';\n({SCRIPT})({payload});')
  assert.match(generator, /include_str!\("webview_reset\.js"\)/)
  const f = fixture({ old: 'preference' })
  assert.doesNotThrow(() => f.run({}, '(() => { window.bridgeReady = true })()'))
  assert.equal(f.window.bridgeReady, true)
  assert.deepEqual([...f.data], [[marker, token]])
})

test('a new completed token removes all preferences and leaves its persistent consumption marker', () => {
  const f = fixture({ 'oc2-locale': 'en', 'oc2-theme-mode': 'dark', 'oc2-theme-accent': 'red',
    'oc2-tutorial': 'complete', 'oc2-map-publish-provider': 'gitee', 'oc2-ui-zoom': '1.75', other: 'old' })
  f.run()
  assert.deepEqual([...f.data], [[marker, token]])
  assert.equal(f.calls[0][0], 'set')
  assert.deepEqual(f.warnings, [])
})

test('the same token preserves new preferences across launches and each profile consumes independently', () => {
  const first = fixture({ old: 'setting' })
  first.run()
  first.storage.setItem('theme', 'new-theme')
  first.calls.length = 0
  first.run()
  assert.equal(first.data.get('theme'), 'new-theme')
  assert.deepEqual(first.calls, [])
  const second = fixture({ old: 'other-profile' })
  second.run()
  assert.deepEqual([...second.data], [[marker, token]])
  first.run({ token: 'c1383e30-6b45-4503-84c4-dfeacb1b9981' })
  assert.equal(first.data.size, 1)
  assert.equal(first.data.get(marker), 'c1383e30-6b45-4503-84c4-dfeacb1b9981')
})

test('only the exact configured top-level page origin is eligible', () => {
  for (const location of [
    { protocol: 'https:', host: 'gitee.com' }, { protocol: 'https:', host: 'gitcode.com' },
    { protocol: 'about:', host: '' }, { protocol: 'http:', host: 'tauri.localhost.evil.test' },
    { protocol: 'https:', host: 'tauri.localhost' }, { protocol: 'http:', host: 'tauri.localhost:5173' },
  ]) {
    const f = fixture({ keep: 'verification-state' }, location)
    f.run()
    assert.deepEqual([...f.data], [['keep', 'verification-state']])
    assert.deepEqual(f.calls, [])
  }
  const frame = fixture({ keep: 'frame-state' })
  frame.window.top = {}
  frame.run()
  assert.deepEqual(frame.calls, [])
  for (const location of [{ protocol: 'tauri:', host: 'localhost' }, { protocol: 'http:', host: 'localhost:5173' }]) {
    const f = fixture({ old: 'value' }, location)
    f.run(location)
    assert.deepEqual([...f.data], [[marker, token]])
  }
})

test('unavailable storage or failed consumption writes preserve preferences and never throw', () => {
  const denied = fixture({ keep: 'new-setting' })
  Object.defineProperty(denied.window, 'localStorage', { get() { throw new Error('denied') } })
  assert.doesNotThrow(() => denied.run())
  assert.deepEqual(denied.calls, [])
  const writeDenied = fixture({ keep: 'new-setting' })
  writeDenied.storage.setItem = () => { throw new Error('write denied') }
  writeDenied.run()
  writeDenied.run()
  assert.deepEqual([...writeDenied.data], [['keep', 'new-setting']])
  assert.deepEqual(writeDenied.calls, [])
  const discarded = fixture({ keep: 'new-setting' })
  discarded.storage.setItem = () => {}
  discarded.run()
  assert.deepEqual([...discarded.data], [['keep', 'new-setting']])
})

test('partial deletion failure cannot trigger another destructive pass on the next launch', () => {
  const f = fixture({ old: 'value' })
  const remove = f.storage.removeItem
  f.storage.removeItem = () => { throw new Error('delete denied') }
  f.run()
  assert.equal(f.data.get(marker), token)
  f.storage.removeItem = remove
  f.storage.setItem('theme', 'new-theme')
  f.calls.length = 0
  f.run()
  assert.deepEqual(f.calls, [])
  assert.equal(f.data.get('theme'), 'new-theme')
})

test('native startup injects the reset before page modules and keeps the session lock alive', () => {
  const native = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8')
  assert.match(native, /let _data_session = match manager_data::initialize\(\)/)
  assert.match(native, /let context = tauri::generate_context!\(\)/)
  assert.match(native, /manager_data::webview_reset_script\([\s\S]*?_data_session.completed_reset_token,[\s\S]*?context.config\(\)/)
  assert.match(native, /\.append_invoke_initialization_script\(preference_reset\)/)
  assert.match(native, /\.build\(context\)/)
  const domain = readFileSync(new URL('../src-tauri/src/manager_data.rs', import.meta.url), 'utf8')
  assert.match(domain, /\.filter\(\|record\| record.phase == storage::ResetPhase::Completed\)/)
  assert.match(domain, /skip_legacy_migration: true,\s*completed_reset_token: None/)
})
