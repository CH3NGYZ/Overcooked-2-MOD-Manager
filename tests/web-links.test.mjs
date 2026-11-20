import assert from 'node:assert/strict'
import test from 'node:test'
import { resolveWebLink } from '../src/webLinks.ts'

const app = new URL('http://tauri.localhost/#/mods-online')

test('main-window routes stay local and document anchors do not replace the Vue hash route', () => {
  assert.deepEqual(resolveWebLink('#/settings', app), { kind: 'internal' })
  assert.deepEqual(resolveWebLink('#quick-start', app), { kind: 'anchor', hash: 'quick-start' })
  assert.deepEqual(resolveWebLink('http://tauri.localhost/#/tools', app), { kind: 'internal' })
  const mac = new URL('tauri://localhost/#/mods-online')
  assert.deepEqual(resolveWebLink('#/tools', mac), { kind: 'internal' })
  assert.deepEqual(resolveWebLink('#installation', mac), { kind: 'anchor', hash: 'installation' })
  assert.deepEqual(resolveWebLink('tauri://other/#/tools', mac), { kind: 'blocked' })
})

test('standalone browser links preserve path, query and fragment and use the full authority', () => {
  for (const url of ['https://example.com/%E8%AF%B4%E6%98%8E?a=20%26b#chapter', 'http://tauri.localhost:1234/help']) {
    assert.deepEqual(resolveWebLink(url, app), { kind: 'external', url })
  }
  assert.deepEqual(resolveWebLink('//example.com/tutorial', app), { kind: 'external', url: 'http://example.com/tutorial' })
})

test('file URLs, custom schemes, malformed URLs and credentials are blocked', () => {
  for (const url of ['file:///manual.md', 'steam://store/0', 'mailto:manual@example.com', 'https://[', 'https://account@example.com/']) {
    assert.deepEqual(resolveWebLink(url, app), { kind: 'blocked' })
  }
})
