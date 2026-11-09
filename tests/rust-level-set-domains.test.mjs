import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/level_sets')
const owners = {
  paths: ['levels_root', 'disabled_root', 'is_safe_relative', 'collect_files'],
  scan: ['scan_level_sets_inner', 'collect_level_set_targets', 'assemble_level_set', 'locate_info_file'],
  info_cache: ['read_info_metadata', 'clear_info_cache', 'content_sha256'],
  identity: ['level_set_identity_matches', 'allocate_dir_name'],
  conflicts: ['find_install_conflicts_with_workers', 'conflicts_with_installed'],
  plugin_registry: ['read_registered_plugins', 'write_registered_plugins', 'registered_plugin_paths'],
  plugin_assignment: ['assign_plugins'],
  requirements: ['runtime_relation'],
  operations: [ 'set_all_level_sets_enabled', 'remove_level_set'],
  flatten: ['flatten_level_sets_with', 'rollback_flatten_group'],
  zip_inspect: ['classify_zip', 'inspect_zip_with_progress'],
  zip_info: ['parse_zip_info_directories'],
  zip_plugins: ['prepare_zip_plugin'],
  bepinex_layer: ['inspect_bepinex_layer', 'install_bepinex_layer_sets'],
  install: ['install_parsed_sets', 'write_level_set'],
  installed_versions: ['installed_mapset_version', 'record_installed_mapset_versions'],
}

test('level_sets is a domain entry, not a second implementation or forwarding layer', () => {
  const entry = readText(resolve(root, '../level_sets.rs'))
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  assert.match(entry, /pub use zip_inspect::inspect_zip_with_progress;/)
  assert.match(entry, /pub use install::install_parsed_sets;/)
  assert.match(entry, /pub\(crate\) mod staging;/)
  assert.match(entry, /pub\(crate\) mod view;/)
})

test('map scan, cache, identity, ZIP and transactions each retain one bounded owner', () => {
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    for (const name of names) {
      const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(definition)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(text, /\bcrate::commands::/, `${path} must not depend on IPC`)
  }
})


test('map operations no longer probe game state while actual MOD writes retain their gates', () => {
  for (const path of ['../commands/levels.rs', '../commands/map_install.rs', '../level_sets.rs']) {
    assert.doesNotMatch(readText(resolve(root, path)), /ensure_operation_allowed|LevelSetOperation|ensure_game_stopped|is_game_running/)
  }
  const operations = readText(resolve(root, 'operations.rs'))
  assert.doesNotMatch(operations, /ALLOW_LEVEL_SET|levelSet\.gameRunning|allowed_while_running/)
  for (const path of ['operations.rs', 'install.rs', 'bepinex_layer.rs']) {
    assert.match(readText(resolve(root, path)), /crate::game::ensure_game_stopped\(game_root\)/)
  }
  assert.match(readText(resolve(root, '../commands/mod_repository_install.rs')), /game::is_game_running/)
})
