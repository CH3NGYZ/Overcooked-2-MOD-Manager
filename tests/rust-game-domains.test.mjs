import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = path => readText(resolve(root, path))
const owners = {
  'game/discovery': ['detect_game', 'detect_game_from_settings', 'location', 'find_game_path'],
  'game/selection': ['game_executable_exists', 'executable_exists_for_package', 'selected_game_root', 'selected_root_for_package', 'selection_root', 'expand_home'],
  'game/steam': ['steam_root_paths', 'steam_libraries_from_roots', 'steam_launch_command', 'steam_launch_strategy', 'native_steam_launch'],
  'game/process': ['is_game_running', 'is_game_running_with_system', 'running_processes', 'process_looks_running', 'is_wine_host_process', 'process_matches_game', 'is_crossover_running', 'ensure_game_stopped'],
  'game/stop': ['stop_game', 'graceful_wait_plan', 'close_window_indexes', 'wait_until_stopped', 'wait_until_stopped_with_progress', 'request_graceful_close', 'send_close', 'last_system_error'],
  'game/paths': ['safe_game_path', 'resolve_existing_ancestor', 'normalize_path'],
  file_metadata: ['read_file_version', 'portable_pe_version', 'read_file_metadata', 'read_version_string'],
  'bepinex/detection': ['detect_bepinex'],
}

test('Game root exports real owners and removes misplaced framework and PE implementations', () => {
  const entry = read('game.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  assert.doesNotMatch(entry, /detect_bepinex|read_file_version|read_file_metadata|FileMetadata/)
  const files = [
    ...sourceFiles(resolve(root, 'game'), ['.rs']),
    ...sourceFiles(resolve(root, 'file_metadata'), ['.rs']),
    resolve(root, 'file_metadata.rs'),
    resolve(root, 'bepinex/detection.rs'),
    resolve(root, 'bepinex/loader_tests.rs'),
  ].map(path => [path, readText(path)])
  for (const [owner, names] of Object.entries(owners)) {
    for (const name of names) {
      const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^;{}]*>)?\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(definition)].map(() => path))
      const count = ['is_crossover_running', 'request_graceful_close'].includes(name) ? 2 : 1
      assert.deepEqual(found, Array(count).fill(resolve(root, `${owner}.rs`)), name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, path)
    assert.doesNotMatch(text, /crate::commands::|#\[tauri::command\]/, path)
  }
  assert.match(read('lib.rs'), /^mod file_metadata;$/m)
  assert.match(read('bepinex.rs'), /pub\(crate\) use detection::detect_bepinex;/)
  assert.match(read('game/discovery.rs'), /use crate::bepinex::\{Package, detect_bepinex\};/)
  assert.match(read('bepinex/detection.rs'), /use crate::file_metadata::read_file_version;/)
  assert.match(read('mods/metadata.rs'), /use crate::file_metadata::read_file_metadata;/)
  assert.match(read('recycle_bin/repository.rs'), /crate::file_metadata::read_file_metadata\(&path\)/)
  assert.match(read('update_migration.rs'), /crate::file_metadata::read_file_version\(&executable\)/)
  for (const path of sourceFiles(root, ['.rs'])) {
    assert.doesNotMatch(readText(path), /game::(?:detect_bepinex|read_file_version|read_file_metadata|FileMetadata)\b/, path)
  }
})

test('Game discovery, observer, stopping and path checks keep their single shared chain', () => {
  const discovery = read('game/discovery.rs')
  assert.ok(discovery.indexOf('if !config.path.trim().is_empty()') < discovery.indexOf('std::env::current_exe()'))
  assert.match(discovery, /stored\.starts_with\(normalize_path\(&library\)\)/)
  assert.match(discovery, /\.map\(\|value\| detect_bepinex\(value, backup_root\.as_deref\(\)\)\)/)
  const selection = read('game/selection.rs')
  assert.match(selection, /executable\.is_dir\(\) && executable\.join\("Contents\/MacOS"\)\.is_dir\(\)/)
  assert.match(selection, /fs::canonicalize\(&root\)/)
  const process = read('game/process.rs')
  assert.match(process, /ProcessStatus::Zombie \| ProcessStatus::Dead[\s\S]*?return false;/)
  assert.match(process, /process_matches_game\(game, process\.name\(\), process\.exe\(\), process\.cmd\(\)\)/)
  assert.match(process, /game_process && executable\.is_none\(\)\s*\|\| args/)
  assert.match(process, /\.contains\("overcooked2\.exe"\)/)
  const stop = read('game/stop.rs')
  assert.match(stop, /use super::process::\{is_game_running, process_looks_running, running_processes\};/)
  assert.ok(stop.indexOf('request_graceful_close(&targets)') < stop.indexOf('process.kill()'))
  assert.match(stop, /GRACEFUL_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs\(20\)/)
  assert.match(stop, /FORCE_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs\(6\)/)
  assert.match(stop, /GRACEFUL_GRACE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis\(1500\)/)
  assert.match(stop, /if report\.posted > 0[\s\S]*?else if cfg!\(windows\)[\s\S]*?GracefulWait::Immediate/)
  assert.match(stop, /PostMessageW\(hwnd, WM_CLOSE, 0, 0\)/)
  assert.match(stop, /SendMessageTimeoutW\(/)
  const paths = read('game/paths.rs')
  assert.match(paths, /let root = resolve_existing_ancestor\(game\)\?;\s*let candidate = resolve_existing_ancestor\(&root\.join\(rel\)\)\?;/)
  assert.match(paths, /if !candidate\.starts_with\(&root\)/)
  assert.match(paths, /Err\(error\) if error\.kind\(\) == std::io::ErrorKind::NotFound => continue/)
  const observer = read('game_state.rs')
  assert.match(observer, /game::is_game_running_with_system\(Path::new\(path\), &mut self\.processes\)/)
  assert.match(observer, /game::steam_libraries_from_roots\(steam_roots\)/)
  assert.match(read('commands/game.rs'), /spawn_blocking\([\s\S]*?move \|\| game::stop_game\(&game_root\)/)
  assert.match(read('game/steam.rs'), /SteamStrategy::ProtocolHandler => crate::shell_open::open_url\(STEAM_RUN_URL\)/)
  assert.match(read('file_metadata.rs'), /#\[cfg\(not\(windows\)\)\][\s\S]*?64 \* 1024 \* 1024/)
})
