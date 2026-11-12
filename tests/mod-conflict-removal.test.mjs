import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks, stripTypeScriptTypes } from 'node:module'
import test from 'node:test'
import { parse } from '@vue/compiler-sfc'
import ts from 'typescript'

// Keep the production page handlers, relation resolution and removal composable.
// Only dialogs, IPC and translated notifications are replaced at their boundaries.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'mod-removal:dialog', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const modules = {
      'mod-removal:dialog': 'export const useDialog = () => globalThis.__modRemovalBoundary.dialog',
      '/src/services/tauri.ts': 'export const api = { uninstallLocalMod: (...args) => globalThis.__modRemovalBoundary.uninstall(...args) }',
      '/src/useNotify.ts': 'export const useNotify = () => globalThis.__modRemovalBoundary.notices',
      '/src/i18n.ts': 'export const t = key => key',
      '/src/locale/backend.ts': 'export const translateBackendMessage = value => value ?? ""; export const backendMessageOf = error => String(error)',
    }
    for (const [suffix, source] of Object.entries(modules)) {
      if (url.endsWith(suffix)) return { format: 'module', shortCircuit: true, source }
    }
    return next(url, context)
  },
})

const { useModRemoval } = await import('../src/composables/useModRemoval.ts')
const { resolveRelation } = await import('../src/modRelations.ts')
const { repositoryForMod } = await import('../src/modStatus.ts')
const { statusActionFor } = await import('../src/statusActions.ts')
const { isDependencyTag } = await import('../src/tagTargets.ts')
const { descriptor } = parse(readFileSync(new URL('../src/views/ModsLocalView.vue', import.meta.url), 'utf8'))
const script = descriptor.scriptSetup.content
const ast = ts.createSourceFile('ModsLocalView.ts', script, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
const names = ['actionContext', 'cardAction', 'relationOwnerMod', 'packageAction', 'runCardAction', 'runPackageAction']
const functions = names.map(name => {
  const statement = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name)
  assert.ok(statement, `production handler ${name} must exist`)
  return statement.getText(ast)
})
const removal = ast.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(declaration =>
  ts.isCallExpression(declaration.initializer) && declaration.initializer.expression.getText(ast) === 'useModRemoval'))
assert.ok(removal, 'page must configure its removal through the shared composable')
const createActions = new Function('environment', `${stripTypeScriptTypes(`
  const { useModRemoval, blockModOperation, deleteConfirm, modTitle, busy, load,
    statusActionFor, repositoryForMod, isDependencyTag, repositories, relationIssueByDll,
    previewModRepository, linkToModRepository, refreshModRepository } = environment
  ${removal.getText(ast)}
  ${functions.join('\n')}
`)}\nreturn { runCardAction, runPackageAction }`)

const mod = name => ({ name, guid: `test.${name}`, dll_file: `${name}.dll`, version: '1.0.0', enabled: true,
  version_source: 'plugin', update_available: false, duplicate_guid: false, status: 'installed', labels: ['dependency_conflict'] })
function setup({ responses = [{ ok: true, message: '' }], blocked = false } = {}) {
  const a = mod('A'), b = mod('B'), dialogs = [], calls = [], notices = [], events = [], busy = { value: {} }
  const state = { blocked, refreshes: 0 }
  const boundary = {
    dialog: { warning(options) { const dialog = { ...options }; dialogs.push(dialog); return dialog } },
    notices: Object.fromEntries(['success', 'error'].map(kind => [kind, message => notices.push([kind, message])])),
    async uninstall(...args) {
      calls.push(args); events.push({ dll: args[0], busy: { ...busy.value } })
      const response = responses[calls.length - 1]
      if (response instanceof Error) throw response
      assert.ok(response, 'an unexpected additional uninstall must fail')
      return response
    },
  }
  globalThis.__modRemovalBoundary = boundary
  const issues = Object.fromEntries([[a, b], [b, a]].map(([source, target]) => [source.dll_file,
    resolveRelation({ kind: 'conflicts', guid: target.guid }, [a, b], [])]))
  const actions = createActions({
    useModRemoval, statusActionFor, repositoryForMod, isDependencyTag, busy,
    blockModOperation: () => state.blocked,
    deleteConfirm: { value: { body: name => `Remove ${name}?`, action: 'Move to recycle bin', done: 'Moved to recycle bin' } },
    modTitle: target => target.name, load: async () => { state.refreshes++ },
    repositories: { value: [] }, relationIssueByDll: { value: issues },
    previewModRepository: () => assert.fail('conflicts must not install'),
    linkToModRepository: () => assert.fail('conflicts must not navigate'),
    refreshModRepository: () => assert.fail('conflicts must not refresh an unrelated repository'),
  })
  return { ...actions, a, b, dialogs, calls, notices, events, busy, state, issues }
}
const packageFor = source => ({ mods: [mod('unrelated'), source], statuses: [{ key: 'dependency_conflict' }] })

for (const entry of ['card', 'package']) test(`${entry} conflict removes the opposite InstalledMod through the ordinary confirmation and refresh`, async () => {
  for (const direction of ['A', 'B']) {
    const fixture = setup(), { a, b, dialogs, calls, busy, events, notices, state } = fixture
    const [source, target] = direction === 'A' ? [a, b] : [b, a]
    if (entry === 'card') fixture.runCardAction(source)
    else fixture.runPackageAction(packageFor(source))
    assert.equal(dialogs.length, 1)
    assert.equal(dialogs[0].content, `Remove ${target.name}?`)
    assert.equal(dialogs[0].positiveText, 'Move to recycle bin')
    assert.deepEqual(calls, [], 'opening confirmation must not uninstall')
    assert.equal(state.refreshes, 0)
    await dialogs[0].onPositiveClick()
    assert.deepEqual(calls, [[target.dll_file]])
    assert.deepEqual(events, [{ dll: target.dll_file, busy: { [target.dll_file]: true } }])
    assert.deepEqual(busy.value, {})
    assert.equal(dialogs[0].loading, true)
    assert.equal(dialogs[0].negativeButtonProps.disabled, true)
    assert.equal(state.refreshes, 1)
    assert.deepEqual(notices, [['success', 'Moved to recycle bin']])
  }
})

test('cancelling conflict confirmation leaves both mods and dashboard untouched', () => {
  const fixture = setup()
  fixture.runCardAction(fixture.a)
  fixture.dialogs[0].onNegativeClick?.()
  assert.deepEqual(fixture.calls, []); assert.deepEqual(fixture.busy.value, {}); assert.equal(fixture.state.refreshes, 0)
})

test('running game blocks both opening and confirming a conflict removal', async () => {
  const fixture = setup({ blocked: true })
  fixture.runCardAction(fixture.a); fixture.runPackageAction(packageFor(fixture.b))
  assert.equal(fixture.dialogs.length, 0); assert.deepEqual(fixture.calls, [])
  fixture.state.blocked = false
  fixture.runCardAction(fixture.a)
  fixture.state.blocked = true
  assert.equal(await fixture.dialogs[0].onPositiveClick(), false)
  assert.deepEqual(fixture.calls, []); assert.deepEqual(fixture.busy.value, {}); assert.equal(fixture.state.refreshes, 0)
})

for (const flag of ['requires_package_confirmation', 'requires_confirmation']) test(`${flag} preserves the original second confirmation and retries the opposite DLL`, async () => {
  const fixture = setup({ responses: [{ ok: false, [flag]: true, package_mod_names: ['B', 'Shared'] }, { ok: true, message: 'Removed package' }] })
  fixture.runPackageAction(packageFor(fixture.a))
  await fixture.dialogs[0].onPositiveClick()
  assert.deepEqual(fixture.calls, [['B.dll']]); assert.equal(fixture.dialogs.length, 2)
  assert.equal(fixture.dialogs[1].title, 'modsLocal.thisModBelongsToAPackage')
  fixture.state.blocked = true
  assert.equal(await fixture.dialogs[1].onPositiveClick(), false)
  assert.deepEqual(fixture.calls, [['B.dll']], 'starting the game before package confirmation still blocks')
  fixture.state.blocked = false
  await fixture.dialogs[1].onPositiveClick()
  assert.deepEqual(fixture.calls, [['B.dll'], ['B.dll', true]])
  assert.equal(fixture.dialogs[1].loading, true)
  assert.equal(fixture.state.refreshes, 2, 'both completed attempts use the existing dashboard reload')
  assert.deepEqual(fixture.notices, [['success', 'Removed package']])
})

test('cancelling the package confirmation never grants allow_unmanaged', async () => {
  const fixture = setup({ responses: [{ ok: false, requires_package_confirmation: true }] })
  fixture.runCardAction(fixture.a)
  await fixture.dialogs[0].onPositiveClick()
  fixture.dialogs[1].onNegativeClick?.()
  assert.deepEqual(fixture.calls, [['B.dll']]); assert.equal(fixture.state.refreshes, 1)
})

test('an uninstall error releases only the conflict target busy state and reports the existing error', async () => {
  const fixture = setup({ responses: [new Error('backend refused')] })
  fixture.runCardAction(fixture.a)
  await fixture.dialogs[0].onPositiveClick()
  assert.deepEqual(fixture.calls, [['B.dll']]); assert.deepEqual(fixture.busy.value, {})
  assert.equal(fixture.state.refreshes, 0); assert.deepEqual(fixture.notices, [['error', 'Error: backend refused']])
})

test('a missing conflict target gives no action and cannot fall back to deleting the current card', () => {
  const fixture = setup()
  delete fixture.issues[fixture.a.dll_file]
  fixture.runCardAction(fixture.a); fixture.runPackageAction(packageFor(fixture.a))
  assert.deepEqual(fixture.dialogs, []); assert.deepEqual(fixture.calls, [])
})
