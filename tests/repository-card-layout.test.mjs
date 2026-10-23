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

test('every repository card exposes a single README icon button', () => {
  // No download-mode or community gate: repository-file cards get the icon too.
  assert.match(view, /<button class="icon-button small" :title="t\('查看仓库自述文件'\)"\s*\n\s*@click="\$emit\('show-repository-readme', repo\)"/)
  assert.match(view, /<FileText :size="14" \/>/)
  assert.doesNotMatch(view, /t\('查看仓库说明'\)/, 'The old README text button must be gone')
  assert.doesNotMatch(description, /emit\('readme'\)/, 'The note component must not render a README button')
  assert.match(i18n, /'查看仓库自述文件':/)
})

test('the per-card refresh and external release-page icons stay removed', () => {
  assert.doesNotMatch(view, /t\('刷新 Release'\)/)
  assert.doesNotMatch(view, /t\('打开发布页'\)/)
  assert.doesNotMatch(view, /ExternalLink/)
  assert.doesNotMatch(i18n, /'刷新 Release':/)
  assert.doesNotMatch(i18n, /'打开发布页':/)
})

test('repository rows shrink instead of pushing the right-hand controls out', () => {
  assert.match(repositoriesCss, /\.repository-row\s*\{[^}]*min-width:\s*0/)
  assert.doesNotMatch(repositoriesCss, /\.repo-release\s*\{[^}]*min-width:\s*90px/)
  assert.match(repositoriesCss, /\.repo-controls\s*\{[^}]*grid-template-columns:\s*minmax\(0, 1fr\)/)
  assert.match(repositoriesCss, /\.row-actions\s*\{[^}]*flex-wrap:\s*wrap/)
  assert.match(repositoriesCss, /\.repo-release b,\s*\.repo-release span\s*\{[^}]*text-overflow:\s*ellipsis/)
  assert.match(repositoriesCss, /\.repo-controls \.primary-button\s*\{[^}]*white-space:\s*normal/)
  // The surrounding grid must also allow shrink, otherwise the row min-content
  // would still widen the card and clip the controls.
  assert.match(repositoriesCss, /\.repository-group-list\s*\{[^}]*min-width:\s*0/)
  assert.match(repositoriesCss, /\.repository-group\s*\{[^}]*min-width:\s*0/)
  assert.match(repositoriesCss, /\.repository-group-items\s*\{[^}]*min-width:\s*0/)
})

test('the release version block keeps the release-log entry point', () => {
  assert.match(view, /<button v-if="repo\.download_mode === 'release'" type="button" class="repo-release"[\s\S]{0,180}@click="\$emit\('show-release', repo\)"/)
  assert.match(view, /:title="t\('查看 Release 日志'\)"/)
  assert.match(repositoriesCss, /\.repo-release\s*\{[^}]*appearance:\s*none/)
})
