import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/mods')
const read = path => readText(resolve(root, path))
const owners = {
  metadata: ['read_dll_details', 'parse_bepin_plugin_metadata', 'read_serialized_string', 'placeholder_dll_details', 'choose_local_version'],
  scan: ['scan_mods_with_manifests', 'inspect_dll', 'annotate_package_counts'],
  binding: ['repository_matches_mod', 'select_repository', 'inherit_repository_from_sibling'],
  status: ['collect_update_status', 'bound_release', 'effective_local_version', 'binding_update_state', 'repository_update_state', 'refresh_status_labels'],
  identity: ['compare_mod_versions', 'mark_duplicate_guids', 'is_safe_replacement_mod', 'note_key'],
  relations: ['select_release_relations', 'effective_relations', 'effective_map_relations'],
  file_state: ['plugin_dll_files', 'canonical_dll_file', 'canonical_dll_name', 'disabled_dll_file', 'set_local_mod_file_state'],
}

test('MOD domain exports actual implementations and extends existing state and status owners', () => {
  const entry = read('../mods.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    assert.match(entry, new RegExp(`mod ${module};`))
    for (const name of names) {
      const pattern = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(`, 'g')
      const found = files.flatMap(([path, text]) => [...text.matchAll(pattern)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(text, /crate::commands::/, path)
  }
})

test('MOD scan keeps the shared metadata batch and distinct local versus Release version rules', () => {
  const scan = read('scan.rs')
  const assembly = scan.slice(scan.indexOf('fn inspect_dll('), scan.indexOf('fn annotate_package_counts('))
  assert.match(scan, /read_metadata_batch\(&requests, workers,/)
  assert.match(assembly, /MetadataValue::version/)
  assert.doesNotMatch(assembly, /read_dll_details\(|parse_bepin_plugin_metadata\(/)
  const order = ['annotate_package_counts(&mut installed)', 'inherit_repository_from_sibling(&mut installed)', 'mark_duplicate_guids(&mut installed)', 'refresh_status_labels(&mut installed)']
  const positions = order.map(call => scan.indexOf(call))
  assert.ok(positions.every((position, i) => position >= 0 && (i === 0 || position > positions[i - 1])))
  for (const path of ['scan.rs', 'file_state.rs']) {
    assert.match(read(path), /registered_plugin_paths/)
    assert.match(read(path), /legacy_disabled_root/)
    assert.match(read(path), /levels_root/)
  }
  assert.match(read('identity.rs'), /parse_mod_version_components\(left\)\?\.cmp\(&parse_mod_version_components\(right\)\?\)/)
  assert.match(read('status.rs'), /crate::releases::compare_release_versions\(dll, binding_tag\)/)
  assert.match(read('relations.rs'), /crate::version_range::VersionRange::parse/)
  assert.doesNotMatch(read('metadata.rs'), /static\s+\w+\s*:\s*(?:OnceLock|Mutex)/)
})
