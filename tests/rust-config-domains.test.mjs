import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/config')
const owners = {
  availability: ['repository_is_available', 'repository_matches_channel', 'current_user_region', 'normalize_repository_platform'],
  bundle: ['export_bundle', 'validate_bundle', 'preview_bundle', 'import_bundle', 'serialize_repository_bundle'],
  defaults: ['default_config', 'default_ui_zoom'],
  identity: ['repo_key', 'export_key', 'source_repository_key', 'repository_from_export'],
  metadata: ['localized_name', 'migrate_repository_i18n', 'normalize_repository_i18n'],
  migrations: ['contains_legacy_repository_proxy_fields', 'migrate_bundled_source_switches', 'normalize_configured_regions'],
  public: ['public_config'],
  secrets: ['protect_secret', 'reveal_secret', 'set_publish_tokens', 'api_token'],
  sources: ['builtin_repository_sources_for_region', 'merge_builtin_repository_sources'],
  storage: ['new', 'cached_if_fresh', 'load_from_disk', 'with_proxies', 'save'],
  validation: ['validate_repository', 'check_version_spec', 'validate_relation'],
}

test('config keeps one shared state owner and does not forward domain methods', () => {
  const entry = readText(resolve(root, '../config.rs'))
  assert.equal([...entry.matchAll(/pub struct ConfigService\s*\{/g)].length, 1)
  assert.match(entry, /cache: std::sync::Arc<std::sync::Mutex<Option<CachedConfig>>>/)
  assert.match(entry, /proxies: std::sync::Arc<crate::github_proxy_runtime::ProxyRuntime>/)
  assert.doesNotMatch(entry, /\bfn\s+(?:load|save|import_bundle|public_config)\s*\(/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    for (const name of names) {
      const pattern = new RegExp(`\\bfn\\s+${name}\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(pattern)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  assert.match(readText(resolve(root, 'storage/test_support.rs')), /fn for_test\(/)
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(text, /\b(?:struct ConfigService|crate::commands::)/, path)
  }
})

test('config load keeps migration ordering and current proxy overlay on every return path', () => {
  const storage = readText(resolve(root, 'storage.rs'))
  const migrate = storage.indexOf('changed |= migrate_bundled_source_switches(&mut config);')
  const sync = storage.indexOf('changed |= synchronize_builtin_repository_sources(&mut config);')
  assert.ok(migrate >= 0 && sync > migrate)
  assert.match(storage, /return Ok\(self\.with_proxies\(cached\)\)/)
  assert.match(storage, /Ok\(self\.with_proxies\(config\)\)/)
  assert.match(storage, /self\.with_proxies\(default_config\(\)\)/)
  assert.match(readText(resolve(root, 'public.rs')), /github_proxies: self\.proxy_snapshot\(\)/)
})
