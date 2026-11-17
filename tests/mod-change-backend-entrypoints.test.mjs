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
  assert.match(read('mod_changes/preparation.rs'), /mods::repository_download::run_repository_install/)
})

test('recovery gate protects write and game handoff commands while holding their existing locks', () => {
  for (const path of ['commands/dashboard.rs', 'commands/game.rs', 'commands/recycle.rs', 'commands/bepinex.rs',
    'commands/mods_local.rs', 'commands/diy_level.rs', 'commands/levels.rs', 'manager_data.rs', 'commands/mod_settings.rs']) {
    assert.match(read(path), /mod_changes::guard::ensure_recovered/, path)
  }
  const guard = read('mod_changes/guard.rs')
  assert.match(guard, /transaction::has_pending/)
  assert.match(guard, /game::ensure_game_stopped/)
  assert.match(guard, /transaction::recover_all/)
  assert.match(guard, /!report\.blocked\.is_empty\(\)/)
  assert.doesNotMatch(guard, /state\.operation\.lock\(/)
})
