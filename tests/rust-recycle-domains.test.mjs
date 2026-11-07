import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/recycle_bin')
const owners = {
  paths: ['validate_absolute_path', 'validate_relative', 'checked_path', 'existing_variants'],
  file_io: ['fingerprint', 'move_file', 'copy_verified_and_remove', 'rollback_moves', 'write_atomic'],
  records: ['save_record', 'load_record', 'list', 'original_game_path'],
  manifests: ['read_active_manifest', 'set_active_manifest', 'original_manifest', 'merge_restored_manifest'],
  selection: ['recycle_package_with_mods', 'recycle_selected_mod', 'has_safe_replacement'],
  repository: ['recycle_repository_packages_before_install', 'dll_matches_repository', 'same_package_dir'],
  staging: ['stage', 'stage_with_related_manifests'],
  restore: ['restore', 'validate_payload'],
  delete: ['delete', 'delete_all', 'remove_entry_directory', 'purge_entry'],
}

test('recycle domain keeps one implementation per responsibility and internal records private', () => {
  const entry = readText(resolve(root, '../recycle_bin.rs'))
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  assert.doesNotMatch(entry, /pub(?:\([^)]*\))? use records::[^;]*(?:RecycleRecord|StagedFile|RecordState|RelatedManifest)/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
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

test('recycle keeps journal-before-move and restore validation-before-move ordering', () => {
  const staging = readText(resolve(root, 'staging.rs'))
  const journal = staging.indexOf('save_record(&directory, &record)?;')
  const move = staging.indexOf('move_file(')
  const manifest = staging.indexOf('set_active_manifest(manifests_dir, &manifest.repository_id, remaining_manifest)?;')
  assert.ok(journal >= 0 && move > journal && manifest > move)
  assert.match(staging, /if files_rolled_back && manifest_rolled_back/)
  const restore = readText(resolve(root, 'restore.rs'))
  const firstMove = restore.indexOf('move_file(')
  for (const check of ['validate_payload(&payload, &record)?;', 'merge_restored_manifest(', 'matches_fingerprint(&staged, file)?', 'existing_variants(game_root, &canonical)?']) {
    assert.ok(restore.indexOf(check) >= 0 && restore.indexOf(check) < firstMove, check)
  }
  assert.match(restore, /restore_plans\[\.\.manifests_changed\]\.iter\(\)\.rev\(\)/)
  assert.match(readText(resolve(root, 'delete.rs')), /follow_links\(false\)/)
})
