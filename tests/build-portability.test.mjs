import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const source = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')

test('Cargo config does not require an unprovisioned local compiler wrapper', () => {
  const config = source('src-tauri/.cargo/config.toml')
  const active = config.split('\n').filter(line => !line.trimStart().startsWith('#')).join('\n')
  assert.doesNotMatch(active, /^\s*rustc(?:-workspace)?-wrapper\s*=\s*["'][^"']+["']/m)
})

test('proxy fixture explicitly restores blocking socket IO with bounded reads and writes', () => {
  const server = source('src-tauri/src/github_proxy_runtime_tests.rs').split('fn client()')[0]
  assert.match(server, /stream\.set_nonblocking\(false\)/)
  assert.match(server, /set_read_timeout\(Some\(Duration::from_secs\(2\)\)\)/)
  assert.match(server, /set_write_timeout\(Some\(Duration::from_secs\(2\)\)\)/)
  assert.ok(server.indexOf('set_nonblocking(false)') < server.indexOf('stream.read('))
})
