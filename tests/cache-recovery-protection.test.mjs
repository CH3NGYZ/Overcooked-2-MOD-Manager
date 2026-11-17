import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText } from './helpers/source-scan.mjs'

const read = file => readText(resolve('src-tauri/src', file))

test('cache reset holds operation and completes chain recovery before deleting ordinary caches', () => {
  const source = read('commands/settings.rs')
  assert.match(source, /pub fn reset_ui\([\s\S]*?reset_ui_in\(&state\)/)
  const body = source.match(/fn reset_ui_in\([\s\S]*?\n\}/)?.[0]
  assert.ok(body)
  assert.ok(body.indexOf('.operation') < body.indexOf('ensure_recovered(state)?'))
  assert.ok(body.indexOf('ensure_recovered(state)?') < body.indexOf('clear_resettable_cache('))
  assert.match(body, /repo\.cached_releases\.clear\(\);\s*repo\.cached_releases_complete = false;/)
  assert.doesNotMatch(body, /shutdown|remove_dir_all/)
  const clear = source.match(/fn clear_resettable_cache\([\s\S]*?\n\}/)?.[0]
  assert.ok(clear)
  assert.match(clear, /eq_ignore_ascii_case\("level-compression"\)[\s\S]*?continue;/)
  assert.match(clear, /symlink_metadata/)
  assert.match(clear, /file_attributes\(\) & 0x400 != 0[\s\S]*?continue;/)
})

test('map source discard accepts only downloader-owned root files after resolving aliases', () => {
  const source = read('level_sets/staging.rs')
  const body = source.match(/fn cache_file_to_discard\([\s\S]*?\n\}/)?.[0]
  assert.ok(body)
  assert.match(body, /fs::canonicalize\(path\.trim\(\)\)/)
  assert.match(body, /uuid::Uuid::parse_str/)
  assert.match(body, /matches!\(extension, "download" \| "zip"\)/)
  assert.match(body, /target\.parent\(\) != Some\(cache_dir\.as_path\(\)\)/)
  assert.match(read('archive/transfer.rs'), /temp_dir\.join\(format!\("\{\}\.download", Uuid::new_v4\(\)\)\)/)
  assert.match(read('archive/release_download.rs'), /cache\.join\(format!\("\{\}\.zip", uuid::Uuid::new_v4\(\)\)\)/)
})
