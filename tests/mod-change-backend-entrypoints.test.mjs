import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText } from './helpers/source-scan.mjs'

const read = path => readText(resolve('src-tauri/src', path))

test('legacy install and switch IPCs block before any live filesystem helper', () => {
  const entries = [
    ['commands/mods_local.rs', ['install_local_mod', 'toggle_local_mod', 'toggle_local_mod_package', 'set_all_local_mods_enabled']],
    ['commands/diy_level.rs', ['set_diy_level_enabled']],
    ['commands/levels.rs', ['set_level_set_enabled', 'set_all_level_sets_enabled']],
    ['commands/map_install.rs', ['import_level_sets']],
    ['mod_preview/commands.rs', ['confirm_mod_install']],
    ['direct_mod.rs', ['install_online_mod']],
  ]
  for (const [file, names] of entries) {
    const source = read(file)
    for (const name of names) {
      const body = source.match(new RegExp(`pub (?:async )?fn ${name}\\([\\s\\S]*?\\n\\}`))?.[0]
      assert.ok(body, `${file}:${name}`)
      assert.match(body, /require_shared_plan\(\)/, `${name} requires an approved shared plan`)
      assert.doesNotMatch(body, /spawn_blocking|fs::|install_manual_mod|set_local_mod_file_state|install_parsed_sets/, `${name} has no legacy live branch`)
    }
  }
  assert.match(read('mods/repository_download.rs'), /preview_only != Some\(true\)\s*\{\s*return crate::mod_changes::guard::require_shared_plan\(\);\s*\}/)
  assert.doesNotMatch(read('mods/repository_download.rs'), /install_downloaded_mod_repository|mod_repository_install_identity|files_result/)
  assert.match(read('commands/mod_repository_install.rs'), /run_repository_install\(/)
})

test('chain preparation consumes the same repository download without creating a UI preview', () => {
  const preparation = read('mod_changes/preparation.rs')
  assert.match(preparation, /download_repository_asset\(/)
  assert.match(preparation, /payload::from_downloaded\(/)
  assert.doesNotMatch(preparation, /run_repository_install|mod_preview|from_preview/)
  const preview = read('mods/repository_download.rs').match(/fn run_repository_install\([\s\S]*?\n\}/)?.[0]
  assert.match(preview, /download_repository_asset\(/)
  assert.match(preview, /stage_downloaded_file\(/)
})

test('recovery gate protects write and game handoff commands while holding their existing locks', () => {
  for (const path of ['commands/game.rs', 'commands/recycle.rs', 'commands/bepinex.rs',
    'commands/mods_local.rs', 'commands/diy_level.rs', 'manager_data.rs', 'commands/mod_settings.rs']) {
    assert.match(read(path), /mod_changes::guard::ensure_recovered/, path)
  }
  const guard = read('mod_changes/guard.rs')
  const method = name => guard.match(new RegExp(`fn ${name}\\([\\s\\S]*?\\n\\}`))?.[0] ?? ''
  assert.match(method('ensure_plannable'), /try_automatic\(state\)[\s\S]*records_for_plan\(state, None\)/)
  assert.match(method('ensure_recovered'), /try_automatic\(state\)[\s\S]*check_unscoped_write\(state\)/)
  assert.match(method('records_for_plan'), /list\(state\)[\s\S]*all\(can_plan\)/)
  assert.match(method('check_unscoped_write'), /list\(state\)[\s\S]*any\(blocks_unscoped_write\)/)
  assert.match(method('ensure_game_stopped'), /game::ensure_game_stopped/)
  assert.match(method('try_automatic'), /recover_files\([\s\S]*transaction::recover_all[\s\S]*!report\.blocked\.is_empty\(\)/)
  for (const name of ['ensure_plannable', 'ensure_recovered', 'records_for_plan', 'check_unscoped_write', 'try_automatic']) {
    assert.doesNotMatch(method(name), /state\.operation\.lock\(/, name)
  }
  assert.match(method('decide'), /state\.operation\.lock\(/)
})

test('inventory reads do not recover and background writes check the persistent recovery gate', () => {
  assert.doesNotMatch(read('commands/dashboard.rs'), /guard::ensure_recovered/)
  const levels = read('commands/levels.rs').match(/fn prepare_level_set_scan\([\s\S]*?\n\}/)?.[0]
  assert.doesNotMatch(levels, /guard::ensure_recovered/)
  assert.match(levels, /guard::check_unscoped_write\(state\)\.is_ok\(\)[\s\S]*migrate_legacy_disabled_root\(&roots\)/)
  assert.match(read('level_sets/recompress.rs'), /state\.operation\.lock\(\)[\s\S]*guard::check_unscoped_write\(state\)[\s\S]*if !replace\(/)
  assert.match(read('level_sets/recompress.rs'), /if !recovery::any_pending\(\)\s*\{\s*return Ok\(\(\)\);\s*\}\s*crate::mod_changes::guard::check_unscoped_write\(state\)\?[\s\S]*recovery::restore_all\(\)/)
  assert.match(read('repository_sources/apply.rs'), /guard::check_unscoped_write\(&state\)[\s\S]*Ok\(\(\)\) => sync_manifest_groups/)
  assert.match(read('manager_data.rs'), /transaction::has_retained_material/)
})

test('compression recovery IPC uses the generated Tauri argument names', () => {
  const commands = read('commands/levels.rs')
  const service = readText(resolve('src/services/tauri.ts'))
  for (const name of ['retry', 'ignore']) {
    assert.match(commands, new RegExp(`pub async fn ${name}_level_compression_recovery\\([\\s\\S]*?recovery_id: String`))
    assert.match(service, new RegExp(`${name}LevelCompressionRecovery: \\(id: string\\) => invoke<LevelCompressionRecovery\\[]>\\('${name}_level_compression_recovery', \\{ recoveryId: id \\}\\)`))
  }
})
