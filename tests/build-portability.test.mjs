import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { BUILD_KEYS, PLATFORM_KEYS, updateAssetKey } from '../scripts/release-lib.mjs'
import { buildUpdateKeys, updateFiles } from '../scripts/build-release.mjs'
import { cacheKeyPrefix } from '../scripts/build-cache.mjs'

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

test('release and cache workflows build only Windows and both macOS architectures', () => {
  const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-pc-windows-msvc']
  assert.deepEqual(Object.keys(BUILD_KEYS).sort(), targets)
  assert.equal(PLATFORM_KEYS.length, 4)
  for (const path of ['.github/workflows/build.yml', '.github/workflows/build-cache.yml']) {
    const workflow = source(path)
    const declared = [...workflow.matchAll(/^\s+target: '([^']+)'$/gm)].map(match => match[1]).sort()
    assert.deepEqual(declared, targets, path)
    assert.doesNotMatch(workflow, /apt-get|libwebkit2gtk|libappindicator|libfuse/)
  }
  const capabilities = JSON.parse(source('src-tauri/capabilities/default.json'))
  assert.deepEqual(capabilities.platforms.slice().sort(), ['macOS', 'windows'])
})

test('Linux targets and package formats cannot enter release, cache, or update selection', () => {
  const target = 'x86_64-unknown-linux-gnu'
  assert.throws(() => buildUpdateKeys([], target), /Unsupported release build target/)
  assert.throws(() => updateFiles('unused', target, '2.1.1'), /Unsupported release build target/)
  assert.throws(() => cacheKeyPrefix(target), /Unsupported build-cache target/)
  for (const name of [
    'OC2ModManager_2.1.1_amd64.AppImage',
    'OC2ModManager_2.1.1_amd64.deb',
    'OC2ModManager_2.1.1_x86_64.rpm',
  ]) assert.equal(updateAssetKey(name, '2.1.1'), undefined, name)
})

test('direct Cargo builds reject unsupported operating systems before generating app resources', () => {
  const build = source('src-tauri/build.rs')
  assert.match(build, /matches!\(target_os\.as_str\(\), "windows" \| "macos"\)/)
  assert.ok(build.indexOf('matches!(target_os.as_str()') < build.indexOf('embed_repository_sources();'))
})
