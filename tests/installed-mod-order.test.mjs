import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const source = readFileSync(new URL('../src/installedModOrdering.ts', import.meta.url), 'utf8')
const repositoryRefreshSource = readFileSync(new URL('../src/composables/useRepositoryRefresh.ts', import.meta.url), 'utf8')
const modsModelSource = readFileSync(new URL('../src/composables/useInstalledModsModel.ts', import.meta.url), 'utf8')
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
const { installedModStatusOrder, installedModStatusPriority, sortInstalledModGroups } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`,
)

const compareText = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' }).compare
function mod(id, status, patch = {}) {
  return { id, status, name: id, dll_file: `${id}.dll`, package_id: undefined, ...patch }
}
function sort(mods) {
  return sortInstalledModGroups(
    mods,
    (item) => item.package_id,
    (item) => item.labels ?? item.status,
    (item, packageId) => packageId ? (item.package_name || packageId) : item.name,
    (item) => item.name,
    (item) => item.dll_file,
    compareText,
  )
}

test('checking MODs sort above update, latest, raw, unbound and disabled', () => {
  const statuses = [
    '已禁用',
    '检查中',
    '未绑定在线仓库，无法检测更新',
    '不是 Release 资产, 无法检测更新',
    '最新版',
    '有更新',
  ]
  assert.deepEqual(statuses.map(installedModStatusPriority).sort((left, right) => left - right), [0, 1, 3, 4, 5, 8])
  assert.deepEqual(sort(statuses.map((status) => mod(status, status))).map((item) => item.status), [
    '检查中', '有更新', '最新版', '不是 Release 资产, 无法检测更新', '未绑定在线仓库，无法检测更新', '已禁用',
  ])
})

test('badge order starts with the checking states before update and latest', () => {
  assert.deepEqual(installedModStatusOrder.slice(0, 4), ['checking', 'pending', 'update_available', 'latest'])
  assert.equal(installedModStatusOrder.at(-1), 'disabled')
})

test('same-status MODs use text order and package members stay contiguous', () => {
  const items = [
    mod('zeta', '最新版'),
    mod('package-child', '已禁用', { package_id: 'package', package_name: 'Package' }),
    mod('alpha', '有更新'),
    mod('package-main', '有更新', { package_id: 'package', package_name: 'Package' }),
    mod('beta', '最新版'),
  ]
  const sorted = sort(items)
  assert.deepEqual(sorted.map((item) => item.id), ['alpha', 'package-child', 'package-main', 'beta', 'zeta'])
  assert.deepEqual(sort([mod('zeta', '最新版'), mod('alpha', '最新版')]).map((item) => item.id), ['alpha', 'zeta'])
})

test('a package uses its highest-priority member status without changing member statuses', () => {
  const sorted = sort([
    mod('standalone', '最新版'),
    mod('package-disabled', '已禁用', { package_id: 'package', package_name: 'Package' }),
    mod('package-update', '有更新', { package_id: 'package', package_name: 'Package' }),
  ])
  assert.deepEqual(sorted.map((item) => item.id), ['package-disabled', 'package-update', 'standalone'])
  assert.equal(sorted.find((item) => item.id === 'package-disabled').status, '已禁用')
})

test('unknown statuses remain before disabled MODs', () => {
  assert.equal(installedModStatusPriority('版本信息不足，无法检测更新'), 6)
  assert.equal(installedModStatusPriority('检查中'), 0)
  assert.deepEqual(sort([
    mod('unknown', '版本信息不足，无法检测更新'),
    mod('checking', '检查中'),
    mod('disabled', '已禁用'),
  ]).map((item) => item.id), ['checking', 'unknown', 'disabled'])
})

test('the installed MOD view iterates every label and only makes update labels actionable', () => {
  const view = readFileSync(new URL('../src/components/InstalledModsView.vue', import.meta.url), 'utf8')
  assert.match(view, /v-for="label in installedModLabels\(mod\)"/)
  assert.match(view, /v-if="installedModLabelActionable\(label\)"/)
  assert.match(view, /openModRepository\(mod, label\)/)
  assert.match(view, /v-for="summary in group\.statuses"/)
  assert.match(view, /summary\.mods\.length/)
})

test('disabled MOD with an available update sorts directly below enabled updates', () => {
  assert.equal(installedModStatusPriority(['disabled', 'update_available']), 2)
  assert.equal(installedModStatusPriority(['update_available']), 1)
  assert.equal(installedModStatusPriority([]), 6)
  const sorted = sort([
    mod('disabled', '已禁用', { labels: ['disabled'] }),
    mod('disabled-update', '已禁用', { labels: ['disabled', 'update_available'] }),
    mod('update', '有更新', { labels: ['update_available'] }),
    mod('latest', '最新版', { labels: ['latest'] }),
  ])
  assert.deepEqual(sorted.map((item) => item.id), ['update', 'disabled-update', 'latest', 'disabled'])
  assert.equal(sorted[1].status, '已禁用')
  assert.deepEqual(sorted[1].labels, ['disabled', 'update_available'])
})

test('the installed MOD list follows the agreed urgency ladder', () => {
  const items = [
    mod('lower-disabled', '已禁用', { labels: ['duplicate_lower_version', 'disabled'] }),
    mod('duplicate-disabled', '已禁用', { labels: ['duplicate', 'disabled'] }),
    mod('latest-disabled', '已禁用', { labels: ['latest', 'disabled'] }),
    mod('cleanup-enabled', '重复', { labels: ['duplicate'] }),
    mod('unbound', '未绑定在线仓库，无法检测更新', { labels: ['unbound'] }),
    mod('raw', '不是 Release 资产, 无法检测更新', { labels: ['not_release_asset'] }),
    mod('latest', '最新版', { labels: ['latest'] }),
    mod('update-disabled', '已禁用', { labels: ['disabled', 'update_available'] }),
    mod('update', '有更新', { labels: ['update_available'] }),
  ]
  assert.deepEqual(sort(items).map((item) => item.id), [
    'update', 'update-disabled', 'latest', 'raw', 'unbound', 'cleanup-enabled',
    'latest-disabled', 'duplicate-disabled', 'lower-disabled',
  ])
  assert.deepEqual(items.map((item) => installedModStatusPriority(item.labels)), [10, 9, 8, 7, 5, 4, 3, 2, 1])
})

test('changing a MOD status causes its group to move on the next sort', () => {
  const changing = mod('z-changing', '最新版')
  const stable = mod('a-stable', '最新版')
  const mods = [changing, stable]
  assert.deepEqual(sort(mods).map((item) => item.id), ['a-stable', 'z-changing'])
  changing.status = '有更新'
  assert.deepEqual(sort(mods).map((item) => item.id), ['z-changing', 'a-stable'])
  changing.status = '已禁用'
  assert.deepEqual(sort(mods).map((item) => item.id), ['a-stable', 'z-changing'])
})

test('the scan action only reloads the dashboard and does not refresh repositories', () => {
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
  const scan = app.match(/async function rescanInstalledMods\(\) \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(scan, 'Use the actual local MOD scan handler')
  assert.match(scan, /api\.dashboard\(\)/)
  assert.doesNotMatch(scan, /refreshMappedRepositoryUpdates|refreshRepository|refreshRepo/)
})

test('the explicit update-check action remains the remote refresh entry point', () => {
  const check = repositoryRefreshSource.match(/async function checkModUpdates\(\) \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(check, 'Use the actual MOD update-check handler')
  assert.match(check, /refreshMappedRepositoryUpdates/)
})

const updatesSource = readFileSync(new URL('../src/modUpdates.ts', import.meta.url), 'utf8')
const { outputText: updatesOutput } = ts.transpileModule(updatesSource, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
const { modUpdateCheckPhase, liveModReleaseLabel } = await import(
  `data:text/javascript;base64,${Buffer.from(updatesOutput).toString('base64')}`,
)

test('a MOD reports per repository progress instead of waiting for the whole check', () => {
  const base = { active: true, refreshing: false, hasBoundRepositories: true, settled: false }
  assert.equal(modUpdateCheckPhase({ ...base, active: false }), 'idle')
  assert.equal(modUpdateCheckPhase({ ...base, refreshing: true, settled: true }), 'checking')
  assert.equal(modUpdateCheckPhase(base), 'pending')
  assert.equal(modUpdateCheckPhase({ ...base, settled: true }), 'settled')
  assert.equal(modUpdateCheckPhase({ ...base, hasBoundRepositories: false }), 'idle')
})

test('a finished MOD shows its own result while other repositories still run', () => {
  assert.equal(liveModReleaseLabel({ releaseStatus: '有更新', hasRepositoryError: false, settled: true }), 'update_available')
  assert.equal(liveModReleaseLabel({ releaseStatus: '最新版', hasRepositoryError: false, settled: true }), 'latest')
  assert.equal(liveModReleaseLabel({ releaseStatus: '', hasRepositoryError: true, settled: true }), 'repository_unavailable')
  assert.equal(liveModReleaseLabel({ releaseStatus: '', hasRepositoryError: false, settled: true }), 'unknown')
  assert.equal(liveModReleaseLabel({ releaseStatus: '', hasRepositoryError: false, settled: false }), 'pending')
})

test('the update-check batch has no global overlay and reloads the dashboard once', () => {
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
  const batch = repositoryRefreshSource.match(/async function refreshMappedRepositoryUpdates\(\) \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(batch, 'Use the actual incremental update-check host')
  assert.match(batch, /modUpdatesCheckActive\.value = true/)
  assert.match(batch, /const mapped = mappedUpdateRepositories\(\)/)
  assert.match(batch, /refreshMappedRepositories\(mapped\)/)
  assert.match(batch, /api\.dashboard\(\)/)
  assert.doesNotMatch(batch, /installedModsChecking|installedModsPendingCheck/)
  const mapped = repositoryRefreshSource.match(/async function refreshMappedRepositories\(repositories = mappedUpdateRepositories\(\)\) \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(mapped, 'Use the actual mapped repository refresh')
  assert.match(mapped, /refreshRepositoryBatch\(repositories, 4\)/)
  const status = modsModelSource.match(/function installedModStatusCodes\(mod: InstalledMod\)([\s\S]*?)\n  \}/)?.[1]
  assert.ok(status, 'Use the actual per-MOD status derivation')
  assert.match(status, /modUpdateCheckPhase\(/)
  assert.match(status, /isModRepositoryRefreshing\(mod\)/)
  assert.doesNotMatch(status, /installedModsChecking|installedModsPendingCheck/)
})
