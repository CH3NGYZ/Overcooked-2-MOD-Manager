import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const view = readFileSync(new URL('../src/components/RepositoriesView.vue', import.meta.url), 'utf8')
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
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
  assert.match(app, /const modRepositoryRefreshActive = computed\(\(\) => repositoryRefreshBatchActive\.value\s*\n\s*\|\| Object\.values\(repositoryRefreshing\)\.some\(Boolean\)\)/)
  assert.match(app, /active: modUpdatesCheckActive\.value \|\| modRepositoryRefreshActive\.value/)
  assert.match(app, /modUpdatesCheckActive\.value \|\| modRepositoryRefreshActive\.value \|\| !hasBackendLabels/)
})
