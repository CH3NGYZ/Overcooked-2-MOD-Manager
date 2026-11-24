import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = name => readText(resolve(root, name))

// Candidate policy must be reusable without depending on the IPC or MOD domains.
test('download policy has one implementation in the download domain', () => {
  const definitions = [
    'ordered_asset_download_candidates', 'plan_pasted_file', 'plan_source_document',
    'classify_remote_url', 'provider_for_remote_url', 'generic_download_threads', 'plan_repository_download', 'plan_avatar_download', 'materialize_route',
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

test('MOD and map downloads use resolved repository plans; unrecognized URLs remain direct', () => {
  for (const file of ['mods/repository_download.rs', 'commands/map_install.rs']) {
    const source = read(file)
    assert.match(source, /crate::download::planning::plan_repository_download\(/, file)
    assert.doesNotMatch(source, /(?:repo|map)\.download_threads/, file)
    assert.match(source, /archive::release_download::download\(/, file)
  }
  assert.match(read('archive/release_download.rs'), /execution: Some\(planned\.execution\)/)
  assert.match(read('direct_mod.rs'), /RemoteUrlKind::Other\s*=>\s*\(\s*vec!\[DownloadCandidate \{\s*route: DownloadRoute::Direct,[\s\S]*?generic_download_threads\(/)
})

test('MOD, maps, README and avatars use shared planning without IPC helpers', () => {
  const consumers = [
    ['direct_mod.rs', ['plan_pasted_file', 'classify_remote_url']],
    ['map_sources/transport.rs', ['plan_source_document', 'RemoteUrlKind']],
    ['repository_readme.rs', ['plan_repository_download', 'RawProxyContext']],
    ['avatars.rs', ['plan_avatar_download']],
    ['bepinex.rs', ['plan_repository_download', 'provider_for_remote_url']],
    ['github_proxy_runtime.rs', ['ordered_asset_download_candidates']],
  ]
  for (const [file, names] of consumers) {
    const source = read(file)
    assert.match(source, /(?:use crate::download::planning::|crate::download::planning::)/, `${file} must use download planning`)
    for (const name of names) assert.match(source, new RegExp(`\\b${name}\\b`), `${file} missing ${name}`)
    assert.doesNotMatch(source, /crate::commands::(?:\{[^}]*\b(?:plan_pasted_file|plan_source_document|plan_repository_download)\b|(?:plan_pasted_file|plan_source_document|plan_repository_download))/s, `${file} depends on IPC planning`)
  }
  const readme = read('repository_readme.rs')
  assert.match(readme, /plan_repository_download\(\s*&repo\.provider,\s*repo\.kind,\s*&repo\.network_plan,/)
  assert.match(readme, /\.get\(url\)\s*\.timeout\(network\.api_timeout\(\)\)/)
  const framework = read('commands/bepinex.rs')
  assert.match(framework, /package\.plan_download\(&provider, &config\.region\)/)
  assert.match(framework, /execution: Some\(planned\.execution\)/)
  assert.match(framework, /archive::is_download_cancelled/)
  assert.doesNotMatch(framework, /github_proxies\.release_proxies/)
})
