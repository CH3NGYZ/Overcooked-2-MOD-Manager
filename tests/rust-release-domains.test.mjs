import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/releases')
const read = path => readText(resolve(root, path))
const owners = {
  assets: ['asset_matches', 'select_install_asset'],
  client: ['build_client', 'build_download_client'],
  coordinates: ['repository_coordinates', 'repository_homepage', 'repository_release_api_url', 'repository_release_api_url_page', 'repository_file_url', 'repository_file_name', 'release_asset_reference', 'repository_cache_key'],
  error: ['release_error_kind'],
  fetch: ['fetch_releases', 'fetch_all_releases', 'fetch_releases_page', 'fetch_releases_from_targets', 'fetch_repository_default_branch'],
  projection: ['parse_release_json', 'filter_channel'],
  transport: ['api_target_kind', 'api_request_targets', 'fetch_api_with_retries', 'fetch_release_api_with_retries', 'fetch_api_json', 'fetch_release_api', 'is_official_gitee_api_url', 'gitee_api_url_with_token'],
  version: ['parse_version', 'compare_release_versions', 'is_strict_semver', 'is_prerelease_version', 'suffix_identifiers', 'split_alpha_numeric', 'compare_prerelease_parts'],
}

test('Release root exports real domain owners without forwarding or replacement monoliths', () => {
  const entry = read('../releases.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    assert.match(entry, new RegExp(`mod ${module};`))
    for (const name of names) {
      const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(definition)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(text, /crate::commands::|#\[tauri::command\]/, path)
  }
  assert.equal(files.reduce((count, [, text]) => count + [...text.matchAll(/\bstruct ApiTestServer\b/g)].length, 0), 1)
  assert.match(read('test_support.rs'), /stream\.set_nonblocking\(false\)\.unwrap\(\);/)
  assert.match(read('test_support.rs'), /set_read_timeout\(Some\(API_REQUEST_TIMEOUT\)\)/)
})

test('Release domains keep credential isolation, shared fetch and distinct version projections', () => {
  const transport = read('transport.rs')
  assert.match(transport, /Some\(token\) => targets\.push\(\(official_url\.to_string\(\), Some\(token\)\)\)/)
  assert.match(transport, /None => targets\.push\(\(official_url\.to_string\(\), None\)\)/)
  assert.match(transport, /proxy_urls\(api_proxies, official_url, false, None\)[\s\S]*?\.map\(\|url\| \(url, None\)\)/)
  assert.match(transport, /Provider::Gitee, Some\(token\)\) if is_official_gitee_api_url\(url\)/)
  assert.equal([...transport.matchAll(/e\.without_url\(\)\.to_string\(\)/g)].length, 2)
  assert.match(transport, /for attempt in 0\.\.REPOSITORY_API_ATTEMPTS/)
  assert.match(transport, /Err\(error\) if error\.is_forbidden\(\) =>/)
  const fetch = read('fetch.rs')
  assert.match(fetch, /crate::api_rate_gate::global_gate\(\)/)
  assert.match(fetch, /gate\.record_forbidden\(&repo\.provider, gate_kind, signal\)/)
  assert.match(fetch, /fetch_releases_from_targets\(client, repo, &official, request_targets\)\.await/)
  assert.match(fetch, /if all\.is_empty\(\) \{\s*return Err\(error\);/)
  assert.match(fetch, /return Ok\(all\);/)
  assert.match(fetch, /let received = page_releases\.len\(\);/)
  assert.doesNotMatch(fetch, /filter_channel\(/)
  for (const file of ['fetch.rs', 'assets.rs']) {
    assert.match(read(file), /!crate::community::is_source_archive\(repo, &release\.tag_name, &asset\.name\)/)
  }
  const projection = read('projection.rs')
  assert.match(projection, /\|\| is_prerelease_version\(&tag\)/)
  assert.match(projection, /version: parse_version\(&tag\)\.map/)
  assert.match(projection, /a\.version\.as_deref\(\)\.and_then\(parse_version\)/)
  assert.match(read('version.rs'), /Some\(SegmentVersion::parse\(remote\)\?\.cmp\(&SegmentVersion::parse\(local\)\?\)\)/)
  assert.match(read('../mods/status.rs'), /crate::releases::compare_release_versions\(&release\.tag_name, &local\)/)
  assert.match(read('../map_sources/releases.rs'), /crate::releases::fetch_releases\(/)
  assert.match(read('../map_sources/release_tags.rs'), /crate::releases::fetch_all_releases\(/)
  assert.match(read('../repository_readme.rs'), /releases::fetch_repository_default_branch\(/)
})
