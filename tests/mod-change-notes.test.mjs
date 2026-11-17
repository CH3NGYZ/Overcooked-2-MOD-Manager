import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
registerHooks({ resolve(specifier, context, next) { if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
  try { return next(`${specifier}.ts`, context) } catch { /* normal resolution below */ }
} return next(specifier, context) } })
const { changePlanInstallNotes } = await import('../src/modChanges/planNotes.ts')
const repo = (id, plugin_guid) => ({ id, name: id, plugin_guid, note: `${id} instructions` })
const repos = [repo('root', 'guid-root'), repo('fix', 'guid-fix'), repo('other', 'guid-other')]
const action = (node_id, kind = 'install') => ({ node_id, kind })
const plan = (actions, nodes = [], status = 'ready') => ({ status, actions, nodes })

test('manual preview and bundled plugin GUIDs include all matching first-install notes', () => {
  const p = plan([action('preview:uuid')], [{ node_id: 'preview:uuid', members: [{ guid: ' GUID-ROOT ' }, { guid: 'guid-fix' }] }])
  assert.deepEqual(changePlanInstallNotes(p, repos, []).map(note => note.id), ['root', 'fix'])
})
test('repository roots and backend source summaries need no inferred installed identity', () => {
  assert.deepEqual(changePlanInstallNotes(plan([action('mod:root')]), repos, []).map(note => note.id), ['root'])
  assert.deepEqual(changePlanInstallNotes(plan([action('preview:uuid')], [{ node_id: 'preview:uuid', repository_id: 'fix' }]), repos, []).map(note => note.id), ['fix'])
})
test('managed members skip reading while unrelated and toggle-only nodes add no notes', () => {
  const p = plan([action('mod:root'), action('mod:fix', 'enable')])
  assert.deepEqual(changePlanInstallNotes(p, repos, [{ guid: 'guid-root', package_id: 'managed' }]), [])
  assert.deepEqual(changePlanInstallNotes({ ...p, status: 'incomplete' }, repos, []), [])
})
