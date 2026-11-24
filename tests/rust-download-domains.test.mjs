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

test('Download entrypoints, coordination, progress and speed sampling have one owner each', () => {
  const download = read('download.rs')
  const race = read('download/race.rs')
  const tests = read('download/tests.rs')
  assert.ok(download.split('\n').length < 500)
  assert.ok(race.split('\n').length < 500)
  assert.match(download, /^pub\(crate\) mod planning;$/m)
  assert.match(download, /^mod race;$/m)
  assert.match(download, /^mod race_speed;$/m)
  assert.match(download, /^#\[cfg\(test\)\]\s*mod tests;$/m)
  assert.doesNotMatch(download, /^\s*#\[(?:tokio::)?test\]/m)
  assert.match(tests, /^use super::\*;/m)
  const files = [
    [resolve(root, 'download.rs'), read('download.rs')],
    ...sourceFiles(resolve(root, 'download'), ['.rs']).map(path => [path, readText(path)]),
  ]
  for (const declaration of [
    'struct DownloadCandidate', 'struct DownloadContext',
    'trait DownloadOutput', 'fn try_download_candidates',
    'fn try_download_candidates_with_progress', 'fn try_download_candidates_until_error',
  ]) owner(files, declaration, 'download.rs')
  owner(files, 'fn race_candidates', 'download/race.rs')
  owner(files, 'struct RaceState', 'download/race.rs')
  owner(files, 'fn leading_source', 'download/race_speed.rs')
  owner(files, 'fn progress_keeps_the_leader_until_another_file_overtakes_it', 'download/tests.rs')
  owner(files, 'fn cancellation_interrupts_pending_streams_and_removes_all_partial_files', 'download/tests.rs')
  owner(files, 'fn explicit_lock_pauses_on_failure_then_retries_or_unlocks_or_cancels', 'download/race/tests.rs')
  assert.match(read('download/race/progress.rs'), /impl RaceState \{\s*pub\(super\) fn progress\(/)
  owner(files, 'struct SourceSpeed', 'download/race_speed.rs')
  owner(files, 'fn switching_candidate', 'download/race_speed.rs')
  owner(files, 'fn probe_should_stop', 'download/race_speed.rs')
})

test('Download race invariants and shared callers remain on the existing seam', () => {
  const download = read('download.rs')
  const race = read('download/race.rs')
  assert.match(download, /parallelism: 3/)
  assert.match(download, /soft_start: Duration::from_secs\(10\)\.min\(network\.first_byte_timeout\(\) \/ 2\)/)
  assert.match(race, /session\.map_or\(policy\.window, \|_\| Duration::from_secs\(2\)\)/)
  assert.match(race, /session\.verified_speed\(route\)/)
  assert.match(download, /if !execution\.race \{\s*self\.reference_speed = 0;/)
  assert.match(race, /self\.sources\.len\(\) \+ self\.retiring\.len\(\) < self\.limit/)
  assert.match(race, /let Some\(session\) = session else \{[\s\S]*?let Some\(\(id, fastest\)\) = switching_candidate\(/)
  assert.match(race, /terminal_error == Some\(error\.as_str\(\)/)
  assert.match(download, /value\.discard\(\)/)
  for (const file of [
    'direct_mod.rs', 'app_update/download.rs', 'app_update/manifest.rs',
    'app_update/channels.rs', 'mods/repository_download.rs', 'commands/map_install.rs',
    'commands/bepinex.rs', 'repository_sources/verified.rs', 'avatars.rs', 'repository_readme.rs',
  ]) assert.match(read(file), /(?:crate::download|use crate::download)/, file)
  assert.match(read('download/planning.rs'), /use super::DownloadCandidate/)
  assert.match(download, /#\[path = "download_retention_tests\.rs"\]/)
  assert.match(download, /mod retention_tests;/)
})

test('Download entrypoints receive explicit network snapshots including calls without progress context', () => {
  const download = read('download.rs')
  for (const name of ['try_download_candidates', 'try_download_candidates_with_progress']) {
    assert.match(download, new RegExp(`fn ${name}<[^>]+>\\([\\s\\S]*?network: &NetworkConfig,\\s*context: Option<DownloadContext<'_>>`))
  }
  assert.match(download, /fn try_download_candidates_until_error<[^>]+>\([\s\S]*?terminal_error: &'static str,\s*network: &NetworkConfig,\s*execution: DownloadExecution,\s*\)/)
  assert.match(download, /try_download_candidates_with_progress\(\s*candidates,\s*is_cancelled,\s*download,\s*network,\s*context,/)
  assert.match(download, /let policy = RacePolicy::from_network\(network\)\s*\.with_execution\(context/)
  assert.match(download, /terminal_error: Some\(terminal_error\),\s*\.\.RacePolicy::from_network\(network\)\.with_execution\(Some\(execution\)\)/)
  assert.match(download, /impl Default for RacePolicy[\s\S]*?Self::from_network\(&NetworkConfig::default\(\)\)/)
})
