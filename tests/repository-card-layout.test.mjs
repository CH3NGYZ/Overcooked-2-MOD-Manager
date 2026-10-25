import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const view = readFileSync(new URL('../src/components/RepositoriesView.vue', import.meta.url), 'utf8')
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const repositoriesCss = readFileSync(new URL('../src/styles/repositories.css', import.meta.url), 'utf8')
const morph = readFileSync(new URL('../src/composables/useRepositoryCardMorph.ts', import.meta.url), 'utf8')
const layoutState = readFileSync(new URL('../src/repositoryLayout.ts', import.meta.url), 'utf8')
const i18n = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8')

test('compact list, grid and expanded states share one card structure', () => {
  assert.match(view, /'ocard', \{/)
  assert.doesNotMatch(view, /repository-row/, 'The old row card must be gone')
  assert.match(repositoriesCss, /\.ocard\s*\{[^}]*grid-template-areas:\s*"tile name oline" "tile desc oline"/)
  assert.match(repositoriesCss, /\.repository-group-items\.as-grid\s*\{[^}]*grid-template-columns:\s*repeat\(auto-fill, 300px\)/)
  assert.match(repositoriesCss, /\.repository-group-items\.as-grid \.ocard\s*\{[^}]*grid-template-areas:\s*"tile name" "tile desc" "oline oline"/)
  assert.match(repositoriesCss, /\.ocard\.is-expanded\s*\{[^}]*position:\s*fixed/)
  assert.match(repositoriesCss, /\.ocard\.is-closing\s*\{[^}]*position:\s*fixed/)
})

test('cards keep an independent background layer so only the border animates', () => {
  assert.match(view, /<div class="ocard-bg"><\/div>/)
  assert.match(repositoriesCss, /\.ocard-bg\s*\{[^}]*position:\s*absolute/)
  // The layer must stay mounted in every state; hiding it in the settled state
  // is what made the surface blink when the open animation finished.
  assert.doesNotMatch(repositoriesCss, /\.ocard\.is-expanded\.is-settled \.ocard-bg\s*\{[^}]*display:\s*none/)
  assert.match(repositoriesCss, /\.ocard\.is-expanded \.ocard-bg,[\s\S]{0,120}\.repository-group-items\.as-grid \.ocard\.is-expanded \.ocard-bg\s*\{[^}]*display:\s*block/)
  assert.match(morph, /card\.style\.removeProperty\('width'\)/)
})

test('layout switching animates real width and height instead of transform fakes', () => {
  assert.match(morph, /flight\.card\.style\.width = `\$\{flight\.from\.width \+/)
  assert.match(morph, /requestAnimationFrame\(fire\)/)
  assert.match(morph, /setTimeout\(\(\) => fire\(performance\.now\(\)\), 40\)/)
  assert.match(morph, /if \(document\.hidden\)/)
  assert.match(view, /morph\.captureCards\(container\)/)
  assert.match(view, /morph\.animateSwitch\(container, flights\)/)
})

test('morph coordinates convert the scaled canvas to logical pixels', () => {
  assert.match(morph, /shell\.getBoundingClientRect\(\)\.width \/ shell\.offsetWidth/)
  assert.match(morph, /rect\.left \/ scale/)
})

test('the install button reads 立即安装 and turns amber for updates', () => {
  assert.match(view, /data-tutorial="repo-install"/)
  assert.match(view, /:class="\{ amber: repositoryHasUpdate\(repo\) \}"/)
  assert.match(view, /repositoryHasUpdate\(repo\) \? t\('立即更新'\) : t\('立即安装'\)/)
  assert.match(i18n, /'立即安装':/)
  assert.match(repositoriesCss, /\.dl\.amber\s*\{[^}]*background:\s*var\(--warning\)/)
})

test('the status badge keeps only its own width and the version column stays centered', () => {
  // The badge must not reserve a fixed slot: a wide "仓库文件"/"最新版" label used
  // to push the action button out of the card.
  assert.doesNotMatch(repositoriesCss, /\.obadges\s*\{[^}]*width:\s*96px/)
  assert.match(repositoriesCss, /\.obadges\s*\{[^}]*flex:\s*0 0 auto/)
  assert.match(repositoriesCss, /\.ometa\s*\{[^}]*width:\s*112px/)
  assert.match(view, /@click\.stop="showReleaseOrRefresh\(repo\)"/)
  assert.match(view, /:title="t\(latest\(repo\) \? '查看 Release 日志' : '刷新仓库'\)"/)
  assert.match(i18n, /'刷新仓库':/)
})

test('grid cards split the action row 1:2:2 so the badge cannot push the button out', () => {
  const gridOline = repositoriesCss.match(/\.repository-group-items\.as-grid \.ocard \.oline \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(gridOline, 'grid card action row rule must exist')
  assert.match(gridOline, /display:\s*grid/)
  assert.match(gridOline, /grid-template-columns:\s*1fr 2fr 2fr/)
  // The badge keeps its intrinsic width inside its track instead of reserving
  // a fixed slot or stretching to the full column.
  assert.doesNotMatch(repositoriesCss, /\.repository-group-items\.as-grid \.ocard \.obadges\s*\{[^}]*width:\s*100%/)
  assert.match(repositoriesCss, /\.repository-group-items\.as-grid \.ocard \.ometa\s*\{[^}]*width:\s*100%/)
  assert.match(repositoriesCss, /\.repository-group-items\.as-grid \.ocard \.oacts\s*\{[^}]*width:\s*100%/)
  // The expanded and closing panels sit in the same grid container, so both
  // must drop the three-column tracks again and fall back to a flex action row.
  assert.match(repositoriesCss, /\.ocard\.is-expanded \.oline,[\s\S]{0,140}\.repository-group-items\.as-grid \.ocard\.is-expanded \.oline\s*\{[^}]*display:\s*flex/)
  assert.match(repositoriesCss, /\.ocard\.is-closing \.oline,[\s\S]{0,140}\.repository-group-items\.as-grid \.ocard\.is-closing \.oline\s*\{[^}]*display:\s*flex/)
})

test('the expanded panel keeps a fixed rectangle and clips overflow', () => {
  assert.match(repositoriesCss, /\.ocard\.is-expanded\s*\{[^}]*overflow:\s*hidden/)
  assert.match(view, /function computePanelRect/)
  assert.match(view, /- 30/)
})

test('the expanded panel is lifted above the sticky headings without shifting them', () => {
  // The card lives inside containers that set `isolation: isolate`, which
  // traps its z-index below the sticky headers (z-index 30/20). The morph must
  // flatten the chain, but it must NOT add `position: relative` to the
  // ancestors: that rebuilds their containing block and makes the sticky source
  // and group headings jump while the panel opens.
  assert.match(morph, /function liftStackingChain/)
  assert.doesNotMatch(morph, /node\.style\.position = 'relative'/)
  assert.match(morph, /node\.style\.isolation = 'auto'/)
  assert.match(morph, /node\.style\.zIndex = '60'/)
  assert.match(morph, /closest\('\.repository-list'\)/)
  assert.match(morph, /function releaseStackingChain/)
  assert.match(morph, /liftStackingChain\(cardEl\)/)
  assert.match(morph, /releaseStackingChain\(\)/)
})

test('expand and collapse only resize the card box, never scale the contents', () => {
  const open = morph.slice(morph.indexOf('async function openCard'), morph.indexOf('async function closeCard'))
  const close = morph.slice(morph.indexOf('async function closeCard'), morph.indexOf('function captureCards'))
  for (const [label, section] of [['openCard', open], ['closeCard', close]]) {
    assert.ok(section.length > 0, `${label} body must be found`)
    assert.doesNotMatch(section, /\.style\.transform\s*=\s*`[^`]*scale\(/, `${label} must not scale elements`)
    assert.doesNotMatch(section, /transformOrigin/, `${label} must not set transform-origin`)
    assert.doesNotMatch(section, /translate\(/, `${label} must not translate elements`)
    assert.match(section, /setBox\(/, `${label} must animate the card box`)
  }
  // Sizes interpolate through a CSS variable so type and buttons follow the box.
  assert.match(close, /--ocard-close-p/)
})

test('the closing state interpolates sizes and uses the compact card grid', () => {
  // Two things used to be wrong. Sizes snapped back to the compact card the
  // moment is-expanded was removed, and the closing layout used the panel's
  // three-row template. The final style swap therefore re-flowed every element
  // at once, which is the "wrong last frame" the user saw. The close now
  // interpolates sizes through --ocard-close-p (0 = panel, 1 = card) and uses
  // the compact card's own grid areas from the first frame.
  const closing = repositoriesCss.slice(repositoriesCss.indexOf('.ocard.is-closing,'))
  assert.match(closing, /--ocard-close-p/)
  assert.match(closing, /grid-template-areas:\s*"tile name oline" "tile desc oline"/)
  assert.match(closing, /\.ocard\.is-closing \.oname,[\s\S]{0,220}font-size:\s*calc\(21px/)
  assert.match(closing, /\.ocard\.is-closing \.tile,[\s\S]{0,220}calc\(46px/)
  assert.match(closing, /\.ocard\.is-closing \.dl,[\s\S]{0,220}calc\(/)
  // The grid layout stacks the action row; the closing state must do the same.
  assert.match(closing, /\.repository-group-items\.as-grid \.ocard\.is-closing \{[^}]*grid-template-areas:\s*"tile name" "tile desc" "oline oline"/)
  // The surface stays painted for the whole morph, otherwise the card blinks.
  assert.match(closing, /\.ocard\.is-closing \.ocard-bg\s*\{[^}]*display:\s*block/)
})

test('collapse lands exactly on the placeholder slot instead of offsetting by one card', () => {
  const close = morph.slice(morph.indexOf('async function closeCard'), morph.indexOf('function captureCards'))
  // The placeholder already sits at the card's home slot; adding its delta to
  // the panel origin double-counts and lands one card away.
  assert.match(close, /const target = logicalRect\(placeholder\)/)
  assert.doesNotMatch(close, /const dx = anchor/, 'no delta-from-anchor arithmetic')
  assert.doesNotMatch(close, /homeRect/, 'the stale home rectangle must not be used')
  assert.doesNotMatch(morph, /homeRect/, 'homeRect must be removed entirely')
})

test('the expanded panel lays out icon+title, then status/version/actions, then the body', () => {
  // Grid mode is more specific, so every expanded override must repeat the
  // container selector or the card keeps the compact grid layout.
  assert.match(repositoriesCss, /\.ocard\.is-expanded,\s*\n\.repository-group-items\.as-grid \.ocard\.is-expanded\s*\{[^}]*grid-template-areas:\s*"tile name" "oline oline" "body body"/)
  // `.oname` is nested in `.onameline`, which is the real grid item.
  assert.match(repositoriesCss, /\.ocard\.is-expanded \.onameline,\s*\n\.repository-group-items\.as-grid \.ocard\.is-expanded \.onameline\s*\{[^}]*grid-area:\s*name/)
  assert.match(repositoriesCss, /\.ocard\.is-expanded \.oline,\s*\n\.repository-group-items\.as-grid \.ocard\.is-expanded \.oline\s*\{[^}]*display:\s*flex/)
  assert.match(repositoriesCss, /\.ocard\.is-expanded \.odesc,\s*\n\.repository-group-items\.as-grid \.ocard\.is-expanded \.odesc\s*\{[^}]*grid-area:\s*body/)
})

test('expanded action icons follow install, edit, README, delete order', () => {
  const acts = view.slice(view.indexOf('<div class="oicons">'), view.indexOf('</div>', view.indexOf('<div class="oicons">')))
  const editAt = acts.indexOf("t('编辑仓库')")
  const readmeAt = acts.indexOf("t('查看仓库自述文件')")
  const deleteAt = acts.indexOf("t('删除仓库')")
  assert.ok(editAt > -1 && readmeAt > -1 && deleteAt > -1, 'all three icon buttons must exist')
  assert.ok(editAt < readmeAt, 'edit comes before README')
  assert.ok(readmeAt < deleteAt, 'README comes before delete')
})

test('expanding a card never fetches the README; only the README button opens the dialog', () => {
  // Opening a card must not touch the network: the panel only shows the note
  // preview, and the README is fetched by RepositoryDocumentDialog on demand.
  assert.doesNotMatch(view, /repositoryReadme/, 'no README IPC from the card view')
  assert.doesNotMatch(view, /MarkdownContent/, 'the card no longer renders Markdown inline')
  assert.doesNotMatch(view, /ocard-body/, 'the inline detail body must be gone')
  assert.match(view, /show-repository-readme/)
  assert.match(app, /@show-repository-readme=/)
  assert.match(app, /<RepositoryDocumentDialog :target="repositoryDocument"/)
})

test('grid cards are the default layout and the choice persists', () => {
  assert.match(layoutState, /=== 'list' \? 'list' : 'grid'/, 'grid is the fallback layout')
  assert.match(layoutState, /localStorage\.setItem\(LAYOUT_STORAGE_KEY/, 'the choice is persisted')
  assert.match(layoutState, /localStorage\.getItem\(LAYOUT_STORAGE_KEY/, 'the choice is restored on start')
})

test('search filters cards, groups and shells with a live count', () => {
  assert.match(view, /v-model="searchText"/)
  assert.match(view, /'search-hidden': !repoMatches\(repo\)/)
  assert.match(view, /'search-hidden': !childSourceVisible\(nestedGroup\)/)
  assert.match(view, /'search-hidden': !sourceShellVisible\(group\)/)
  assert.match(repositoriesCss, /\.search-hidden\s*\{[^}]*display:\s*none !important/)
  assert.match(view, /t\('共 \{count\} 个 MOD'\)\.replace\('\{count\}'/)
  assert.match(i18n, /'共 \{count\} 个 MOD':/)
  assert.match(view, /'没有匹配的 MOD'/)
})

test('layout controls live in the topbar, left of the language selector', () => {
  // The switch is rendered by App.vue's topbar so it sits next to the global
  // controls; RepositoriesView only consumes the shared state.
  assert.match(app, /data-tutorial="repo-layout-list"/)
  assert.match(app, /data-tutorial="repo-layout-grid"/)
  assert.match(app, /class="layout-seg"[\s\S]*?class="language-select"/)
  assert.match(app, /v-if="activeView === 'repositories'" class="layout-seg"/)
  assert.doesNotMatch(view, /data-tutorial="repo-layout-list"/)
  assert.match(view, /const viewMode = repositoryLayout/)
  assert.match(view, /watch\(repositoryLayout, async/)
  assert.match(view, /morph\.captureCards\(container\)/)
  assert.match(view, /morph\.animateSwitch\(container, flights\)/)
})
