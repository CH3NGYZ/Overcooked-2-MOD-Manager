import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const base = resolve(root, 'map_note')
const read = path => readText(resolve(root, path))
const files = sourceFiles(base, ['.rs']).map(path => [path, readText(path)])

const owners = {
  metadata: ['struct MapNoteMetadata', 'fn normalize_version'],
  relations: ['enum RelationSpec', 'struct NoteReleaseRelations', 'fn push_relations'],
  block: ['const NOTE_BLOCK_OPEN', 'const NOTE_BLOCK_CLOSE', 'struct NoteBlock', 'fn parse_release_note', 'fn strip_note_block', 'fn block_span'],
  assets: ['const VERSION_PLACEHOLDER', 'const VERSION_PATTERN', 'fn versioned_asset_rule', 'fn template_rule'],
  publish: ['fn publish_metadata_from_note', 'fn canonical_range', 'fn render_metadata_block', 'fn replace_body_keeping_block', 'fn render_release_body', 'fn render_inline_table', 'fn toml_string'],
}

function onlyOwner(owner, declaration) {
  const pattern = new RegExp(`^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?${declaration.replace(/[.*+?^${}()|[\\]\\\\]/g, '\\$&')}\\b`, 'gm')
  const found = files.flatMap(([path, source]) => [...source.matchAll(pattern)].map(() => path))
  assert.deepEqual(found, [resolve(base, `${owner}.rs`)], declaration)
}

test('Map-note root exports the real owners without retaining duplicate logic', () => {
  const entry = read('map_note.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  for (const owner of Object.keys(owners)) assert.match(entry, new RegExp(`^mod ${owner};$`, 'm'))
  for (const [owner, declarations] of Object.entries(owners)) {
    for (const declaration of declarations) onlyOwner(owner, declaration)
  }
  assert.match(entry, /pub use metadata::MapNoteMetadata;/)
  assert.match(entry, /NOTE_BLOCK_CLOSE, NOTE_BLOCK_OPEN, NoteBlock, parse_release_note, strip_note_block/)
  assert.match(entry, /pub use assets::\{VERSION_PLACEHOLDER, versioned_asset_rule\};/)
  for (const exported of ['publish_metadata_from_note', 'render_metadata_block', 'render_release_body', 'replace_body_keeping_block']) assert.ok(entry.includes(exported), exported)
  assert.match(entry, /pub use relations::\{NoteReleaseRelations, RelationSpec\};/)
  assert.equal(files.filter(([path, source]) => path.endsWith('test_support.rs') && /\bfn body\s*\(/.test(source)).length, 1)
  for (const [path, source] of files) assert.ok(source.trimEnd().split('\n').length < 1000, path)
})

test('Map-note parser, compatibility rules and writer cases remain covered by their owners', () => {
  const metadata = read('map_note/metadata.rs')
  assert.match(metadata, /alias = "latest_version"/)
  assert.match(metadata, /asset_versions\.last\(\)/)
  const relations = read('map_note/relations.rs')
  assert.match(relations, /VersionRange::parse\(&raw\)/)
  assert.match(relations, /range\.is_any\(\)/)
  assert.match(relations, /optional: optional && matches!\(kind, RelationKind::Requires\)/)
  const block = read('map_note/block.rs')
  assert.match(block, /toml::from_str\(toml_text\)/)
  assert.match(block, /metadata\.level_set_name\.trim\(\)\.is_empty\(\)/)
  assert.match(block, /Some\(\(start, end\)\) => format!/)
  const assets = read('map_note/assets.rs')
  assert.match(assets, /template\.contains\(VERSION_PLACEHOLDER\)/)
  assert.match(assets, /regex::escape\(/)
  assert.match(assets, /pattern\.push\('\$'\)/)
  const publish = read('map_note/publish.rs')
  assert.ok(publish.indexOf('block.push_str("asset_versions = [\\n")') < publish.indexOf('for entry in &metadata.release_relations'))
  assert.match(publish, /let Some\(\(start, end\)\) = block_span\(remote\) else/)
  const tests = sourceFiles(base, ['.rs']).filter(path => path.endsWith('tests.rs')).map(path => readText(path)).join('\n')
  assert.equal([...tests.matchAll(/#\[test\]/g)].length, 24, 'all migrated Rust regression tests remain present')
  for (const name of [
    'the_legacy_latest_version_field_still_parses',
    'an_empty_table_tail_means_unknown_instead_of_borrowing_older_versions',
    'a_bad_relation_is_skipped_instead_of_dropping_the_map',
    'release_relations_override_the_relations_for_that_version',
    'replacing_the_body_keeps_the_existing_block',
    'the_asset_template_becomes_an_anchored_regex',
  ]) assert.match(tests, new RegExp(`fn ${name}\\s*\\(`), name)
})
