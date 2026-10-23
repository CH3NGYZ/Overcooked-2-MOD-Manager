import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const source = readFileSync(new URL('../src/installedModOrdering.ts', import.meta.url), 'utf8')
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
const { installedModStatusPriority, sortInstalledModGroups } = await import(
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

test('installed MOD status priority follows update, latest, raw, unbound, unknown, disabled', () => {
  const statuses = [
    '已禁用',
    '检查中',
    '未绑定在线仓库，无法检测更新',
    '不是 Release 资产, 无法检测更新',
    '最新版',
    '有更新',
  ]
  assert.deepEqual(statuses.map(installedModStatusPriority).sort((left, right) => left - right), [0, 1, 2, 3, 4, 5])
  assert.deepEqual(sort(statuses.map((status) => mod(status, status))).map((item) => item.status), [
    '有更新', '最新版', '不是 Release 资产, 无法检测更新', '未绑定在线仓库，无法检测更新', '检查中', '已禁用',
  ])
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
  assert.equal(installedModStatusPriority('检查中'), 4)
  assert.deepEqual(sort([
    mod('unknown', '检查中'),
    mod('disabled', '已禁用'),
  ]).map((item) => item.id), ['unknown', 'disabled'])
})

test('the installed MOD view iterates every label and only makes update labels actionable', () => {
  const view = readFileSync(new URL('../src/components/InstalledModsView.vue', import.meta.url), 'utf8')
  assert.match(view, /v-for="label in installedModLabels\(mod\)"/)
  assert.match(view, /v-if="installedModLabelActionable\(label\)"/)
  assert.match(view, /openModRepository\(mod, label\)/)
  assert.match(view, /v-for="summary in group\.statuses"/)
  assert.match(view, /summary\.mods\.length/)
})

test('disabled MOD with an available update sorts by update and keeps both codes', () => {
  assert.equal(installedModStatusPriority(['disabled', 'update_available']), 0)
  assert.equal(installedModStatusPriority([]), 4)
  const sorted = sort([
    mod('disabled', '已禁用', { labels: ['disabled'] }),
    mod('disabled-update', '已禁用', { labels: ['disabled', 'update_available'] }),
    mod('latest', '最新版', { labels: ['latest'] }),
  ])
  assert.deepEqual(sorted.map((item) => item.id), ['disabled-update', 'latest', 'disabled'])
  assert.deepEqual(sorted[0].status, '已禁用')
  assert.deepEqual(sorted[0].labels, ['disabled', 'update_available'])
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
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
  const check = app.match(/async function checkModUpdates\(\) \{([\s\S]*?)\n\}/)?.[1]
  assert.ok(check, 'Use the actual MOD update-check handler')
  assert.match(check, /refreshMappedRepositoryUpdates/)
})
