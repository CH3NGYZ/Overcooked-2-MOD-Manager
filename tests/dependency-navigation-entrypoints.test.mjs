import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks, stripTypeScriptTypes } from 'node:module'
import test from 'node:test'
import { parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import { effectScope } from 'vue'
import { dependencyActionLabel, dependencyRowFromStatus, levelSetDependencyRows } from '../src/dependencyRows.ts'
import { levelSetCardAction, levelSetCardTags, levelSetForMap, levelSetRelationStatuses } from '../src/levelSetStatus.ts'
import { dependencyTagFor, primaryRelation, relationJumpRepositoryId, relationStatusFromEvaluation, resolveRelations } from '../src/modRelations.ts'
import { repositoryForMod } from '../src/modStatus.ts'
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
const createOnlineMap = handlers('../src/views/MapsOnlineView.vue', [
  'mapDependencyStatuses', 'dependencyOf', 'jumpToDependency', 'openDependencyJump',
], ['maps', 'sets', 'mods', 'repositories', 'gameProbed', 'levelSetForMap', 'levelSetRelationStatuses',
  'dependencyTagFor', 'primaryRelation', 'relationJumpRepositoryId', 'hasLocalMap', 'describeFramework',
  'dependencyRowFromStatus', 'useDependencyNavigation', 'linkToModRepository', 't', 'pickI18n'])
const createOnlineMod = handlers('../src/views/ModsOnlineView.vue', [
  'unmetRelation', 'relationTag', 'relationJumpId', 'cardDependency', 'jumpToRelationRepository', 'openDependencyJump',
], ['mods', 'repositories', 'dependencyTagFor', 'primaryRelation', 'relationJumpRepositoryId', 'resolveRelations',
  'hasLocalInstall', 'relationTagTitle', 'dependencyRowFromStatus', 'useDependencyNavigation', 'focusRepository', 't', 'pickI18n'])
const createModTags = handlers('../src/components/ModStatusTags.vue', ['activate'], [
  'status', 'mods', 'repositories', 'isDependencyTag', 'dependencyRowFromStatus', 'useDependencyNavigation',
  'openDependencyJump', 'targetOf', 'linkToModRepository', 't', 'pickI18n',
])
const release = tag => ({ tag_name: tag, version: tag, name: tag, body: '', assets: [], prerelease: false, draft: false })
const repo = (id, guid, tags) => ({ id, name: id, plugin_guid: guid, download_mode: 'release', cached_releases: tags.map(release) })
const installed = (guid, version) => ({ guid, version, name: guid, dll_file: `${guid}.dll`, enabled: true,
  labels: ['missing_dependency'] })
const isDependencyTag = key => ['missing_dependency', 'dependency_conflict', 'dependency_installable'].includes(key)

function fixture(t, currentVersion = '4.0', targetVersion = 'v3.10.2', issueState = 'version_mismatch') {
  cancel(); leave()
  const mods = { value: [installed('web.runtime', currentVersion)] }, maps = { value: [] }
  const repositories = { value: [repo('runtime-source', 'web.runtime', [targetVersion])] }
  const relation = { guid: 'web.runtime', version: '>=3.10.2,<4', ...(issueState === 'conflict' ? { kind: 'conflicts' } : {}) }
  const owner = installed('owner', '1.0')
  mods.value.push(owner)
  const set = { dir_name: 'Map', name: 'Map', version: '1.0', enabled: true, relations: [relation] }
  const sets = { value: [set] }, jumps = [], scope = effectScope()
  const snapshot = evaluationSnapshot([
    {id:'map:Map',kind:'map',dir:'Map',rows:[{relation,state:issueState}]},
    {id:'mod:owner',dlls:[owner.dll_file],rows:[{relation,state:issueState}]},
  ])
  const issues = { get value() { return { [owner.dll_file]: relationStatusFromEvaluation(snapshot.evaluations[1].runtime[0], mods.value) } } }
  globalThis.__repairRequests=[]
  const pkg = { key: 'pkg:owner', mods: [owner], statuses: [{ key: 'missing_dependency' }] }
  const environment = { mods, maps, repositories, sets, effectiveMods: mods, groups: { value: [{ packages: [pkg] }] },
    gameProbed: { value: true }, refreshingHint: { value: undefined }, relationIssueByDll: issues,
    tagsOf: current => levelSetCardTags(current, maps.value, mods.value, true, repositories.value),
    levelSetCardAction, dependencyActionLabel, dependencyRowFromStatus, useDependencyNavigation,
    statusActionFor, repositoryForMod, isDependencyTag,
    linkToModRepository: (...args) => jumps.push(args), linkToMap: () => assert.fail('must target a MOD'),
    t: key => key, pickI18n: (messages, fallback) => fallback }
  const map = scope.run(() => createMap(environment)), mod = scope.run(() => createMod(environment))
  t.after(() => { scope.stop(); cancel(); leave(); clearEvaluation() })
  return { mods, maps, repositories, sets, set, owner, pkg, jumps, map, mod, relation, issues }
}

for (const entry of ['map-action', 'map-tag', 'mod-action', 'package-action', 'package-tag']) {
  test(`${entry} sends its owner to the shared whole-chain repair plan`, async t => {
    const f = fixture(t)
    if (entry === 'map-action') await f.map.runCardAction(f.set)
    if (entry === 'map-tag') f.map.onTagClick(f.set, levelSetCardTags(f.set, [], f.mods.value, true, f.repositories.value)[0])
    if (entry === 'mod-action') f.mod.runCardAction(f.owner)
    if (entry === 'package-action') f.mod.runPackageAction(f.pkg)
    if (entry === 'package-tag') f.mod.openPackageTag(f.pkg, 'missing_dependency')
    await new Promise(resolve=>setImmediate(resolve))
    assert.equal(open.value, false, 'the only confirmation belongs to the complete change plan')
    assert.deepEqual(f.jumps, [])
    assert.deepEqual(globalThis.__repairRequests, [{roots:[{node_id:entry.startsWith('map')?'map:Map':'mod:owner',enabled:true}]}])
  })
}

test('card repair does not freeze a dependency release chosen from stale frontend caches', async t => {
  const f=fixture(t)
  f.repositories.value=[repo('runtime-source','web.runtime',['v999.0'])]
  assert.equal(f.map.actionOf(f.set).label,'modChanges.reviewSelection')
  assert.equal(f.mod.cardAction(f.owner).action.label,'modChanges.reviewSelection')
  f.mod.runCardAction(f.owner)
  await new Promise(resolve=>setImmediate(resolve))
  assert.deepEqual(globalThis.__repairRequests,[{roots:[{node_id:'mod:owner',enabled:true}]}])
})

for (const state of ['missing', 'disabled', 'version_mismatch', 'conflict']) {
  for (const kind of ['map', 'mod']) test(`online ${kind} ${state} tag uses the installed owner's authoritative repair`, async t => {
    const f = fixture(t)
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
    ])
    const environment = { mods: f.mods, maps: f.maps, sets: f.sets, repositories: f.repositories,
      gameProbed: { value: true }, levelSetForMap, levelSetRelationStatuses, dependencyTagFor, primaryRelation,
      relationJumpRepositoryId, resolveRelations, dependencyRowFromStatus, useDependencyNavigation,
      hasLocalMap: entry => !!levelSetForMap(entry, f.sets.value), hasLocalInstall: () => true,
      describeFramework: entry => entry.guid, relationTagTitle: () => '',
      linkToModRepository: (...args) => f.jumps.push(args), focusRepository: (...args) => f.jumps.push(args),
      t: key => key, pickI18n: (_messages, fallback) => fallback }
    const scope = effectScope()
    t.after(() => scope.stop())
    const view = scope.run(() => kind === 'map' ? createOnlineMap(environment) : createOnlineMod(environment))
    const tag = kind === 'map' ? view.dependencyOf(map) : view.cardDependency(ownerRepo)
    assert.equal(tag?.clickable, true, 'the shared OnlineCard must receive a clickable repair tag')
    if (kind === 'map') view.jumpToDependency(map)
    else view.jumpToRelationRepository(ownerRepo)
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(f.jumps, [], 'repair node IDs must never be treated as repository navigation IDs')
    assert.deepEqual(globalThis.__repairRequests, [{ roots: [{ node_id: kind === 'map' ? 'map:Map' : 'mod:owner', enabled: true }] }])
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
    const tagKey = state === 'conflict' ? 'dependency_conflict' : 'missing_dependency'
    if (entry === 'package-tag') f.mod.openPackageTag(f.pkg, tagKey)
    else if (entry === 'local-map-tag') f.map.onTagClick(f.set, levelSetCardTags(f.set, [], f.mods.value, true, f.repositories.value)[0])
    else {
      const scope = effectScope()
      t.after(() => scope.stop())
      const component = scope.run(() => createModTags({ status: { get value() { return { issue: f.issues.value[f.owner.dll_file] } } },
        mods: f.mods, repositories: f.repositories, isDependencyTag, dependencyRowFromStatus, useDependencyNavigation,
        openDependencyJump: row => f.jumps.push(row.jump.id), targetOf: () => 'mod:owner',
        linkToModRepository: (...args) => f.jumps.push(args), t: key => key, pickI18n: (_messages, fallback) => fallback }))
      component.activate(tagKey)
    }
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(f.jumps, [])
    assert.deepEqual(globalThis.__repairRequests, [{ roots: [{ node_id: entry === 'local-map-tag' ? 'map:Map' : 'mod:owner', enabled: true }] }])
  })
}
