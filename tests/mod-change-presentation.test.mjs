import assert from 'node:assert/strict'
import test from 'node:test'
import { changePlanRows, preparationCompletion } from '../src/modChanges/presentation.ts'

const action = (node, kind = 'downgrade', extra = {}) => ({ node_id: node, kind, reason: '', caused_by: [], ...extra })
const node = (id, label) => ({ node_id: id, label, members: [], dll_files: [], candidates: [] })
const plan = { nodes: [node('web', 'DIYLevel Web 前置'), node('level', 'DIYLevel - 自定义关卡')], roots: [],
  actions: [action('web', 'downgrade', { from_tag: 'v4.0.0', to_tag: 'v3.10.2' }),
    action('level', 'enable', { member_key: 'opaque-member-id' }), action('level', 'replace', { from_tag: 'v0.10', to_tag: 'v0.10' })] }

test('groups package switches with the version action while preserving the complete details', () => {
  const rows = changePlanRows(plan)
  assert.deepEqual(rows.map(row => [row.nodeId, row.label, row.action.kind]), [
    ['web', 'DIYLevel Web 前置', 'downgrade'], ['level', 'DIYLevel - 自定义关卡', 'replace']])
  assert.equal(rows[1].actions.length, 2)
  assert.equal(rows[1].actions[0].member_key, 'opaque-member-id')
  assert.equal(plan.actions.length, 3)
})

test('projects authoritative progress by node identity and includes newly discovered download targets', () => {
  const progress = { actions: plan.actions, items: [{ node_id: 'level', label: 'Level', stage: 'verifying' },
    { node_id: 'new', label: 'New dependency', stage: 'queued' }] }
  const rows = changePlanRows(plan, progress)
  assert.equal(rows[0].item, undefined)
  assert.equal(rows[1].item.stage, 'verifying')
  assert.equal(rows[2].label, 'New dependency')
  assert.equal(rows[2].action, undefined)
})

test('uses the latest solved actions and removes candidates abandoned during preparation', () => {
  const actions = [action('web', 'upgrade', { from_tag: 'v4', to_tag: 'v5' })]
  const rows = changePlanRows(plan, { actions, items: [{ node_id: 'web', label: 'Web', tag: 'v5', stage: 'downloading' }] })
  assert.equal(rows.length, 1)
  assert.equal(rows[0].action.kind, 'upgrade')
  assert.equal(rows[0].action.to_tag, 'v5')
  assert.deepEqual(changePlanRows(plan, { actions: [], items: [] }), [])
})

test('measures only verified packages and does not treat downloaded bytes as validated contents', () => {
  assert.deepEqual(preparationCompletion(null), { completed: 0, total: 0, percentage: null })
  const items = [{ stage: 'ready' }, { stage: 'verifying', download: { downloaded: 100, total: 100, finished: true } }, { stage: 'failed' }]
  assert.deepEqual(preparationCompletion({ items }), { completed: 1, total: 3, percentage: 33 })
  assert.deepEqual(preparationCompletion({ items: [items[0]] }), { completed: 1, total: 1, percentage: 100 })
})
