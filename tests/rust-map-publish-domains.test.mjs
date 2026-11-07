import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/map_publish')
const read = path => readText(resolve(root, path))
const commands = [
  ['authorization', 'start_github_authorization'],
  ['authorization', 'poll_github_authorization'],
  ['authorization', 'cancel_github_authorization'],
  ['session', 'map_publish_state'],
  ['session', 'check_publish_session'],
  ['session', 'sign_out_map_publish'],
  ['repositories', 'list_publish_repositories'],
  ['releases', 'list_publish_releases'],
  ['releases', 'save_publish_release'],
  ['releases', 'delete_publish_release'],
  ['assets', 'upload_publish_asset'],
  ['assets', 'delete_publish_asset'],
  ['zip', 'inspect_publish_zip'],
  ['zip', 'stage_publish_zip'],
  ['repositories', 'publish_repository_topics'],
]
const owners = {
  authorization: ['authorization_target', 'start_github_authorization', 'poll_github_authorization', 'cancel_github_authorization'],
  'authorization/callback': ['bind_callback_listener', 'spawn_callback_listener', 'handle_callback', 'parse_query'],
  credentials: ['tokens_from_response', 'client_secret', 'exchange_code', 'fetch_login'],
  session: ['publish_token', 'check_publish_session', 'check_session_once', 'refresh_tokens', 'store_refreshed_tokens'],
  http: ['api_client', 'upload_client', 'authorized', 'classify_write_failure', 'ensure_success', 'get_json'],
  validation: ['validate_coordinates', 'validate_tag', 'validate_new_tag', 'validate_asset_name', 'encode_component'],
  repositories: ['list_publish_repositories', 'remembered_installation_repositories', 'writable_repositories', 'publish_repository_topics'],
  releases: ['release_from_api', 'save_publish_release', 'delete_publish_release', 'validate_publish_relations'],
  assets: ['upload_publish_asset', 'delete_publish_asset', 'validate_zip_identity', 'upload_asset_name'],
  zip: ['inspect_publish_zip', 'inspect_publish_zip_blocking', 'stage_publish_zip', 'stage_publish_zip_into'],
}

test('map publish IPC points at bounded domain owners with no compatibility forwarding', () => {
  const entry = read('../map_publish.rs').replace(/^\/\/.*$/gm, '').trim()
  assert.match(entry, /^(?:(?:pub\(crate\) )?mod [a-z_]+;\s*)+$/)
  const paths = [...read('../lib.rs').matchAll(/\bmap_publish::([a-z_]+)::([a-z_]+),/g)]
  assert.deepEqual(paths.map(([, domain, name]) => [domain, name]), commands)
  const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])
  const declared = files.flatMap(([path, source]) =>
    [...source.matchAll(/#\[tauri::command\]\s*pub (?:async )?fn ([a-z_]+)\(/g)].map(([, name]) => [path, name]))
  assert.deepEqual(declared.map(([, name]) => name).sort(), commands.map(([, name]) => name).sort())
  for (const [module, names] of Object.entries(owners)) {
    for (const name of names) {
      const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^>]*>)?\\s*\\(`, 'g')
      const actual = files.flatMap(([path, source]) => [...source.matchAll(definition)].map(() => path))
      assert.deepEqual(actual, [resolve(root, `${module}.rs`)], name)
    }
  }
  for (const [path, source] of files) {
    assert.ok(source.trimEnd().split('\n').length < 1000, `${path} is a replacement monolith`)
    assert.doesNotMatch(source, /crate::commands::/, path)
  }
})

test('publish authorization, session rotation and ZIP upload retain their shared boundaries', () => {
  const files = [read('../map_publish.rs'), ...sourceFiles(root, ['.rs']).map(readText)]
  for (const [name, owner] of [['PENDING_AUTH', 'authorization.rs'], ['SESSION_LOCK', 'session.rs']]) {
    const definition = new RegExp(`\\bstatic ${name}:`, 'g')
    assert.equal(files.reduce((count, source) => count + [...source.matchAll(definition)].length, 0), 1)
    assert.match(read(owner), definition)
  }
  const auth = read('authorization.rs')
  for (const [before, after] of [
    ['let listener = bind_callback_listener()?', 'crate::shell_open::open_url('],
    ['*guard = CallbackOutcome::Exchanging', 'match exchange_code('],
  ]) {
    assert.ok(auth.indexOf(before) >= 0 && auth.indexOf(before) < auth.indexOf(after), before)
  }
  const session = read('session.rs')
  assert.match(session, /let _session = SESSION_LOCK\.lock\(\)\.await;\s*let outcome = check_session_once/)
  assert.match(session, /store_refreshed_tokens\(state, &tokens, &login\)\?;\s*if let TokenProbe::Valid\(fresh\) = probe_token/)
  assert.match(session, /RefreshResult::Unavailable\(reason\) => \{\s*log::warn!\([^;]*\);\s*Ok\(PublishSessionOutcome::Unavailable \{ reason \}\)\s*\}/s)
  const zip = read('zip.rs')
  assert.match(zip, /#\[path = "\.\.\/map_publish_zip_cache\.rs"\]\s*pub\(super\) mod zip_cache;/)
  assert.match(zip, /crate::level_sets::inspect_zip_with_progress\(/)
  assert.match(zip, /spawn_blocking\(move \|\| zip_cache::inspect\(&path\)\)/)
  const assets = read('assets.rs')
  assert.match(assets, /spawn_blocking\(move \|\| zip_cache::inspect\(&inspect_path\)\)/)
  assert.match(assets, /validate_zip_identity\(note\.as_ref\(\)\.map\(\|note\| &note\.metadata\), &inspection\)\?;/)
  assert.match(assets, /&inspection\.info_version/)
  assert.match(assets, /reqwest::Body::wrap_stream\(stream\)/)
  assert.match(assets, /phase: if sent >= total \{\s*"confirming"/)
  const confirmed = assets.indexOf('ensure_success(response, "upload-asset")')
  assert.ok(confirmed >= 0 && confirmed < assets.indexOf('phase: "done"'))
  assert.doesNotMatch(assets, /stage_publish_zip|std::fs::read\(/)
  const cache = read('../map_publish_zip_cache.rs')
  assert.match(cache, /inspect_with\(&CACHE, path, super::inspect_publish_zip_blocking\)/)
  assert.match(cache, /const CAPACITY: usize = 8;/)
  assert.match(read('releases.rs'), /replace_body_keeping_block\(&remote\.body, &request\.body\)/)
  assert.match(read('releases.rs'), /render_release_body\(&metadata, &request\.body\)/)
})
