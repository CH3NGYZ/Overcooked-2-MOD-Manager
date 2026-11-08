import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = path => readText(resolve(root, path))

test('commands root only declares domains; registered IPC points at the real implementation', () => {
  const index = read('commands.rs').replace(/^\/\/.*$/gm, '').trim()
  assert.ok(index)
  assert.match(index, /^(?:pub\(crate\) mod [a-z_]+;\s*)+$/)
  const handler = read('lib.rs').match(/generate_handler!\s*\[([\s\S]*?)\]/)?.[1]
  assert.ok(handler)
  const paths = [...handler.matchAll(/\bcommands::([a-z_]+)::([a-z_]+)\b/g)]
  assert.equal(paths.length, 108, '迁移的 IPC 集合不得因拆分丢失')
  const declared = sourceFiles(resolve(root, 'commands'), ['.rs']).flatMap(path =>
    [...readText(path).matchAll(/^#\[tauri::command\]\s*pub (?:async )?fn ([a-z_]+)\(/gm)].map(m => m[1]))
  assert.equal(new Set(declared).size, declared.length, '命令不能在旧入口重复定义')
  assert.deepEqual(declared.sort(), paths.map(m => m[2]).sort())
  for (const [, domain, name] of paths) {
    assert.match(read(`commands/${domain}.rs`), new RegExp(`#\\[tauri::command\\]\\s*pub (?:async )?fn ${name}\\(`))
  }
})

test('shared Rust domains never import command implementations', () => {
  for (const file of sourceFiles(root, ['.rs'])) {
    if (file === resolve(root, 'lib.rs')) continue
    assert.doesNotMatch(readText(file), /\bcrate::commands::/, `${file} depends on the IPC layer`)
  }
})

test('extracted business helpers have one owner outside IPC and do not become another large file', () => {
  const owners = [
    ['mods/install.rs', ['install_manual_mod', 'install_staged_repository', 'install_downloaded_repository', 'repository_install_identity']],
    ['mods/status.rs', ['collect_update_status']],
    ['mods/file_state.rs', ['set_local_mod_file_state']],
    ['framework.rs', ['backup_framework', 'restore_framework_backup', 'validate_framework_backup_in']],
    ['bepinex/install.rs', ['install_framework_archive']],
    ['repository_sources/apply.rs', ['import_repository_source_into_config']],
    ['level_sets/staging.rs', ['cache_file_to_discard', 'validate_map_release_sets']],
    ['level_sets/view.rs', ['level_set_to_info', 'bind_map_relation_snapshot']],
    ['map_sources/view.rs', ['map_to_info']],
    ['app_update/manifest.rs', ['inspect_installer_manifest', 'discover_installer_update']],
    ['app_update/download.rs', ['download_verified_installer', 'discover_and_download_installer_update']],
    ['app_update/cache.rs', ['app_update_cache_get', 'app_update_cache_store']],
    ['release_cache/freshness.rs', ['cache_is_fresh']],
  ]
  const files = sourceFiles(root, ['.rs']).map(file => [file, readText(file)])
  for (const [file, names] of owners) {
    assert.ok(read(file).trimEnd().split('\n').length < 1000, `${file} must remain cohesive and bounded`)
    for (const name of names) {
      // Avatar freshness is a different private rule over file metadata, not this timestamp policy.
      const signature = name === 'cache_is_fresh' ? '\\s*cached_at: Option<i64>' : ''
      const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(${signature}`, 'g')
      const actual = files.flatMap(([path, source]) => [...source.matchAll(definition)].map(() => path))
      assert.deepEqual(actual, [resolve(root, file)], `${name} must have one domain owner`)
    }
  }
  for (const file of sourceFiles(resolve(root, 'commands'), ['.rs'])) {
    assert.ok(readText(file).trimEnd().split('\n').length < 1000, `${file} must not become a replacement monolith`)
  }
})
