import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/repository_sources')
const read = path => readText(resolve(root, path))
const owners = {
  cache: ['clear_source_cache'],
  diagnostics: ['safe_log_address', 'error_category'],
  document: ['nickname_fallback', 'normalize_source_nickname', 'deserialize_builtin', 'source_address_key', 'parse_document'],
  embedded: ['bundled_repository_configs', 'bundled_document', 'parse_bundled_document', 'bundled_text'],
  location: ['parse', 'resolve', 'resolve_remote_path', 'remote', 'bundled_path', 'address', 'is_file_relative_path', 'raw_repository_root', 'remote_document_location'],
  transport: ['load_text_document', 'load_document'],
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
  assert.match(transport, /load_text_document\(location, \|text, origin\|[\s\S]*parse_document\(text, origin, inherited\)/)
  assert.match(document, /document\.network_plan = document\.network_plan\.inherit\(inherited\)/)
  const verified = read('verified.rs')
  assert.equal([...verified.matchAll(/return load_text_document\(location, parse\)\.await;/g)].length, 2)
  ordered(verified, ['if verified.candidates.is_empty()', 'crate::download::try_download_candidates(', 'let text = String::from_utf8(bytes)', 'parse(&text, location)'])
  assert.doesNotMatch(verified.slice(verified.indexOf('crate::download::try_download_candidates(')), /load_text_document\(/)
  assert.match(verified, /crate::archive::validate_download_bytes\(&content, expected_size, digest\)\?/)
  assert.equal([...verified.matchAll(/error\.without_url\(\)\.to_string\(\)/g)].length, 2)
  const map = read('../map_sources/transport.rs')
  assert.match(map, /use crate::repository_sources::SourceLocation;/)
  assert.match(map, /crate::repository_sources::load_verified_text_document\(/)
  assert.match(map, /crate::repository_sources::load_text_document\(location, \|text, origin\|[\s\S]*parse_document\(text, origin, inherited\)/)
  const cache = read('cache.rs')
  ordered(cache, ['source_repository_ids.insert(key, repo.id.clone())', 'staged_release_cache.insert(', 'config.repositories.retain('])
  const apply = read('apply.rs')
  assert.equal([...apply.matchAll(/let _cache_guard = state\.repository_cache\.lock\(\)\.await;/g)].length, 2)
  assert.equal([...apply.matchAll(/let _operation_guard = state\s*\.operation\s*\.lock\(\)/g)].length, 2)
  ordered(apply, ['let _cache_guard', 'let _operation_guard', '};\n    let document_url', 'repository_sources::load_document(&location, &parent_plan).await', 'let _cache_guard', 'let _operation_guard', 'config.staged_release_cache.remove(previous_id)', 'sync_manifest_groups(&state, &config)'])
  assert.doesNotMatch(apply, /staged_release_cache\.clear\(/)
  assert.match(apply, /let child_builtin = parent_builtin && child\.builtin\.unwrap_or\(true\);/)
  assert.match(read('../commands/repository_sources.rs'), /use crate::repository_sources::apply::import_repository_source_into_config;/)
})
