import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = name => readText(resolve(root, name))

// Candidate policy must be reusable without depending on the IPC or MOD domains.
test('download policy has one implementation in the download domain', () => {
  const definitions = [
    'ordered_asset_download_candidates', 'selected_asset_proxy_mode', 'asset_proxy_urls',
    'provider_proxy_entries', 'plan_remote_download_candidates', 'plan_asset_download_candidates',
    'classify_remote_url', 'provider_for_remote_url',
  ]
  const files = sourceFiles(root, ['.rs'])
  for (const name of definitions) {
    const declaration = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(`, 'g')
    const owners = files.flatMap(path => [...readText(path).matchAll(declaration)].map(() => path))
    assert.deepEqual(owners, [resolve(root, 'download/planning.rs')], `${name} must have one owner`)
  }
  const planning = read('download/planning.rs')
  assert.doesNotMatch(planning, /crate::(?:commands|direct_mod)::/)
})

test('MOD, maps, README and avatars use shared planning without IPC helpers', () => {
  const consumers = [
    ['direct_mod.rs', ['plan_remote_download_candidates', 'classify_remote_url']],
    ['map_sources.rs', ['plan_remote_download_candidates', 'RemoteUrlKind']],
    ['repository_readme.rs', ['asset_proxy_urls', 'ordered_asset_download_candidates']],
    ['avatars.rs', ['selected_asset_proxy_mode', 'asset_proxy_urls', 'ordered_asset_download_candidates']],
    ['github_proxy_runtime.rs', ['ordered_asset_download_candidates']],
  ]
  for (const [file, names] of consumers) {
    const source = read(file)
    assert.match(source, /(?:use crate::download::planning::|crate::download::planning::)/, `${file} must use download planning`)
    for (const name of names) assert.match(source, new RegExp(`\\b${name}\\b`), `${file} missing ${name}`)
    assert.doesNotMatch(source, /crate::commands::(?:\{[^}]*\b(?:plan_remote_download_candidates|asset_proxy_urls|selected_asset_proxy_mode)\b|(?:plan_remote_download_candidates|asset_proxy_urls|selected_asset_proxy_mode))/s, `${file} depends on IPC planning`)
  }
})
