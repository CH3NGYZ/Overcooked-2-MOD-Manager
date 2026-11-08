import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src')
const read = path => readText(resolve(root, path))

function owner(files, declaration, expected) {
  const pattern = new RegExp(`^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?${declaration.replace(/[.*+?^${}()|[\\]\\\\]/g, '\\$&')}\\b`, 'gm')
  const found = files.flatMap(([path, text]) => [...text.matchAll(pattern)].map(() => path))
  assert.deepEqual(found, [resolve(root, expected)], declaration)
}

test('Download root keeps the deep race coordinator and externalizes only its tests', () => {
  const download = read('download.rs')
  const tests = read('download/tests.rs')
  assert.ok(download.split('\n').length < 500)
  assert.match(download, /^pub\(crate\) mod planning;$/m)
  assert.match(download, /^#\[cfg\(test\)\]\s*mod tests;$/m)
  assert.doesNotMatch(download, /^\s*#\[(?:tokio::)?test\]/m)
  assert.match(tests, /^use super::\*;/m)
  assert.equal([...tests.matchAll(/#\[(?:tokio::)?test\]/g)].length, 14)
  const files = [
    [resolve(root, 'download.rs'), read('download.rs')],
    ...sourceFiles(resolve(root, 'download'), ['.rs']).map(path => [path, readText(path)]),
  ]
  for (const declaration of [
    'struct DownloadCandidate', 'fn candidate_progress', 'struct DownloadContext',
    'trait DownloadOutput', 'fn try_download_candidates',
    'fn try_download_candidates_until_error', 'fn race_candidates',
  ]) owner(files, declaration, 'download.rs')
  owner(files, 'fn progress_keeps_the_leader_until_another_file_overtakes_it', 'download/tests.rs')
  owner(files, 'fn cancellation_interrupts_pending_streams_and_removes_all_partial_files', 'download/tests.rs')
})

test('Download race invariants and shared callers remain on the existing seam', () => {
  const download = read('download.rs')
  assert.match(download, /parallelism: 3/)
  assert.match(download, /window: Duration::from_secs\(5\)/)
  assert.match(download, /minimum_speed: 256 \* 1024/)
  assert.match(download, /let replacements = pending\.len\(\)\.saturating_sub\(retiring\.len\(\)\)/)
  assert.match(download, /terminal_error == Some\(error\.as_str\(\)/)
  assert.match(download, /value\.discard\(\)/)
  for (const file of [
    'direct_mod.rs', 'app_update/download.rs', 'app_update/manifest.rs',
    'app_update/channels.rs', 'commands/repository_install.rs', 'commands/map_install.rs',
    'commands/bepinex.rs', 'repository_sources/verified.rs', 'avatars.rs', 'repository_readme.rs',
  ]) assert.match(read(file), /(?:crate::download|use crate::download)/, file)
  assert.match(read('download/planning.rs'), /use super::DownloadCandidate/)
  assert.match(download, /#\[path = "download_retention_tests\.rs"\]/)
  assert.match(download, /mod retention_tests;/)
})
