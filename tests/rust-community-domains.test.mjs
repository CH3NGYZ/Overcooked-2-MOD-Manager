import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = path => readText(resolve(root, path))
const owners = {
  catalog: ['SOURCE_ID', 'GITEE_SOURCE_ID', 'REQUIRED_TOPICS', 'is_repository', 'is_source', 'source_id', 'is_source_archive', 'source', 'SearchHit', 'classify_search_item', 'repository_from_coordinates', 'SearchResults'],
  ownership: ['RepositoryIdentity', 'identity', 'official_repository_keys', 'bundled_official_repository_keys', 'visible_discoveries'],
  state: ['CommunitySearch', 'SearchState', 'SearchRateLimits', 'SEARCH_INTERVAL', 'rate_limit_delay'],
  github: ['SEARCH_ENDPOINT', 'PAGE_SIZE', 'SEARCH_QUERY', 'RESULT_LIMIT', 'MAX_RESPONSE_BYTES', 'page_url', 'fetch_page', 'SearchTarget', 'search_targets', 'fetch_page_from_targets', 'search_all'],
  merge: ['merge_results'],
  commands: ['refresh_community_repositories', 'cancel_community_search'],
}

function ordered(text, parts) {
  let previous = -1
  for (const part of parts) {
    const index = text.indexOf(part, previous + 1)
    assert.ok(index > previous, `missing or reordered: ${part}`)
    previous = index
  }
}

test('Community definitions have one real owner and IPC registers implementation directly', () => {
  const entry = read('community.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s/)
  assert.match(entry, /^pub\(crate\) mod commands;$/m)
  assert.doesNotMatch(entry, /use[^;]*(?:refresh_community_repositories|cancel_community_search)/)
  const files = sourceFiles(resolve(root, 'community'), ['.rs']).map(path => [path, readText(path)])
  for (const [owner, names] of Object.entries(owners)) {
    for (const name of names) {
      const definition = new RegExp(`^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?(?:fn|struct|enum|type|const)\\s+${name}\\b`, 'gm')
      const found = files.flatMap(([path, text]) => [...text.matchAll(definition)].map(() => path))
      assert.deepEqual(found, [resolve(root, `community/${owner}.rs`)], name)
    }
  }
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, path)
    assert.doesNotMatch(text, /crate::commands::/, path)
    if (path !== resolve(root, 'community/commands.rs')) assert.doesNotMatch(text, /#\[tauri::command\]/, path)
  }
  const lib = read('lib.rs')
  for (const name of owners.commands) assert.match(lib, new RegExp(`community::commands::${name}\\b`))
  assert.match(read('app_state.rs'), /community: Arc::new\(crate::community::CommunitySearch::default\(\)\)/)
  assert.match(read('gitee_community.rs'), /use crate::community::\{SearchRateLimits, SearchResults, repository_from_coordinates\};/)
  assert.match(read('gitee_community.rs'), /crate::community::REQUIRED_TOPICS/)
  assert.match(read('config/public.rs'), /crate::community::visible_discoveries\(config\)/)
})

test('Community discovery keeps per-platform state, trusted pagination, token isolation and ordered commit locks', () => {
  const state = read('community/state.rs')
  assert.match(state, /github: SearchState,\s*gitee: SearchState,/)
  assert.match(state, /SearchState::new\(Provider::Github\)[\s\S]*?SearchState::new\(Provider::Gitee\)/)
  assert.match(state, /pub\(super\) limits: tokio::sync::Mutex<SearchRateLimits>/)
  assert.match(state, /pub\(super\) cancelled: AtomicBool/)
  assert.match(state, /Duration::from_millis\(6100\)/)
  assert.match(state, /Provider::Gitee \| Provider::Gitcode => Duration::from_secs\(1\)/)
  const github = read('community/github.rs')
  ordered(github, ['limits.before_request(route).await?', '.timeout(Duration::from_secs(5))', 'limits.observe(route, status, response.headers())', 'bytes.len() + chunk.len() > MAX_RESPONSE_BYTES'])
  assert.match(github, /MAX_RESPONSE_BYTES: usize = 2 \* 1024 \* 1024/)
  assert.match(github, /SEARCH_QUERY: &str = "topic:overcooked-2 topic:bepinex"/)
  assert.match(github, /route: "official"\.into\(\)/)
  assert.match(github, /token: None,\s*route: format!\("proxy-\{\}", proxy.id\)/)
  assert.match(github, /let official = page_url\(endpoint, page\)\?/)
  assert.match(github, /processed < results.discovery.total_count.min\(RESULT_LIMIT as u64\)/)
  const commands = read('community/commands.rs')
  ordered(commands, ['.try_lock()', 'cancelled.store(false, Ordering::Release)', 'let config = {', 'let results = tokio::select!', 'state.repository_cache.lock().await', 'state.operation.lock()', 'state.config.load_or_default()', 'merge_results(&mut config', '.save(&config)'])
  assert.match(commands, /search.cancelled.load\(Ordering::Acquire\)/)
  assert.match(commands, /crate::gitee_community::search_all\(&app, state.config.cache_dir\(\).join\("gitee-webview"\), &mut limits\)/)
  const ownership = read('community/ownership.rs')
  assert.match(ownership, /keys.extend\(bundled_official_repository_keys\(\)\)/)
  assert.match(ownership, /discovery.clone\(\).map\(\|mut result\|/)
  const merge = read('community/merge.rs')
  assert.match(merge, /repo.enabled = old.enabled/)
  assert.match(merge, /\.retain\(\|repository\| repository.provider != \*provider\)/)
  for (const [name, path] of [['item', 'test_support'], ['page', 'test_support'], ['server', 'test_support'], ['limits', 'state/tests']]) {
    const files = sourceFiles(resolve(root, 'community'), ['.rs'])
    const found = files.filter(file => new RegExp(`\\bfn ${name}\\s*\\(`).test(readText(file)))
    assert.deepEqual(found, [resolve(root, `community/${path}.rs`)], `shared original fixture ${name}`)
  }
})
