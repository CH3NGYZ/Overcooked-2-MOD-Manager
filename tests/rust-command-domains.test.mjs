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
  const handler = read('lib.rs').match(/audited_handler!\s*\[([\s\S]*?)\]/)?.[1]
  assert.ok(handler)
  const paths = [...handler.matchAll(/\bcommands::([a-z_]+)::([a-z_]+)\b/g)]
  assert.equal(paths.length, 149, '包含独立版更新预约、退出与 Windows 普通权限交接 IPC')
  assert.ok(paths.some(([, domain, name]) => domain === 'logs' && name === 'record_ipc_failure'))
  assert.ok(paths.some(([, domain, name]) => domain === 'logs' && name === 'record_file_drop'))
  assert.ok(paths.some(([, domain, name]) => domain === 'system' && name === 'open_external_page'))
  assert.ok(paths.some(([, domain, name]) => domain === 'system' && name === 'apply_staged_app_update'))
  assert.ok(paths.some(([, domain, name]) => domain === 'update' && name === 'get_app_distribution'))
  assert.ok(paths.some(([, domain, name]) => domain === 'download' && name === 'control_download_race'))
  for (const name of ['stage_app_update', 'restart_update_as_user', 'open_app_update_release_and_exit']) {
    assert.ok(paths.some(([, domain, actual]) => domain === 'update' && actual === name))
  }
  assert.ok(!paths.some(([, domain, name]) => domain === 'update' && name === 'install_app_update'))
  for (const name of ['evaluate_mod_changes', 'plan_mod_changes', 'prepare_mod_changes', 'execute_mod_changes', 'cancel_mod_changes']) assert.ok(paths.some(([, domain, actual]) => domain === 'mod_changes' && actual === name))
  for (const name of ['get_mod_change_recoveries', 'pause_mod_change_recovery', 'retry_mod_change_recovery', 'abandon_mod_change_recovery']) assert.ok(paths.some(([, domain, actual]) => domain === 'mod_changes' && actual === name))
  for (const name of ['get_manager_data_directory', 'clear_all_config', 'migrate_manager_data']) assert.ok(paths.some(([, domain, actual]) => domain === 'manager_data' && actual === name))
  for (const name of ['open_diychef_editor', 'migrate_diychef_resources', 'initialize_diychef_config', 'validate_diychef_draft', 'save_diychef_config', 'delete_diychef_resource']) assert.ok(paths.some(([, domain, actual]) => domain === 'diychef_editor' && actual === name))
  for (const name of ['get_diychef_preview', 'read_diychef_preview_asset', 'clear_diychef_cache']) assert.ok(paths.some(([, domain, actual]) => domain === 'diychef_preview' && actual === name))
  for (const name of ['get_diychef_preview_fingerprint', 'diychef_thumbnail_cache']) assert.ok(paths.some(([, domain, actual]) => domain === 'diychef_thumbnail' && actual === name))
  assert.ok(paths.some(([, domain, name]) => domain === 'clipboard' && name === 'copy_image'))
  assert.ok(paths.some(([, domain, name]) => domain === 'dashboard' && name === 'rebind_local_mods'))
  for (const name of ['get_level_set_preview', 'get_level_preview_detail']) assert.ok(paths.some(([, domain, actual]) => domain === 'levels' && actual === name))
  const declared = sourceFiles(resolve(root, 'commands'), ['.rs']).flatMap(path =>
    [...readText(path).matchAll(/^#\[tauri::command\]\s*pub(?:\(crate\))? (?:async )?fn ([a-z_]+)\(/gm)].map(m => m[1]))
  assert.equal(new Set(declared).size, declared.length, '命令不能在旧入口重复定义')
  assert.deepEqual(declared.sort(), paths.map(m => m[2]).sort())
  for (const [, domain, name] of paths) {
    assert.match(read(`commands/${domain}.rs`), new RegExp(`#\\[tauri::command\\]\\s*pub(?:\\(crate\\))? (?:async )?fn ${name}\\(`))
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
    ['mods/install.rs', ['mod_repository_install_identity']],
    ['mods/repository_download.rs', ['run_repository_install']],
    ['mod_changes/payload.rs', ['from_preview']],
    ['mods/status.rs', ['collect_update_status']],
    ['framework.rs', ['backup_framework', 'restore_framework_backup', 'validate_framework_backup_in']],
    ['bepinex/install.rs', ['install_framework_archive']],
    ['repository_sources/apply.rs', ['import_repository_source_into_config']],
    ['level_sets/staging.rs', ['cache_file_to_discard', 'validate_map_release_sets']],
    ['level_sets/view.rs', ['level_set_to_info', 'bind_map_relation_snapshot']],
    ['map_sources/view.rs', ['map_to_info']],
    ['app_update/manifest.rs', ['inspect_installer_manifest', 'discover_installer_update']],
    ['app_update/download.rs', ['download_verified_installer', 'download_confirmed_update']],
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
