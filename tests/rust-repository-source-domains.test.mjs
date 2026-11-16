import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/repository_sources')
const read = path => readText(resolve(root, path))
const owners = {
  cache: ['clear_source_cache', 'clear_source_repositories'],
  diagnostics: ['safe_log_address', 'error_category'],
  document: ['nickname_fallback', 'normalize_source_nickname', 'deserialize_builtin', 'source_address_key', 'parse_document'],
  embedded: ['bundled_files', 'bundled_repository_configs', 'bundled_document', 'parse_bundled_document', 'bundled_text'],
  location: ['parse', 'resolve', 'resolve_remote_path', 'remote', 'bundled_path', 'address', 'is_file_relative_path', 'raw_repository_root', 'remote_document_location'],
  transport: ['package_document_path', 'load_text_document', 'load_document'],
  verified: ['load_verified_text_document', 'fetch_document_bytes'],
  apply: ['import_repository_source_into_config', 'sync_manifest_groups'],
}

function ordered(text, parts) {
  let previous = -1
  for (const part of parts) {
    const index = text.indexOf(part, previous + 1)
    assert.ok(index > previous, `missing or out of order: ${part}`)
    previous = index
  }
}

test('repository source facade exports unique responsibility owners and the shared location type', () => {
  const entry = read('../repository_sources.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    assert.match(entry, new RegExp(`mod ${module};`))
    for (const name of names) {
      const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^;{}]*>)?\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(definition)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(text, /crate::commands::|#\[tauri::command\]/, path)
  }
  assert.equal(files.reduce((count, [, text]) => count + [...text.matchAll(/\benum SourceLocation\b/g)].length, 0), 1)
  assert.equal(files.reduce((count, [, text]) => count + [...text.matchAll(/static INDEX:/g)].length, 0), 1)
  const embedded = read('embedded.rs')
  assert.match(embedded, /include!\(concat!\(env!\("OUT_DIR"\), "\/repository_sources\.rs"\)\)/)
  assert.match(embedded, /INDEX\.get_or_init\(\|\| self\.parse_bundled_document\(\)\)\.clone\(\)/)
  assert.match(embedded, /matches!\(self, Self::Bundled\(path\) if path\.eq_ignore_ascii_case\(BUILTIN_INDEX\)\)/)
  assert.doesNotMatch(read('transport.rs'), /OnceLock|INDEX\.get/)
})

test('MOD and map sources retain shared reading, verified failure and cache application contracts', () => {
  const location = read('location.rs')
  assert.match(location, /!matches!\(url\.scheme\(\), "http" \| "https"\)/)
  assert.match(location, /!url\.username\(\)\.is_empty\(\) \|\| url\.password\(\)\.is_some\(\)/)
  assert.match(location, /resolved\.origin\(\) != root\.origin\(\) \|\| !resolved\.path\(\)\.starts_with\(root\.path\(\)\)/)
  assert.match(location, /release_asset_reference\(base\)\.is_some\(\)/)
  const document = read('document.rs')
  ordered(document, ['validate_repository_platform(', 'normalize_repository_platform(', 'validate_mod_repository(repository)?'])
  assert.match(document, /!ids\.insert\(source\.id\.clone\(\)\)/)
  assert.match(document, /\.retain\(\|source\| urls\.insert\(source_address_key\(&source\.url, source\.region\.as_deref\(\)\)\)\)/)
  const transport = read('transport.rs')
  assert.match(transport, /pub const SOURCE_DOCUMENT_TIMEOUT: u64 = 30;/)
  assert.match(transport, /const MAX_SOURCE_BYTES: usize = 1024 \* 1024;/)
  assert.equal([...transport.matchAll(/remote_document_location\(url, response\.url\(\)\.clone\(\)\)/g)].length, 1)
  assert.equal([...transport.matchAll(/"no-cache, no-store, max-age=0"/g)].length, 1)
  assert.equal([...transport.matchAll(/if !status\.is_success\(\)/g)].length, 1)
  assert.match(transport, /load_text_document\(location, network, \|text, origin\|[\s\S]*parse_document\(text, origin, inherited\)/)
  assert.match(document, /document\.network_plan = document\.network_plan\.inherit\(inherited\)/)
  const verified = read('verified.rs')
  assert.equal([...verified.matchAll(/return load_text_document\(location, network, parse\)\.await;/g)].length, 3)
  ordered(verified, ['package_document_path(location).is_some()', 'let SourceLocation::Remote(url)', 'if verified.candidates.is_empty()'])
  ordered(verified, ['if verified.candidates.is_empty()', 'crate::download::try_download_candidates(', 'let text = String::from_utf8(bytes)', 'parse(&text, location)'])
  assert.doesNotMatch(verified.slice(verified.indexOf('crate::download::try_download_candidates(')), /load_text_document\(/)
  assert.match(verified, /crate::archive::validate_download_bytes\(&content, expected_size, digest\)\?/)
  assert.equal([...verified.matchAll(/error\.without_url\(\)\.to_string\(\)/g)].length, 1)
  const map = read('../map_sources/transport.rs')
  assert.match(map, /use crate::repository_sources::SourceLocation;/)
  assert.match(map, /crate::repository_sources::load_verified_text_document\(/)
  assert.match(map, /crate::repository_sources::load_text_document\(\s*location,\s*&ctx\.config\.network,\s*\|text, origin\|[\s\S]*parse_document\(text, origin, inherited\)/)
  const cache = read('cache.rs')
  ordered(cache, ['source_repository_ids.insert(key, repo.id.clone())', 'staged_release_cache.insert(', 'config.repositories.retain('])
  const apply = read('apply.rs')
  assert.equal([...apply.matchAll(/let _cache_guard = state\.repository_cache\.lock\(\)\.await;/g)].length, 2)
  assert.equal([...apply.matchAll(/let _operation_guard = state\s*\.operation\s*\.lock\(\)/g)].length, 2)
  ordered(apply, ['let _cache_guard', 'let _operation_guard', 'let (document, selection) = if official_root', 'state.config.official_root(force).await?', 'repository_sources::load_document(&location, &parent_plan, &network).await', 'let _cache_guard', 'let _operation_guard', 'config.staged_release_cache.remove(previous_id)', 'sync_manifest_groups(&state, &config)'])
  assert.match(apply, /state\.config\.data_selection\(\) != selection/)
  ordered(apply, ['repository_sources::load_document(&location, &parent_plan, &network).await', 'super::cache::clear_source_repositories(', 'state.config.save(&config)'])
  assert.doesNotMatch(apply.slice(0, apply.indexOf('repository_sources::load_document(&location, &parent_plan, &network).await')), /clear_source_cache\(|clear_source_repositories\(|state\.config\.save\(/)
  assert.doesNotMatch(apply, /staged_release_cache\.clear\(/)
  assert.match(apply, /let child_builtin = parent_builtin && child\.builtin\.unwrap_or\(true\);/)
  assert.match(read('../commands/repository_sources.rs'), /use crate::repository_sources::apply::import_repository_source_into_config;/)
})

test('source document transfers capture network settings and retain their independent total cap', () => {
  const transport = read('transport.rs')
  const verified = read('verified.rs')
  for (const source of [transport, verified]) {
    assert.match(source, /network: &NetworkConfig/)
    assert.match(source, /let first_byte_timeout = network\.first_byte_timeout\(\);/)
    assert.match(source, /releases::build_download_client\(first_byte_timeout\)/)
    assert.match(source, /\.timeout\(Duration::from_secs\((?:timeout_seconds|SOURCE_DOCUMENT_TIMEOUT)\)\)/)
    ordered(source, ['let deadline = tokio::time::Instant::now() + first_byte_timeout', 'send_before_first_byte_deadline(request, Some(deadline))', 'first_nonempty_chunk(&mut body, Some(deadline))'])
    assert.doesNotMatch(source, /network\.timeout_seconds|build_client\(/)
  }
  assert.match(verified, /digest\.as_deref\(\),\s*first_byte_timeout,/)
  assert.match(verified, /\},\s*network,\s*None,/)
  const runtime = read('../network_policy/runtime.rs')
  assert.match(runtime, /refresh_from\([\s\S]*?network: &crate::models::NetworkConfig/)
  assert.match(runtime, /load_verified_text_document\(\s*&location,\s*&verified,\s*network,/)
  assert.match(runtime, /load_text_document\(&location, network,/)
})


test('official package dispatch retains native verification and one blocking ZIP parser', () => {
  const download = read('package/download.rs')
  assert.match(download, /if\s+provider\s*==\s*DataProvider::Gitee\s*\{\s*browser_bytes\(browser,\s*archive_address,\s*provider\)\.await\?\s*\}/)
  assert.match(download, /Err\(ArchiveError::Verification\)\s+if\s+provider\s*==\s*DataProvider::Gitcode\s*=>\s*\{\s*browser_bytes\(browser,\s*archive_address,\s*provider\)\.await\?\s*\}/)
  assert.match(download, /tokio::task::spawn_blocking\(move\s*\|\|\s*SourcePackage::from_zip\(&bytes\)\.map\(Arc::new\)\)/)
  assert.doesNotMatch(download, /try_download_candidates|bearer_auth|AUTHORIZATION/, 'official archives use the shared native/anonymous download paths')
})

test('archive HTTP and native verification share provider UA selection and bounded requests', () => {
  const download = read('package/download.rs')
  assert.match(download, /const\s+MANAGER_USER_AGENT:\s*&str\s*=\s*concat!\(\s*"Overcooked 2 MOD Manager\/",\s*env!\("CARGO_PKG_VERSION"\),?\s*\)/)
  assert.match(download, /DataProvider::Cloudflare\s*\|\s*DataProvider::Builtin\s*=>\s*MANAGER_USER_AGENT/)
  assert.match(download, /DataProvider::Github\s*\|\s*DataProvider::Gitee\s*\|\s*DataProvider::Gitcode\s*=>\s*BROWSER_USER_AGENT/)
  assert.match(download, /\.header\(reqwest::header::USER_AGENT,\s*archive_user_agent\(provider\)\)/)
  assert.match(download, /user_agent:\s*archive_user_agent\(provider\)\.to_owned\(\)/)
  ordered(download, ['let request = browser_request(address, provider)?', 'crate::webview_download::download(app, request).await'])
  assert.match(download, /Ok\(crate::webview_download::Request\s*\{/)
  assert.match(download, /max_bytes:\s*MAX_ARCHIVE_BYTES/)
  assert.match(download, /timeout:\s*std::time::Duration::from_secs\(120\)/)
  assert.match(download, /allowed_download_paths:\s*if\s+provider\s*==\s*DataProvider::Gitee\s*\{\s*vec!\[\s*"\/ch3ngyz\/Overcooked-2-MOD-Manager-Repos\/repository\/download"\.to_owned\(\)/)
  assert.match(download, /"\/ch3ngyz\/Overcooked-2-MOD-Manager-Repos\/repository\/blazearchive\/main\.zip"\s*\.to_owned\(\)/, 'Gitee redirects verified archives to its signed blazearchive endpoint')
})

test('official child documents read the captured immutable package through the shared transport', () => {
  const entry = read('../repository_sources.rs'), archive = read('package.rs')
  assert.match(entry, /mod\s+package\s*;/)
  assert.match(entry, /use\s+package::\{\s*SourcePackage,\s*fetch\s+as\s+fetch_source_package\s*\}/)
  assert.match(archive, /tokio::task_local!\s*\{\s*static\s+ACTIVE:\s*Arc\u003cSourcePackage>;\s*\}/)
  assert.match(archive, /ACTIVE\.try_with\(Arc::clone\)\.ok\(\)/)
  assert.match(archive, /"source\.packageFileMissing"/)
  const transport = read('transport.rs')
  assert.match(transport, /SourcePackage::active\(\)/)
  assert.match(transport, /let\s+text\s*=\s*package\.read\(&path\)\?/)
  const runtime = read('../github_proxy_runtime/official_root.rs')
  assert.match(runtime, /SourcePackage::in_snapshot\(package,\s*work\)\.await/)
  assert.match(runtime, /snapshot\.package\s*=\s*Some\(package\.clone\(\)\)/)
  assert.match(runtime, /snapshot\.revision\s*\+=\s*1/)
})

test('task-local package and policy wrappers box work before constructing their scoped futures', () => {
  const archive = read('package.rs'), policy = read('../network_policy/runtime.rs')
  assert.doesNotMatch(archive, /\basync\s+fn\s+in_snapshot\b/)
  assert.match(archive, /ACTIVE\.scope\(package,\s*Box::pin\(work\)\)/)
  assert.doesNotMatch(policy, /\basync\s+fn\s+in_snapshot\b/)
  assert.match(policy, /ACTIVE\.scope\(policy,\s*Box::pin\(work\)\)/)
  const runtime = read('../github_proxy_runtime/official_root.rs')
  assert.doesNotMatch(runtime, /\basync\s+fn\s+(?:in_selection|official_root)\b/)
  assert.match(runtime, /let\s+work\s*=\s*Box::pin\(work\)/)
  assert.match(runtime, /Box::pin\(self\.official_root_from\(/)
  const config = read('../config/storage.rs')
  assert.match(config, /\bfn\s+in_data_selection\b/)
  assert.doesNotMatch(config, /\basync\s+fn\s+in_data_selection\b/)
})

test('startup shares the official root without eagerly expanding builtin map or paused community trees', () => {
  const defaults = read('../config/defaults.rs')
  assert.match(defaults, /\bmap_repositories:\s*Vec::new\(\)/)
  assert.match(defaults, /\bcommunity_map_repositories:\s*Vec::new\(\)/)
  assert.doesNotMatch(defaults, /bundled_map_tree\s*\(/)
  assert.match(read('preflight.rs'), /config\.official_root\(false\)/)
  const ownership = read('../community/ownership.rs')
  assert.match(ownership, /if\s+include_bundled\s*\{\s*keys\.extend\(bundled_official_repository_keys\(\)\);?\s*\}/)
  assert.match(ownership, /if\s+has_candidates\s*\{\s*official_repository_keys\(config,\s*super::DISCOVERY_ENABLED\)\s*\}\s*else\s*\{\s*HashSet::new\(\)/)
  assert.match(read('../community.rs'), /const\s+DISCOVERY_ENABLED:\s*bool\s*=\s*false/)
})

test('native archive downloads are registered and closed with the main window', () => {
  const lib = read('../lib.rs'), browser = read('../webview_download.rs')
  assert.match(lib, /^\s*mod\s+webview_download\s*;/m)
  assert.match(lib, /webview_download::close_with_main\(window,\s*event\)/)
  ordered(lib, ['let proxy_config = state.config.clone()', 'proxy_config.set_source_browser(app.handle().clone())', 'app.manage(state.clone())', 'async_runtime::spawn(async move {', 'proxy_config.refresh_proxies().await'])
  assert.match(browser, /window\.label\(\)\s*!=\s*"main"\s*\|\|\s*!matches!\(event,\s*WindowEvent::Destroyed\)/)
  assert.match(browser, /label\.starts_with\(WINDOW_PREFIX\)/)
  assert.match(browser, /webview\.destroy\(\)/)
  assert.match(browser, /\.on_download\(/)
  assert.match(browser, /DownloadEvent::Requested/)
  assert.match(browser, /DownloadEvent::Finished/)
  assert.match(browser, /Drop\s+for\s+DownloadGuard/)
  assert.match(browser, /self\.state\.cancel\(\)/)
  assert.doesNotMatch(browser, /reqwest::|bearer_auth/, 'the native download keeps its verified session instead of replaying a URL')
})
