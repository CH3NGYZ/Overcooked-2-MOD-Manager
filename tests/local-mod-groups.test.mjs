import assert from 'node:assert/strict'
import test from 'node:test'

const {
  buildLocalModGroups,
  filterLocalModGroups,
  localModStatusUnits,
  packageKeysMatchingStatus,
} = await import('../src/localModGroups.ts')
const { buildLevelSetGroups } = await import('../src/levelSetGroups.ts')

function repository(overrides = {}) {
  return {
    id: 'repo-main', name: 'Main repository', name_i18n: { en: 'Main repository' },
    group: 'Tools', group_i18n: { 'zh-CN': '工具', en: 'Tools' },
    provider: 'github', username: 'owner', repository: 'main', download_mode: 'release',
    branch: 'main', file_path: '', channel: 'all', asset_rules: [], install_target: '',
    note: '', plugin_guid: '', local_product_name: '', local_original_filename: '',
    version_regex: '', max_release_count: 10, download_threads: 0, enabled: true,
    builtin: false, cached_releases: [],
    ...overrides,
  }
}

function mod(overrides = {}) {
  return {
    name: 'Mod', title: 'Mod', product_name: 'Mod', dll_file: 'Mod.dll',
    package_id: 'package-a', enabled: true, is_dependency: false,
    update_available: false, duplicate_guid: false, status: 'latest', labels: ['latest'],
    ...overrides,
  }
}

const options = (overrides = {}) => ({
  mods: [], repositories: [], matchedDllFiles: null,
  categoryTexts: { ungrouped: 'Ungrouped', localInstall: 'Local install', unbound: 'Unbound' },
  pickI18n: (map, fallback) => map?.en || map?.['zh-CN'] || fallback,
  displayTitle: (item) => item.title,
  actionFor: () => undefined,
  ...overrides,
})

test('MOD package projection keeps complete operations while search filters visible members', () => {
  const main = mod({
    name: 'Main', title: 'Main', dll_file: 'Main.dll', package_id: 'same', repository_id: 'repo-main',
  })
  const dependency = mod({
    name: 'Library', title: 'Library', dll_file: 'Library.dll', package_id: 'same',
    is_dependency: true, repository_id: undefined, labels: ['dependency'],
  })
  const groups = buildLocalModGroups(options({
    mods: [dependency, main],
    repositories: [repository()],
    matchedDllFiles: new Set(['Library.dll']),
  }))

  assert.equal(groups.length, 1)
  assert.equal(groups[0].key, 'group:tools')
  const pkg = groups[0].packages[0]
  assert.equal(pkg.key, 'pkg:same')
  assert.equal(pkg.grouped, true)
  assert.deepEqual(pkg.mods.map((item) => item.dll_file), ['Main.dll', 'Library.dll'])
  assert.deepEqual(pkg.visibleMods.map((item) => item.dll_file), ['Library.dll'])
  assert.equal(pkg.repositoryId, 'repo-main')
})

test('MOD categories use language-stable snapshot keys and put unbound groups last', () => {
  const snapshot = mod({
    package_id: undefined, dll_file: 'snapshot.dll', group_i18n: { 'zh-CN': '本地工具', en: 'Local tools' },
  })
  const unbound = mod({ package_id: undefined, dll_file: 'unbound.dll', labels: ['unbound'] })
  const groups = buildLocalModGroups(options({ mods: [unbound, snapshot] }))

  assert.deepEqual(groups.map((group) => group.key), ['group:本地工具', '__unbound__'])
  assert.deepEqual(groups.map((group) => group.label), ['Local tools', 'Unbound'])
})

test('status units count packages once and mixed package toggles do not claim a uniform state', () => {
  const mods = [
    mod({ dll_file: 'a.dll', enabled: true, labels: ['latest'] }),
    mod({ dll_file: 'b.dll', enabled: false, is_dependency: true, labels: ['dependency'] }),
    mod({ dll_file: 'single.dll', package_id: undefined, enabled: false, labels: ['update_available'] }),
  ]
  const groups = buildLocalModGroups(options({ mods }))
  const units = localModStatusUnits(groups)
  const packageUnit = units.find((unit) => unit.packageKey === 'pkg:package-a')
  assert.ok(packageUnit)
  assert.ok(!packageUnit.keys.includes('enabled'))
  assert.ok(!packageUnit.keys.includes('disabled'))
  assert.equal(units.filter((unit) => unit.packageKey === 'pkg:package-a').length, 1)

  const matching = packageKeysMatchingStatus(units, (keys) => keys.includes('update_available'))
  const visible = filterLocalModGroups(groups, matching)
  assert.deepEqual(visible.flatMap((group) => group.packages.map((pkg) => pkg.key)), ['dll:single.dll'])
})

test('level-set grouping delegates identity/category to levelSetGroup and sorts localized labels', () => {
  const sets = [
    { name: 'B', author: 'Author B', map_group: '', map_group_i18n: { en: 'Zeta' } },
    { name: 'A', author: 'Author A', map_group: '', map_group_i18n: { en: 'Alpha' } },
    { name: '', author: '', map_group: '', map_group_i18n: {} },
  ]
  const groups = buildLevelSetGroups(sets, [], 'en', (map, fallback) => map?.en || fallback, 'Ungrouped')

  assert.deepEqual(groups.map((group) => group.label), ['Alpha', 'Ungrouped', 'Zeta'])
  assert.deepEqual(groups.map((group) => group.key), ['group:alpha', '__ungrouped__', 'group:zeta'])
  assert.deepEqual(groups[0].sets, [sets[1]])
})
