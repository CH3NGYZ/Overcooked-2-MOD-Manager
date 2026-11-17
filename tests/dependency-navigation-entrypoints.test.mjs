import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks, stripTypeScriptTypes } from 'node:module'
import test from 'node:test'
import { parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import { effectScope } from 'vue'
import { dependencyActionLabel, dependencyRowFromStatus, levelSetDependencyRows } from '../src/dependencyRows.ts'
import { levelSetCardAction, levelSetCardTags } from '../src/levelSetStatus.ts'
import { repositoryForMod } from '../src/modStatus.ts'
import { resolveRelation } from '../src/modRelations.ts'
import { statusActionFor } from '../src/statusActions.ts'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/i18n.ts')) return { format: 'module', shortCircuit: true, source: 'export const t = key => key' }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true,
      source: 'export const useNotify=()=>({warning:()=>{}})' }
    return next(url, context)
  },
})
const { useDependencyNavigation, dependencyNavigationOpen: open, pendingDependencyNavigation: pending,
  confirmDependencyNavigation: confirm, cancelDependencyNavigation: cancel, afterDependencyNavigationLeave: leave,
} = await import('../src/composables/useDependencyNavigation.ts')

function handlers(path, names, bindings) {
  const { descriptor } = parse(readFileSync(new URL(path, import.meta.url), 'utf8'))
  const ast = ts.createSourceFile(path, descriptor.scriptSetup.content, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
  const source = names.map(name => {
    const node = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name)
    assert.ok(node, `real production handler ${name}`)
    return node.getText(ast)
  }).join('\n')
  return new Function('environment', `${stripTypeScriptTypes(`
    const { ${bindings.join(',')} } = environment
    ${source}
    const dependencyNavigation = useDependencyNavigation(openDependencyJump)
  `)}\nreturn { ${names.join(',')} }`)
}
const createMap = handlers('../src/views/LevelsView.vue', [
  'actionOf', 'dependencyRow', 'runCardAction', 'onTagClick', 'openDependencyJump',
], ['mods', 'maps', 'sets', 'repositories', 'gameProbed', 'refreshingHint', 'tagsOf',
  'levelSetCardAction', 'dependencyActionLabel', 'dependencyRowFromStatus', 'useDependencyNavigation',
  'linkToModRepository', 'linkToMap', 't', 'pickI18n'])
const createMod = handlers('../src/views/ModsLocalView.vue', [
  'actionContext', 'cardAction', 'relationOwnerMod', 'packageAction', 'runCardAction', 'runPackageAction',
  'packageTagTargetId', 'openPackageTag', 'dependencyRow', 'openDependencyJump',
], ['mods', 'effectiveMods', 'repositories', 'groups', 'relationIssueByDll', 'statusActionFor',
  'repositoryForMod', 'dependencyActionLabel', 'dependencyRowFromStatus', 'useDependencyNavigation',
  'linkToModRepository', 'isDependencyTag', 't', 'pickI18n'])
const release = tag => ({ tag_name: tag, version: tag, name: tag, body: '', assets: [], prerelease: false, draft: false })
const repo = (id, guid, tags) => ({ id, name: id, plugin_guid: guid, download_mode: 'release', cached_releases: tags.map(release) })
const installed = (guid, version) => ({ guid, version, name: guid, dll_file: `${guid}.dll`, enabled: true,
  labels: ['missing_dependency'] })
const isDependencyTag = key => ['missing_dependency', 'dependency_conflict', 'dependency_installable'].includes(key)

function fixture(t, currentVersion = '4.0', targetVersion = 'v3.10.2') {
  cancel(); leave()
  const mods = { value: [installed('web.runtime', currentVersion)] }, maps = { value: [] }
  const repositories = { value: [repo('runtime-source', 'web.runtime', [targetVersion])] }
  const relation = { guid: 'web.runtime', version: '>=3.10.2,<4' }
  const owner = installed('owner', '1.0')
  mods.value.push(owner)
  const set = { dir_name: 'Map', name: 'Map', version: '1.0', enabled: true, relations: [relation] }
  const sets = { value: [set] }, jumps = [], scope = effectScope()
  const issues = { get value() { return { [owner.dll_file]: resolveRelation(relation, mods.value, repositories.value) } } }
  const pkg = { key: 'pkg:owner', mods: [owner], statuses: [{ key: 'missing_dependency' }] }
  const environment = { mods, maps, repositories, sets, effectiveMods: mods, groups: { value: [{ packages: [pkg] }] },
    gameProbed: { value: true }, refreshingHint: { value: undefined }, relationIssueByDll: issues,
    tagsOf: current => levelSetCardTags(current, maps.value, mods.value, true, repositories.value),
    levelSetCardAction, dependencyActionLabel, dependencyRowFromStatus, useDependencyNavigation,
    statusActionFor, repositoryForMod, isDependencyTag,
    linkToModRepository: (...args) => jumps.push(args), linkToMap: () => assert.fail('must target a MOD'),
    t: key => key, pickI18n: (messages, fallback) => fallback }
  const map = scope.run(() => createMap(environment)), mod = scope.run(() => createMod(environment))
  t.after(() => { scope.stop(); cancel(); leave() })
  return { mods, maps, repositories, sets, set, owner, pkg, jumps, map, mod, relation }
}

for (const entry of ['map-action', 'map-tag', 'mod-action', 'package-action', 'package-tag']) {
  test(`${entry} confirms the same exact downgrade target as the map dependency table`, async t => {
    const f = fixture(t)
    if (entry === 'map-action') await f.map.runCardAction(f.set)
    if (entry === 'map-tag') f.map.onTagClick(f.set, levelSetCardTags(f.set, [], f.mods.value, true, f.repositories.value)[0])
    if (entry === 'mod-action') f.mod.runCardAction(f.owner)
    if (entry === 'package-action') f.mod.runPackageAction(f.pkg)
    if (entry === 'package-tag') f.mod.openPackageTag(f.pkg, 'missing_dependency')
    assert.equal(open.value, true)
    assert.deepEqual(f.jumps, [])
    const row = levelSetDependencyRows(f.set, { maps: [], mods: f.mods.value, repositories: f.repositories.value })[0]
    assert.deepEqual(pending.value.jump.versionChange, row.jump.versionChange)
    confirm()
    assert.deepEqual(f.jumps, [['runtime-source', 'v3.10.2']])
  })
}

test('map and MOD card buttons distinguish actual upgrade and downgrade candidates', t => {
  const f = fixture(t)
  assert.equal(f.map.actionOf(f.set).label, 'tools.goDowngrade')
  assert.equal(f.mod.cardAction(f.owner).action.label, 'tools.goDowngrade')
  f.mods.value[0] = installed('web.runtime', '3.0')
  assert.equal(f.map.actionOf(f.set).label, 'tools.goUpdate')
  assert.equal(f.mod.cardAction(f.owner).action.label, 'tools.goUpdate')
})

test('a changed current candidate invalidates a map card confirmation instead of jumping to an old release', async t => {
  const f = fixture(t)
  await f.map.runCardAction(f.set)
  f.repositories.value = [repo('runtime-source', 'web.runtime', ['v3.10.3'])]
  confirm()
  assert.deepEqual(f.jumps, [])
})

test('same repository with two dependency ranges keeps the selected relation identity', async t => {
  const f = fixture(t)
  f.mods.value[0] = installed('web.runtime', '3.5')
  f.repositories.value = [repo('runtime-source', 'web.runtime', ['v4.0', 'v3.10.2', 'v2.5'])]
  f.set.relations = [{ guid: 'web.runtime', version: '<3' }, { guid: 'web.runtime', version: '>=4' }]
  await f.map.runCardAction(f.set)
  assert.equal(pending.value.requirement, '<3')
  assert.equal(pending.value.jump.targetVersion, 'v2.5')
  confirm()
  assert.deepEqual(f.jumps, [['runtime-source', 'v2.5']])
})
