import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks, stripTypeScriptTypes } from 'node:module'
import test from 'node:test'
import { parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import { effectScope } from 'vue'
import { dependencyRowFromStatus, dependencyTagTitle, installedModDependencyRows, levelSetDependencyRows } from '../src/dependencyRows.ts'
import { levelSetCardAction, levelSetCardTags, levelSetForMap, levelSetRelationStatuses } from '../src/levelSetStatus.ts'
import { levelSetTagKey } from '../src/levelSetStatusSummary.ts'
import { conflictRemovalTarget, dependencyRepairStatus, dependencyTagFor, isRelationCardKey,
  relationJumpTarget, relationStatusesForSubject } from '../src/modRelations.ts'
import { runtimeModStatus } from '../src/modRuntimeStatus.ts'
import { modsForRepository, repositoryForMod } from '../src/modStatus.ts'
import { evaluationSnapshot, clearEvaluation } from './helpers/mod-change-evaluations.mjs'
import { statusActionFor } from '../src/statusActions.ts'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/modChanges/store.ts')) return { format:'module',shortCircuit:true,source:'export const useModChanges=()=>({run:async request=>{globalThis.__repairRequests.push(request);return null}})' }
    if (url.endsWith('/src/composables/useDashboardStore.ts')) return { format:'module',shortCircuit:true,
      source:'export const useDashboard=()=>{throw new Error("Repair must not run the toggle path")}' }
    if (url.endsWith('/src/i18n.ts')) return { format: 'module', shortCircuit: true, source: 'export const t = key => key' }
    if (url.endsWith('/src/useNotify.ts')) return { format: 'module', shortCircuit: true,
      source: 'export const useNotify=()=>({warning:()=>{},error:value=>{throw new Error(value)}})' }
    return next(url, context)
  },
})
const { useDependencyNavigation, dependencyNavigationOpen: open,
  cancelDependencyNavigation: cancel, afterDependencyNavigationLeave: leave,
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
  'actionOf', 'dependencyRow', 'repairOf', 'repairDependencies', 'onTagClick', 'openDependencyJump',
], ['mods', 'maps', 'sets', 'repositories', 'gameProbed', 'refreshingHint', 'tagsOf',
  'levelSetCardAction', 'dependencyRepairStatus', 'levelSetRelationStatuses', 'levelSetTagKey',
  'dependencyRowFromStatus', 'useDependencyNavigation',
  'linkToModRepository', 'linkToMap', 't', 'pickI18n'])
const createMod = handlers('../src/views/ModsLocalView.vue', [
  'actionContext', 'cardAction', 'packageAction', 'packageRelations', 'packageRelationTag', 'currentPackage',
  'modRepairStatus', 'packageRepairStatus', 'repairModDependencies', 'repairPackageDependencies',
  'packageTagTargetId', 'openPackageTag', 'dependencyRow', 'openDependencyJump',
], ['mods', 'effectiveMods', 'repositories', 'groups', 'relationsByDll', 'statusActionFor',
  'repositoryForMod', 'dependencyRepairStatus', 'dependencyTagFor', 'relationJumpTarget', 'conflictRemovalTarget',
  'dependencyRowFromStatus', 'useDependencyNavigation',
  'linkToModRepository', 'isRelationCardKey', 't', 'pickI18n'])
const createOnlineMap = handlers('../src/views/MapsOnlineView.vue', [
  'mapRelationTag', 'dependencyRow', 'dependencyOf', 'jumpToDependency', 'openDependencyJump',
], ['maps', 'sets', 'mods', 'repositories', 'gameProbed', 'levelSetForMap', 'levelSetRelationStatuses',
  'dependencyTagFor', 'dependencyTagTitle', 'dependencyRowFromStatus', 'useDependencyNavigation', 'linkToModRepository', 't', 'pickI18n'])
const createOnlineMod = handlers('../src/views/ModsOnlineView.vue', [
  'repoRelationTag', 'dependencyRow', 'cardDependency', 'jumpToRelationRepository', 'openDependencyJump',
], ['mods', 'repositories', 'dependencyTagFor', 'modsForRepository', 'relationStatusesForSubject',
  'dependencyTagTitle', 'dependencyRowFromStatus', 'useDependencyNavigation', 'focusRepository', 't', 'pickI18n'])
const createModTags = handlers('../src/components/ModStatusTags.vue', ['relationRow', 'activate'], [
  'status', 'mods', 'repositories', 'isRelationCardKey', 'dependencyRowFromStatus', 'useDependencyNavigation',
  'openDependencyJump', 'repositoryId', 'linkToModRepository', 't', 'pickI18n',
])
const release = tag => ({ tag_name: tag, version: tag, name: tag, body: '', assets: [], prerelease: false, draft: false })
const repo = (id, guid, tags) => ({ id, name: id, plugin_guid: guid, download_mode: 'release', cached_releases: tags.map(release) })
const installed = (guid, version) => ({ guid, version, name: guid, dll_file: `${guid}.dll`, enabled: true,
  labels: [] })
const needsAction = 'relation_needs_action'
const unconfirmed = 'relation_unconfirmed'

function fixture(t, currentVersion = '4.0', targetVersion = 'v3.10.2', issueState = 'version_mismatch') {
  cancel(); leave()
  const mods = { value: [installed('web.runtime', currentVersion)] }, maps = { value: [] }
  const repositories = { value: [repo('runtime-source', 'web.runtime', [targetVersion])] }
  const relation = { guid: 'web.runtime', version: '>=3.10.2,<4', ...(issueState === 'conflict' ? { kind: 'conflicts' } : {}) }
  const owner = installed('owner', '1.0')
  owner.repository_id = 'owner'
  mods.value.push(owner)
  const set = { dir_name: 'Map', name: 'Map', author: '', version: '1.0', enabled: true, relations: [relation] }
  const sets = { value: [set] }, jumps = [], scope = effectScope()
  evaluationSnapshot([
    {id:'map:Map',kind:'map',dir:'Map',rows:[{relation,state:issueState}]},
    {id:'mod:owner',dlls:[owner.dll_file],rows:[{relation,state:issueState}]},
  ])
  const ownerRuntime = () => runtimeModStatus(owner, mods.value, repositories.value, { refreshing: {}, queued: {}, sourceRefreshing: {}, entryStateUnavailable: false })
  const relationsByDll = { get value() { return { [owner.dll_file]: ownerRuntime().relations } } }
  globalThis.__repairRequests=[]
  const pkg = { key: 'pkg:owner', mods: [owner], coreMods: [owner], statuses: [{ key: issueState === 'unknown' ? unconfirmed : needsAction }] }
  const effectiveMods = { get value() { return mods.value.map(mod => mod === owner ? ownerRuntime().mod : mod) } }
  const environment = { mods, maps, repositories, sets, effectiveMods, groups: { value: [{ packages: [pkg] }] },
    gameProbed: { value: true }, refreshingHint: { value: undefined }, relationsByDll,
    tagsOf: current => levelSetCardTags(current, maps.value, mods.value, true, repositories.value),
    levelSetCardAction, dependencyRepairStatus, levelSetRelationStatuses, levelSetTagKey,
    dependencyRowFromStatus, useDependencyNavigation, relationJumpTarget, conflictRemovalTarget,
    statusActionFor, repositoryForMod, isRelationCardKey, dependencyTagFor,
    linkToModRepository: (...args) => jumps.push(args), linkToMap: () => assert.fail('must target a MOD'),
    t: key => key, pickI18n: (messages, fallback) => fallback }
  const map = scope.run(() => createMap(environment)), mod = scope.run(() => createMod(environment))
  const localModTags = scope.run(() => createModTags({ ...environment,
    status: { get value() { return ownerRuntime() } }, repositoryId: { value: owner.repository_id },
    openDependencyJump: row => jumps.push([row.jump.id, row.jump.targetVersion]),
  }))
  const onlineEnvironment = { ...environment, levelSetForMap, modsForRepository, relationStatusesForSubject, dependencyTagTitle,
    focusRepository: (...args) => jumps.push(args) }
  const onlineMap = scope.run(() => createOnlineMap(onlineEnvironment)), onlineMod = scope.run(() => createOnlineMod(onlineEnvironment))
  t.after(() => { scope.stop(); cancel(); leave(); clearEvaluation() })
  return { mods, maps, repositories, sets, set, owner, ownerRuntime, pkg, jumps, map, mod, relation, localModTags, onlineMap, onlineMod }
}

for (const entry of ['map-action', 'map-tag', 'mod-action', 'package-action', 'package-tag']) {
  test(`${entry} sends its owner to the shared whole-chain repair plan`, async t => {
    const f = fixture(t)
    if (entry === 'map-action') f.map.repairDependencies(f.set)
    if (entry === 'map-tag') f.map.onTagClick(f.set, levelSetCardTags(f.set, [], f.mods.value, true, f.repositories.value)[0])
    if (entry === 'mod-action') f.mod.repairModDependencies(f.owner)
    if (entry === 'package-action') f.mod.repairPackageDependencies(f.pkg)
    if (entry === 'package-tag') f.mod.openPackageTag(f.pkg, needsAction)
    await new Promise(resolve=>setImmediate(resolve))
    assert.equal(open.value, false, 'the only confirmation belongs to the complete change plan')
    assert.deepEqual(f.jumps, [])
    assert.deepEqual(globalThis.__repairRequests, [{ repair_dependencies_of: entry.startsWith('map') ? 'map:Map' : 'mod:owner' }])
  })
}

test('card repair does not freeze a dependency release chosen from stale frontend caches', async t => {
  const f=fixture(t)
  f.repositories.value=[repo('runtime-source','web.runtime',['v999.0'])]
  assert.equal(f.map.repairOf(f.set).relation.guid, 'web.runtime')
  assert.equal(f.mod.modRepairStatus(f.owner).relation.guid, 'web.runtime')
  f.mod.repairModDependencies(f.owner)
  await new Promise(resolve=>setImmediate(resolve))
  assert.deepEqual(globalThis.__repairRequests, [{ repair_dependencies_of: 'mod:owner' }])
})

test('disabled map and MOD with updates can repair both kinds of unmet dependency', async t => {
  const f = fixture(t)
  f.set.enabled = f.owner.enabled = false
  const ownerRepo = { ...repo('owner', 'owner', ['2.0']), latest_tag: '2.0' }
  f.repositories.value.push(ownerRepo)
  f.owner.repository_id = ownerRepo.id
  f.owner.labels = ['update_available']
  const online = { id: 'map-online', name: 'Map', level_set_name: 'Map', author: 'Author', latest_tag: '2.0', latest_info_version: '2.0',
    cached_releases: [release('2.0')] }
  f.set.author = 'Author'
  f.maps.value = [online]
  const rows = [{ relation: f.relation, state: 'version_mismatch' }, { relation: { guid: 'extra' }, state: 'disabled' }]
  evaluationSnapshot([
    { id: 'map:Map', kind: 'map', dir: 'Map', enabled: false, rows },
    { id: 'mod:owner', repo: ownerRepo.id, dlls: [f.owner.dll_file], enabled: false, rows },
  ])
  f.pkg.statuses = [{ key: 'update_available' }, { key: needsAction }]
  assert.equal(f.map.actionOf(f.set)?.kind, 'update')
  assert.equal(f.mod.packageAction(f.pkg)?.action.kind, 'update')
  assert.equal(f.map.repairOf(f.set)?.state, 'version_mismatch')
  assert.equal(f.mod.packageRepairStatus(f.pkg)?.state, 'version_mismatch')
  assert.equal(f.mod.packageRelationTag(f.pkg, needsAction)?.relation.guid, f.relation.guid)
  const tags = levelSetCardTags(f.set, f.maps.value, f.mods.value, true, f.repositories.value)
  assert.equal(tags[0]?.text, 'tools.relationNeedsAction')
  assert.equal(tags.filter(tag => tag.kind === 'dependency').length, 1)
  f.map.onTagClick(f.set, tags[0])
  f.mod.repairPackageDependencies(f.pkg)
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(globalThis.__repairRequests, [{ repair_dependencies_of: 'map:Map' }, { repair_dependencies_of: 'mod:owner' }])
})

for (const state of ['missing', 'disabled', 'version_mismatch', 'conflict']) {
  for (const kind of ['map', 'mod']) test(`online ${kind} ${state} tag uses the installed owner's authoritative repair`, async t => {
    const f = fixture(t, '4.0', 'v3.10.2', state)
    const map = { id: 'online-map', name: 'Map', level_set_name: 'Map', level_set_name_zh: '', author: '', latest_tag: 'v9' }
    const ownerRepo = repo('owner', 'owner', ['v9'])
    f.set.author = ''
    f.maps.value = [map]
    f.repositories.value.push(ownerRepo)
    evaluationSnapshot([
      { id: 'map:Map', kind: 'map', dir: 'Map', map: map.id, rows: [{ relation: f.relation, state }] },
      { id: 'mod:owner', repo: ownerRepo.id, dlls: [f.owner.dll_file], rows: [{ relation: f.relation, state }] },
      // Current cards must keep using installed facts while a different release is open.
      { id: 'subject:map:online-map:v9', kind: 'map', map: map.id, tag: 'v9', enabled: false,
        rows: [{ relation: f.relation, state: 'satisfied' }] },
      { id: 'subject:mod:owner:v9', repo: ownerRepo.id, tag: 'v9', enabled: false,
        rows: [{ relation: f.relation, state: 'satisfied' }] },
    ])
    const tag = kind === 'map' ? f.onlineMap.dependencyOf(map) : f.onlineMod.cardDependency(ownerRepo)
    assert.equal(tag?.text, 'tools.relationNeedsAction')
    assert.equal(tag?.clickable, true, 'the shared OnlineCard must receive a clickable repair tag')
    if (kind === 'map') f.onlineMap.jumpToDependency(map)
    else f.onlineMod.jumpToRelationRepository(ownerRepo)
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(f.jumps, [], 'repair node IDs must never be treated as repository navigation IDs')
    assert.deepEqual(globalThis.__repairRequests, [{ repair_dependencies_of: kind === 'map' ? 'map:Map' : 'mod:owner' }])
  })
}

test('both online card templates connect the visible dependency tag to their actual repair handler', () => {
  for (const [view, property, event] of [
    ['MapsOnlineView.vue', ':dependency="dependencyOf(map)"', '@dependency-click="jumpToDependency(map)"'],
    ['ModsOnlineView.vue', ':dependency="cardDependency(repo)"', '@dependency-click="jumpToRelationRepository(repo)"'],
  ]) {
    const source = readFileSync(new URL(`../src/views/${view}`, import.meta.url), 'utf8')
    const cards = [...source.matchAll(/<OnlineCard\b[\s\S]*?\/>/g)]
    assert.ok(cards.length > 0)
    for (const [card] of cards) {
      assert.ok(card.includes(property), `${view} binds its tag projection`)
      assert.ok(card.includes(event), `${view} binds its repair handler`)
    }
  }
})

for (const state of ['missing', 'disabled', 'version_mismatch', 'conflict']) {
  for (const entry of ['single-mod-tag', 'package-tag', 'local-map-tag']) test(`${entry} ${state} sends repair owners through the shared plan`, async t => {
    const f = fixture(t, '4.0', 'v3.10.2', state)
    if (entry === 'package-tag') f.mod.openPackageTag(f.pkg, needsAction)
    else if (entry === 'local-map-tag') f.map.onTagClick(f.set, levelSetCardTags(f.set, [], f.mods.value, true, f.repositories.value)[0])
    else f.localModTags.activate(needsAction)
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(f.jumps, [])
    assert.deepEqual(globalThis.__repairRequests, [{ repair_dependencies_of: entry === 'local-map-tag' ? 'map:Map' : 'mod:owner' }])
  })
}

test('unknown relations cannot start repair or navigation from any of the four pages', async t => {
  const f = fixture(t, '4.0', 'v3.10.2', 'unknown')
  const ownerRepo = repo('owner', 'owner', ['v9'])
  const map = { id: 'map-online', name: 'Map', level_set_name: 'Map', author: 'A' }
  f.set.author = 'A'; f.maps.value = [map]; f.repositories.value.push(ownerRepo)
  const localMapTag = levelSetCardTags(f.set, f.maps.value, f.mods.value, true)[0]
  for (const tag of [localMapTag, f.ownerRuntime().relationTag, f.onlineMap.dependencyOf(map), f.onlineMod.cardDependency(ownerRepo)]) {
    assert.equal(tag?.text, 'tools.relationUnconfirmed')
    assert.equal(tag?.clickable, false)
  }
  for (const row of [installedModDependencyRows(f.owner, { mods: f.mods.value })[0],
    levelSetDependencyRows(f.set, { mods: f.mods.value, maps: f.maps.value })[0]]) {
    assert.equal(row.status, 'tools.relationUnconfirmed')
    assert.equal(row.jump, undefined)
  }
  f.map.onTagClick(f.set, localMapTag); f.map.repairDependencies(f.set)
  f.localModTags.activate(unconfirmed); f.mod.repairModDependencies(f.owner)
  f.mod.openPackageTag(f.pkg, unconfirmed); f.mod.repairPackageDependencies(f.pkg)
  f.onlineMap.jumpToDependency(map); f.onlineMod.jumpToRelationRepository(ownerRepo)
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(f.jumps, [])
  assert.deepEqual(globalThis.__repairRequests, [])
  assert.equal(open.value, false)
})

test('online cards do not display an assessment for an uninstalled Release or bind a MOD by GUID alone', t => {
  const f = fixture(t)
  const map = { id: 'map-online', name: 'Map', level_set_name: 'Map', author: 'other-author', latest_tag: 'v9' }
  const repoWithSameGuid = repo('another-owner', f.owner.guid, ['v9'])
  evaluationSnapshot([
    { id: 'mod:owner', repo: f.owner.repository_id, dlls: [f.owner.dll_file], rows: [{ relation: f.relation, state: 'missing' }] },
    { id: 'subject:map:v9', kind: 'map', map: map.id, tag: 'v9', rows: [{ relation: f.relation, state: 'missing' }] },
    { id: 'subject:mod:v9', repo: repoWithSameGuid.id, tag: 'v9', rows: [{ relation: f.relation, state: 'missing' }] },
  ])
  assert.equal(f.onlineMap.dependencyOf(map), null)
  assert.equal(f.onlineMod.cardDependency(repoWithSameGuid), null)
})
