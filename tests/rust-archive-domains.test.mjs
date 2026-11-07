import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/archive')
const owners = {
  control: ['cancel_download', 'start_download', 'begin_app_update', 'app_update_in_progress'],
  progress: ['emit_progress', 'emit_progress_event'],
  transfer: ['download_to_temp_with_threads_and_first_byte_timeout', 'download_stream', 'download_ranges', 'read_response_bytes'],
  integrity: ['sha256_file', 'verify_digest', 'validate_download_file', 'validate_download_bytes', 'validate_zip'],
  zip_layout: ['local_mod_zip_plan', 'repository_mod_zip_plan', 'inspect_mod_zip', 'online_zip_strip_root'],
  install: ['extract_zip_pending', 'copy_asset_pending', 'snapshot_path', 'restore_snapshot', 'zip_entry_destinations'],
  manifests: ['new_manifest', 'write_manifest', 'read_manifest', 'read_manifests', 'fold_legacy'],
  map_snapshots: ['read_map_relation_snapshots', 'write_map_relation_snapshots'],
  paths: ['clear_readonly', 'prune_empty_parent_dirs', 'path_relative'],
}

test('archive exports domain implementations without duplicating transport or installation', () => {
  const entry = readText(resolve(root, '../archive.rs'))
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    assert.match(entry, new RegExp(`mod ${module};`))
    for (const name of names) {
      const pattern = new RegExp(`\\bfn\\s+${name}\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(pattern)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(text, /crate::commands::/, path)
  }
})

test('archive keeps cancellation ownership and installation snapshots inside their implementations', () => {
  const control = readText(resolve(root, 'control.rs'))
  assert.equal([...control.matchAll(/static DOWNLOAD_CONTROL:/g)].length, 1)
  assert.match(control, /\.then\(\|\| AppUpdateGuard\(self\)\)/)
  assert.doesNotMatch(control, /then_some/)
  const install = readText(resolve(root, 'install.rs'))
  assert.match(install, /impl Drop for PendingModInstall[\s\S]*?self\.rollback\(\)/)
  assert.match(install, /let destinations = zip_entry_destinations\([^]*?PendingModInstall::new/)
  assert.match(install, /pending\.track_destination\(&destination\)\?;[^]*?File::create\(&destination\)/)
  for (const field of ['game_root', 'files', 'snapshots', 'created_directories']) {
    assert.doesNotMatch(install, new RegExp(`pub(?:\\([^)]*\\))?\\s+${field}:`))
  }
})
