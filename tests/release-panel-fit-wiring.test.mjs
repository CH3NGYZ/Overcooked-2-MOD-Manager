import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const panel = read('src/components/ReleaseDetailPanel.vue')
const repositoryDetail = read('src/components/ModRepositoryDetail.vue')
const modsOnline = read('src/views/ModsOnlineView.vue')
const mapsOnline = read('src/views/MapsOnlineView.vue')
const mapReleaseDetail = read('src/composables/useMapReleaseDetail.ts')
const fit = read('src/composables/useReleasePanelFit.ts')
const pureFit = read('src/releasePanelFit.ts')

test('release panel delegates DOM sizing and lifecycle to one composable', () => {
  assert.match(panel, /useReleasePanelFit\(/)
  assert.match(panel, /:class="\{ 'detail--fill': isFitted \}"/)
  assert.match(panel, /ref="panelElement"/)
  assert.match(panel, /ref="logBoxElement"/)
  assert.doesNotMatch(panel, /new ResizeObserver|function fitInstallPanel|function scheduleFitInstallPanel|onBeforeUnmount\(/)

  assert.match(fit, /new ResizeObserver/)
  assert.match(fit, /onBeforeUnmount\(/)
  assert.match(fit, /cancelAnimationFrame\(fitFrame\)/)
  assert.match(fit, /\.n-scrollbar-container/)
  assert.match(fit, /\.n-drawer/)
  assert.match(fit, /\.install-row/)
  assert.match(fit, /fitsMinLogHeight\(/)
  assert.match(fit, /releasePanelHeight\(/)
})

test('both online pages use the shared release panel through their domain adapters', () => {
  assert.match(repositoryDetail, /<ReleaseDetailPanel v-if="repo"/)
  assert.match(modsOnline, /import ModRepositoryDetail from '\.\.\/components\/ModRepositoryDetail\.vue'/)
  assert.match(mapsOnline, /<ReleaseDetailPanel :key="detailMap\.id"/)
  assert.match(mapsOnline, /detailAssets/)
  assert.match(mapsOnline, /useMapReleaseDetail\(/)
  assert.match(mapsOnline, /detailDependencies/)
  assert.doesNotMatch(mapsOnline, /useModRepositoryReadme/)

  assert.match(mapReleaseDetail, /mapDependencyRows\(/)
  assert.match(mapReleaseDetail, /matchedReleaseAssets\(/)
  assert.match(mapReleaseDetail, /watch\(\s*\[activeMapId, latestTag\]/s)
  assert.match(mapReleaseDetail, /selectedAssetName\.value = null/)
  assert.doesNotMatch(mapsOnline, /mapDependencyRows\(|matchedReleaseAssets\(|isPrereleaseVersion\(/)
})

test('release panel pure geometry remains the single owner of height and scroll rules', () => {
  assert.match(fit, /from '\.\.\/releasePanelFit'/)
  assert.match(pureFit, /export function releasePanelHeight\(/)
  assert.match(pureFit, /export function fitsMinLogHeight\(/)
  assert.match(pureFit, /export function wheelNeedsHandoff\(/)
})
