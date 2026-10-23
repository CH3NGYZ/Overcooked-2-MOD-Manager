import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const view = readFileSync(new URL('../src/components/RepositoriesView.vue', import.meta.url), 'utf8')
const description = readFileSync(new URL('../src/components/RepositoryDescription.vue', import.meta.url), 'utf8')
const repositoriesCss = readFileSync(new URL('../src/styles/repositories.css', import.meta.url), 'utf8')
const overviewCss = readFileSync(new URL('../src/styles/overview.css', import.meta.url), 'utf8')
const i18n = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8')

test('repository cards align their icon and content to the top', () => {
  assert.match(repositoriesCss, /\.repository-row\s*\{[^}]*align-items:\s*flex-start/)
  assert.match(overviewCss, /\.repo-logo\s*\{[^}]*align-self:\s*flex-start/)
})

test('a faint divider separates the repository title from its description', () => {
  assert.match(repositoriesCss, /\.repo-note\s*\{[^}]*border-top:\s*1px solid var\(--line\)/)
  assert.match(repositoriesCss, /\.repo-note\s*\{[^}]*padding-top:/)
})

test('the README action is a single icon button that reuses the release-log slot', () => {
  assert.match(view, /:title="t\('查看仓库自述文件'\)"[\s\S]{0,120}@click="\$emit\('show-repository-readme', repo\)"/)
  assert.match(view, /<FileText :size="14" \/>/)
  assert.doesNotMatch(view, /t\('查看仓库说明'\)/, 'The old README text button must be gone')
  assert.doesNotMatch(description, /emit\('readme'\)/, 'The note component must not render a README button')
  assert.match(i18n, /'查看仓库自述文件':/)
})

test('the release version block keeps the release-log entry point', () => {
  assert.match(view, /<button v-if="repo\.download_mode === 'release'" type="button" class="repo-release"[\s\S]{0,180}@click="\$emit\('show-release', repo\)"/)
  assert.match(view, /:title="t\('查看 Release 日志'\)"/)
  assert.match(repositoriesCss, /\.repo-release\s*\{[^}]*appearance:\s*none/)
})
