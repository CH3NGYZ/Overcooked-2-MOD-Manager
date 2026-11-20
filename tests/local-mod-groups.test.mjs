import assert from 'node:assert/strict'
import test from 'node:test'

const {
  buildLocalModGroups,
  filterLocalModGroups,
  localModStatusUnits,
  packageKeysMatchingStatus,
  summarizeLocalModStatuses,
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
  assert.deepEqual(pkg.coreMods.map((item) => item.dll_file), ['Main.dll'])
  assert.equal(pkg.repositoryId, 'repo-main')
})

test('package version follows the core GUID rather than a higher bundled DLL or a stale release tag', () => {
  const binding = { provider: 'github', owner: 'owner', repository: 'main', channel: 'all', tag: 'v4.0.0' }
  const core = mod({ dll_file: 'Loader.dll', version: '3.10.2', repository_identity_match: 'guid', repository_binding: binding })
  const debug = mod({ dll_file: 'debugLog.dll', version: '99.1.0', repository_identity_match: 'metadata', repository_binding: binding })
  const dependency = mod({ dll_file: 'Library.dll', version: '200.0.0', is_dependency: true, repository_binding: binding })
  const project = (mods, matchedDllFiles = null) => buildLocalModGroups(options({ mods, matchedDllFiles }))[0].packages[0]

  // The display filter must not affect the full package's installed identity.
  assert.equal(project([debug, dependency, core], new Set(['debugLog.dll'])).version, '3.10.2')
  assert.equal(project([debug, { ...core, version: '4.0' }]).version, 'v4.0.0')
  assert.equal(project([debug, { ...core, version: '4.0.0-beta.3' }]).version, '4.0.0-beta.3')
  assert.equal(project([debug, { ...core, version: '4.1.0' }]).version, '4.1.0')
  assert.equal(project([debug, { ...core, version: undefined }]).version, 'v4.0.0')

  const beta = { ...binding, tag: 'v4.0.0-beta.3' }
  const betaDebug = { ...debug, repository_binding: beta }
  assert.equal(project([betaDebug, { ...core, repository_binding: beta, version: '4.0.0.3' }]).version, beta.tag)
  assert.equal(project([betaDebug, { ...core, repository_binding: beta, version: '4.0.0.2' }]).version, '4.0.0.2')
})

test('ambiguous package identity falls back only to a shared tag, isolated from other packages', () => {
  const binding = { provider: 'github', owner: 'owner', repository: 'main', channel: 'all', tag: 'v4.0.0' }
  const plugin = mod({ dll_file: 'A.dll', version: '1.0', repository_binding: binding })
  const sibling = mod({ dll_file: 'B.dll', version: '2.0', repository_binding: binding })
  const dependency = mod({ dll_file: 'Stub.dll', is_dependency: true, repository_binding: binding })
  const project = (mods) => buildLocalModGroups(options({ mods })).flatMap((group) => group.packages)

  assert.equal(project([plugin, sibling])[0].version, binding.tag)
  assert.equal(project([plugin, dependency])[0].version, binding.tag)
  assert.equal(project([plugin, { ...sibling, repository_identity_match: 'metadata' }])[0].version, '2.0')
  assert.equal(project([plugin, sibling].map((item) => ({ ...item, repository_identity_match: 'guid' })))[0].version, binding.tag)
  assert.equal(project([plugin, { ...sibling, repository_binding: { ...binding, tag: 'v5.0' } }])[0].version, undefined)

  const second = [plugin, sibling].map((item) => ({ ...item, package_id: 'package-b', repository_binding: undefined }))
  const third = [plugin, dependency].map((item) => ({ ...item, package_id: 'package-c', repository_binding: undefined }))
  assert.deepEqual(project([plugin, sibling, ...second, ...third]).map((pkg) => [pkg.key, pkg.version]), [
    ['pkg:package-a', binding.tag], ['pkg:package-b', undefined], ['pkg:package-c', '1.0'],
  ])
})

test('declared loose plugins follow the real core for grouping, status and switches without gaining package deletion', () => {
  const core = mod({ dll_file: 'Main.dll', title: 'Main', package_id: undefined, version: '4.0.0',
    repository_id: 'repo-main', repository_identity_match: 'guid' })
  const helper = mod({ dll_file: 'Other/Debug.dll', title: 'DebugLog', package_id: undefined, bundled_with_dll: core.dll_file,
    version: '99.0.0-beta.1', enabled: false, labels: ['unbound', 'disabled', 'load_warning'] })
  const project = (mods) => buildLocalModGroups(options({ mods, repositories: [repository()],
    matchedDllFiles: new Set([helper.dll_file]) })).flatMap((group) => group.packages)[0]
  const loose = project([helper, core])
  assert.equal(loose.key, 'dll:Main.dll')
  assert.equal(loose.label, 'Main')
  assert.equal(loose.repositoryId, 'repo-main')
  assert.equal(loose.version, '4.0.0')
  assert.equal(loose.partial, true)
  assert.equal(loose.grouped, true)
  assert.equal(loose.canRemovePackage, false)
  assert.deepEqual(loose.mods.map((item) => item.dll_file), ['Main.dll', helper.dll_file])
  assert.deepEqual(loose.visibleMods, [helper])
  assert.deepEqual(loose.coreMods, [core])
  assert.deepEqual(loose.statuses.map(({ key }) => key), ['latest', 'load_warning'])

  const recordedCore = { ...core, package_id: 'manifest', repository_binding: { tag: 'v4.0' } }
  const mixed = project([helper, recordedCore])
  assert.equal(mixed.key, 'pkg:manifest')
  assert.equal(mixed.version, 'v4.0')
  assert.equal(mixed.canRemovePackage, false)
  assert.equal(project([{ ...helper, bundled_with_dll: undefined, package_id: 'manifest' }, recordedCore]).canRemovePackage, true)
  const recordedCompanion = { ...helper, bundled_with_dll: undefined, release_companion: true, package_id: 'manifest' }
  assert.deepEqual(project([recordedCompanion, recordedCore]).coreMods, [recordedCore])
  assert.equal(project([helper]).grouped, false)
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

test('MOD status summary keeps primary and toggle groups owned by the projection', () => {
  const summary = summarizeLocalModStatuses([
    { packageKey: 'pkg:a', keys: ['enabled', 'latest', 'latest'] },
    { packageKey: 'pkg:b', keys: ['disabled', 'update_available'] },
  ])

  assert.deepEqual(summary.all.map((entry) => entry.key), ['update_available', 'disabled', 'latest', 'enabled'])
  assert.deepEqual(summary.primary.map((entry) => entry.key), ['update_available', 'latest'])
  assert.deepEqual(summary.toggle.map((entry) => entry.key), ['enabled', 'disabled'])
  assert.deepEqual(summary.all.map((entry) => entry.count), [1, 1, 1, 1])
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
