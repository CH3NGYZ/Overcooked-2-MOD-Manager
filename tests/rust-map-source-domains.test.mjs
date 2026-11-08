import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/map_sources')
const read = path => readText(resolve(root, path))
const owners = {
  document: ['parse_document', 'validate_declaration', 'apply_declaration_coordinates', 'validate_map_entry', 'build_declaration_map'],
  identity: ['map_identity', 'map_repository_id', 'map_tag', 'map_label'],
  assets: ['version_rule', 'synthesize_releases', 'asset_name_for_version', 'matched_version', 'newest_first'],
  state: ['replace_maps_keeping_runtime_state', 'merge_maps_keeping_runtime_state', 'map_is_available', 'available_maps'],
  tree: ['bundled_map_tree', 'load_map_tree', 'load_map_document', 'fill_missing_source_names'],
  community: ['community_map_declaration', 'append_community_map_maps'],
  release_tags: ['load_release_tag_maps', 'note_block_map'],
  transport: ['load_document', 'verified_document'],
  context: ['token', 'api_proxies', 'previous_for'],
  releases: ['release_probe', 'fetch_map_releases', 'select_map_stage_asset'],
  view: ['map_to_info'],
}

test('map source responsibilities have one owner and preserve the existing DTO projection', () => {
  const entry = read('../map_sources.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  for (const [module, names] of Object.entries(owners)) {
    assert.match(entry, new RegExp(`mod ${module};`))
    for (const name of names) {
      const declaration = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(`, 'g')
      const found = files.flatMap(([path, source]) => [...source.matchAll(declaration)].map(() => path))
      assert.deepEqual(found, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, source] of files) {
    assert.ok(source.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(source, /crate::commands::/, path)
  }
})

test('map loading shares transport and cache while retaining tag assets and runtime state rules', () => {
  const transport = read('transport.rs')
  assert.match(transport, /crate::repository_sources::load_verified_text_document\(/)
  assert.match(transport, /crate::repository_sources::load_text_document\(/)
  assert.match(transport, /crate::download::planning::plan_remote_download_candidates\(/)
  for (const file of ['transport.rs', 'release_tags.rs', 'releases.rs']) {
    assert.match(read(file), /\.cache\s*\.list\(key, ctx\.pass_id,/)
  }
  const tags = read('release_tags.rs')
  assert.match(tags, /crate::releases::repository_cache_key/)
  assert.doesNotMatch(tags, /"\{\}::all"/)
  assert.match(tags, /crate::releases::fetch_releases\(/)
  assert.match(tags, /ctx\.previous_for\(&label\.id\)/)
  assert.match(read('community.rs'), /load_release_tag_maps\(&declaration, &label, false, origin, ctx\)/)
  const state = read('state.rs')
  for (const field of ['cached_releases', 'releases_cached_at', 'last_error']) {
    assert.match(state, new RegExp(`map\\.${field} = old\\.${field}`))
  }
  assert.doesNotMatch(state, /map\.enabled\s*=\s*old\.enabled/)
  const assets = read('assets.rs')
  assert.match(assets, /if map\.asset_versions\.is_empty\(\)\s*\{\s*for asset in &release\.assets/)
  assert.match(assets, /map\.asset_versions\.iter\(\)\.rev\(\)/)
  assert.match(read('releases.rs'), /if asset_name\.is_none\(\) && map_tag\(map\)\.is_some\(\)\s*\{\s*return release\.assets\.first\(\);/)
  assert.doesNotMatch(read('context.rs'), /static\s+|Mutex|RwLock/)
})
