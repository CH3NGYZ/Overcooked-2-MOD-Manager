import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const view = readFileSync(new URL('../src/components/RepositoriesView.vue', import.meta.url), 'utf8')
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const repositoryRefresh = readFileSync(new URL('../src/composables/useRepositoryRefresh.ts', import.meta.url), 'utf8')
const modsModel = readFileSync(new URL('../src/composables/useInstalledModsModel.ts', import.meta.url), 'utf8')
const repositoriesCss = readFileSync(new URL('../src/styles/repositories.css', import.meta.url), 'utf8')
const i18n = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8')

test('a repository with an installed update turns its install button into an amber update action', () => {
  assert.match(view, /function repositoryHasUpdate\(repo: Repository\)[\s\S]*?repositoryInstallStatus\.value\(repo\) === '有更新'/)
  assert.match(view, /:class="\{ 'primary-button-update': repositoryHasUpdate\(repo\) \}"/)
  assert.match(view, /repositoryHasUpdate\(repo\) \? t\('立即更新'\) : t\('下载并安装'\)/)
  assert.match(i18n, /'立即更新':/)
  assert.match(repositoriesCss, /\.repo-controls \.primary-button\.primary-button-update\s*\{[^}]*background:\s*var\(--warning\)/)
  assert.match(repositoriesCss, /\.repo-controls \.primary-button\.primary-button-update\s*\{[^}]*border-color:\s*var\(--warning\)/)
})

test('repository refresh activity drives MOD checking state, not only the explicit check batch', () => {
  assert.match(repositoryRefresh, /const modRepositoryRefreshActive = computed\(\(\) => repositoryRefreshBatchActive\.value\s*\n\s*\|\| Object\.values\(repositoryRefreshing\)\.some\(Boolean\)\)/)
  assert.match(modsModel, /active: modUpdatesCheckActive\.value \|\| modRepositoryRefreshActive\.value/)
  assert.match(modsModel, /modUpdatesCheckActive\.value \|\| modRepositoryRefreshActive\.value \|\| !hasBackendLabels/)
})

test('the update-check batch marks every mapped repository unready so queued MODs show pending', () => {
  assert.match(repositoryRefresh, /function mappedUpdateRepositories\(\)[\s\S]*?download_mode === 'release'[\s\S]*?mappedIds\.has\(repo\.id\)/)
  assert.match(repositoryRefresh, /refreshRepositoryBatch\(repositories, 4\)/)
  assert.match(repositoryRefresh, /const mapped = mappedUpdateRepositories\(\)[\s\S]*?for \(const repo of mapped\) repositoryReleaseReady\[repo\.id\] = false[\s\S]*?modUpdatesCheckActive\.value = true/)
})

test('each repository reports its own refresh state while the batch runs', () => {
  const refresh = repositoryRefresh.match(/async function refreshRepositoryEntry\(repo: import\('\.\.\/types'\)\.Repository\) \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(refresh, 'Use the actual per-repository refresh')
  assert.match(refresh, /repositoryRefreshing\[repo\.id\] = true/)
  assert.match(refresh, /repositoryReleaseReady\[repo\.id\] = false/)
  assert.match(refresh, /repositoryReleaseReady\[repo\.id\] = true/)
  assert.match(refresh, /repositoryRefreshing\[repo\.id\] = false/)
  const status = modsModel.match(/function repositoryInstallStatus\(repo: Repository\) \{([\s\S]*?)\n  \}/)?.[1]
  assert.ok(status, 'Use the actual repository card status')
  assert.match(status, /if \(repositoryRefreshing\[repo\.id\]\) return '检查中'/)
  assert.match(status, /if \(!repositoryReleaseReady\[repo\.id\][\s\S]*?return '待检查'/)
  assert.match(status, /return '有更新'/)
  assert.match(status, /return states\.every\(\(state\) => state === 'latest'\) \? '最新版' : '已安装'/)
})
