import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/releases')
const read = path => readText(resolve(root, path))
const owners = {
  assets: ['asset_matches', 'select_install_asset'],
  client: ['build_client', 'build_download_client'],
  error: ['release_error_kind'],
  fetch: ['fetch_releases', 'fetch_releases_page', 'fetch_releases_from_targets', 'fetch_repository_default_branch'],
  projection: ['parse_release_json', 'filter_channel'],
  transport: ['api_target_kind', 'api_request_targets', 'fetch_api_with_retries', 'fetch_release_api_with_retries', 'fetch_api_json'],
  version: ['parse_version', 'compare_release_versions', 'is_strict_semver', 'is_prerelease_version', 'suffix_identifiers', 'split_alpha_numeric', 'compare_prerelease_parts'],
}

test('Release root exports real domain owners without forwarding or replacement monoliths', () => {
  const entry = read('../releases.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  const coordinates = read('../providers/coordinates.rs')
  for (const name of ['repository_coordinates', 'repository_homepage', 'repository_release_api_url', 'repository_release_api_url_page', 'repository_file_url', 'repository_file_name', 'release_asset_reference', 'repository_cache_key']) {
    assert.match(coordinates, new RegExp(`\\bfn ${name}\\(`))
    for (const [, text] of files) assert.doesNotMatch(text, new RegExp(`\\bfn ${name}\\(`))
  }
  assert.match(entry, /pub\(crate\) use crate::providers::/)
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
  assert.match(read('transport/test_support.rs'), /pub async fn fetch_release_api\(/)
  assert.match(read('test_support.rs'), /set_read_timeout\(Some\(API_REQUEST_TIMEOUT\)\)/)
})

test('Release domains keep credential isolation, shared fetch and distinct version projections', () => {
  const transport = read('transport.rs')
  assert.match(transport, /Some\(token\) => targets\.push\(\(official_url\.to_string\(\), Some\(token\)\)\)/)
  assert.match(transport, /None => targets\.push\(\(official_url\.to_string\(\), None\)\)/)
  assert.match(transport, /proxy_urls\(api_proxies, official_url, false, None\)[\s\S]*?\.map\(\|url\| \(url, None\)\)/)
  assert.match(transport, /crate::providers::api_request\(client, url, provider, token\)/)
  assert.match(read('../providers/authentication.rs'), /is_official_gitee_api_url\(url\)/)
  assert.equal([...transport.matchAll(/e\.without_url\(\)\.to_string\(\)/g)].length, 2)
  assert.match(transport, /for attempt in 0\.\.REPOSITORY_API_ATTEMPTS/)
  assert.match(transport, /Err\(error\) if error\.is_forbidden\(\) =>/)
  const fetch = read('fetch.rs')
  assert.match(fetch, /crate::api_rate_gate::global_gate\(\)/)
  assert.match(fetch, /gate\.record_forbidden\(&repo\.provider, gate_kind, signal\)/)
  assert.match(fetch, /fetch_releases_from_targets\(client, repo, &official, request_targets\)\.await/)
  assert.match(fetch, /pagination::collect_release_pages/)
  const pagination = read('pagination.rs')
  assert.match(pagination, /RELEASE_PAGE_SIZE: u32 = 50/)
  assert.match(pagination, /let \(items, received\) = fetch\(page\).await\?/)
  assert.match(pagination, /received < RELEASE_PAGE_SIZE as usize/)
  assert.doesNotMatch(fetch, /filter_channel\(/)
  for (const file of ['fetch.rs', 'assets.rs']) {
    assert.match(read(file), /!crate::community::is_source_archive\(\s*&repo.provider,\s*repo.source_id.as_deref\(\),\s*&release\.tag_name,\s*&asset\.name,?\s*\)/)
  }
  const projection = read('projection.rs')
  assert.match(projection, /\|\| is_prerelease_version\(&tag\)/)
  assert.match(projection, /version: parse_version\(&tag\)\.map/)
  assert.match(projection, /a\.version\.as_deref\(\)\.and_then\(parse_version\)/)
  assert.match(read('version.rs'), /Some\(SegmentVersion::parse\(remote\)\?\.cmp\(&SegmentVersion::parse\(local\)\?\)\)/)
  assert.match(read('../mods/status.rs'), /crate::releases::compare_release_versions\(&release\.tag_name, &local\)/)
  assert.match(read('../map_sources/releases.rs'), /crate::releases::fetch_releases\(/)
  assert.match(read('../map_sources/release_tags.rs'), /crate::releases::fetch_releases\(/)
  assert.match(read('../repository_readme.rs'), /releases::fetch_repository_default_branch\(/)
})


test('generic repository reads and map adapters do not construct MOD installation models', () => {
  for (const file of ['fetch.rs', 'transport.rs', 'assets.rs']) {
    assert.match(read(file), /RepositoryRequest/)
    assert.doesNotMatch(read(file), /ModRepositoryConfig|install_target|plugin_guid/)
  }
  for (const file of ['../map_sources/releases.rs', '../map_sources/release_tags.rs', '../map_sources/transport.rs']) {
    assert.match(read(file), /RepositoryRequest/)
    assert.doesNotMatch(read(file), /ModRepositoryConfig|ModRepositoryExport|mod_repository_from_export/)
  }
  const models = read('../models/mod_repositories.rs')
  assert.match(models, /pub struct ModRepositoryConfig/)
  assert.match(models, /pub plugin_guid: String/)
  assert.match(models, /pub install_target: String/)
  assert.doesNotMatch(read('../models/repositories.rs'), /pub struct ModRepositoryConfig|pub plugin_guid:/)
})

test('MOD and map downloads share a ten-second startup window while independent request policies remain intact', () => {
  const client = read('client.rs')
  assert.match(client, /REQUEST_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs\(10\)/)
  assert.match(client, /Duration::from_secs\(timeout_seconds\)\.max\(REQUEST_START_TIMEOUT\)/)
  const [api, download] = client.split('pub fn build_download_client(')
  for (const builder of [api, download]) assert.match(builder, /\.connect_timeout\(connection_timeout\(timeout_seconds\)\)/)
  assert.doesNotMatch(download, /\.timeout\(/)
  assert.match(download, /\.read_timeout\(std::time::Duration::from_secs\(120\)\)/)
  assert.match(read('transport.rs'), /API_REQUEST_TIMEOUT: std::time::Duration = REQUEST_START_TIMEOUT/)
  assert.match(read('transport.rs'), /\.timeout\(API_REQUEST_TIMEOUT\)/)
  assert.match(read('../archive/transfer.rs'), /FIRST_BYTE_TIMEOUT: std::time::Duration = crate::releases::REQUEST_START_TIMEOUT/)
  for (const path of ['../commands/mod_repository_install.rs', '../commands/map_install.rs']) {
    assert.match(read(path), /releases::build_download_client\(config\.network\.timeout_seconds\)/)
    assert.match(read(path), /archive::release_download::download\(/)
  }
  for (const path of ['../community/github.rs', '../community/gitcode.rs', '../app_update/manifest.rs']) {
    assert.match(read(path), /\.timeout\((?:std::time::)?Duration::from_secs\(5\)\)/)
  }
})

test('MOD and map download commands consume cached versions without Release API fallbacks', () => {
  for (const file of ['../commands/mod_repository_install.rs', '../commands/map_install.rs']) {
    const command = read(file)
    assert.match(command, /releases::select_cached_release\(/)
    assert.doesNotMatch(command, /fetch_releases|fetch_map_release_data|refresh_(?:map|repository)_releases|\.release_cache\b/)
  }
  for (const file of ['../mod_preview/commands.rs', '../mods/install.rs', '../direct_mod.rs']) {
    assert.doesNotMatch(read(file), /fetch_releases|fetch_map_release_data|refresh_(?:map|repository)_releases/)
  }
  assert.match(read('../commands/maps.rs'), /fetch_map_release_data\(/)
  assert.match(read('../commands/mod_repository_releases.rs'), /releases::fetch_releases\(/)
  assert.match(read('../commands/map_install.rs'), /select_map_stage_asset\(&map, release, asset_name\.as_deref\(\)\)/)
})
