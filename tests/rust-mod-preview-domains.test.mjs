import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = path => readText(resolve(root, path))
const ownerFiles = [
  [resolve(root, 'mod_preview.rs'), read('mod_preview.rs')],
  ...sourceFiles(resolve(root, 'mod_preview'), ['.rs']).map(path => [path, readText(path)]),
]

function assertOwner(declaration, expected) {
  const escaped = declaration.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(`^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?${escaped}\\b`, 'gm')
  const found = ownerFiles.flatMap(([path, text]) => [...text.matchAll(pattern)].map(() => path))
  assert.deepEqual(found, [resolve(root, expected)], declaration)
}

test('MOD preview production definitions have one responsibility owner', () => {
  for (const [declaration, expected] of [
    ['struct ModPreview', 'mod_preview/model.rs'],
    ['struct ModPreviewEntry', 'mod_preview/model.rs'],
    ['enum PreviewOrigin', 'mod_preview/model.rs'],
    ['enum ModSourceKind', 'mod_preview/model.rs'],
    ['struct ModSource', 'mod_preview/model.rs'],
    ['fn analyze_zip', 'mod_preview/analysis.rs'],
    ['fn build_preview', 'mod_preview/analysis.rs'],
    ['fn apply_repository_origin', 'mod_preview/analysis.rs'],
    ['fn find_install_conflicts', 'mod_preview/conflicts.rs'],
    ['fn same_plugin', 'mod_preview/conflicts.rs'],
    ['fn preview_root', 'mod_preview/staging.rs'],
    ['fn stage_downloaded_file', 'mod_preview/staging.rs'],
    ['fn analyze_mod_source', 'mod_preview/commands.rs'],
    ['fn cancel_mod_preview', 'mod_preview/commands.rs'],
    ['fn confirm_mod_install', 'mod_preview/commands.rs'],
  ]) assertOwner(declaration, expected)
  for (const path of [
    'mod_preview.rs', 'mod_preview/analysis.rs', 'mod_preview/commands.rs',
    'mod_preview/conflicts.rs', 'mod_preview/model.rs', 'mod_preview/staging.rs',
    'mod_preview/tests.rs',
  ]) assert.ok(read(path).split('\n').length < 1000, path)
  assert.match(read('mod_preview.rs'), /^pub use model::\{ModPreview, ModPreviewEntry, ModSource, ModSourceKind, PreviewOrigin\};$/m)
  assert.match(read('mod_preview.rs'), /^pub use commands::\{analyze_mod_source, cancel_mod_preview, confirm_mod_install\};$/m)
  assert.match(read('mod_preview.rs'), /^pub use conflicts::find_install_conflicts;$/m)
})

test('MOD preview preserves analysis, conflict, staging and Tauri command contracts', () => {
  const model = read('mod_preview/model.rs')
  const analysis = read('mod_preview/analysis.rs')
  const conflicts = read('mod_preview/conflicts.rs')
  const staging = read('mod_preview/staging.rs')
  const commands = read('mod_preview/commands.rs')
  const lib = read('lib.rs')
  assert.match(model, /#\[serde\(default\)\]\s*pub conflicts: Vec<InstallConflict>/)
  assert.match(analysis, /MAX_DLL_BYTES/)
  assert.match(analysis, /read_metadata_batch/)
  assert.match(analysis, /kind: if total_dlls == 1 \{ "single" \} else \{ "package" \}/)
  assert.match(conflicts, /entry_guid\.eq_ignore_ascii_case\(installed_guid\)/)
  assert.match(conflicts, /compare_mod_versions\(target, local\)/)
  assert.match(conflicts, /HashSet<String>/)
  assert.match(staging, /const PREVIEW_TTL: Duration = Duration::from_secs\(60 \* 60\)/)
  assert.match(staging, /const ORIGIN_FILE: &str = "origin.json"/)
  assert.match(commands, /Uuid::parse_str\(value\.trim\(\)\)/)
  assert.match(commands, /spawn_blocking\(move \|\| confirm_mod_install_blocking/)
  assert.match(commands, /install_staged_mod_repository/)
  assert.match(commands, /install_manual_mod/)
  const registered = [...lib.matchAll(/mod_preview::commands::(analyze_mod_source|confirm_mod_install|cancel_mod_preview)/g)].map(match => match[1])
  assert.deepEqual(registered, ['analyze_mod_source', 'confirm_mod_install', 'cancel_mod_preview'])
  assert.equal([...read('mod_preview/tests.rs').matchAll(/#\[(?:tokio::)?test\]/g)].length, 17)
})
