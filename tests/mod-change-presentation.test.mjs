import assert from 'node:assert/strict'
import test from 'node:test'
import { changePlanDetails, changePlanRows, preparationCompletion } from '../src/modChanges/presentation.ts'

const action = (node, kind = 'downgrade', extra = {}) => ({ node_id: node, kind, reason: '', caused_by: [], ...extra })
const node = (id, label) => ({ node_id: id, label, members: [], dll_files: [], candidates: [] })
const plan = { nodes: [node('web', 'DIYLevel Web 前置'), node('level', 'DIYLevel - 自定义关卡')], roots: [],
  actions: [action('web', 'downgrade', { from_tag: 'v4.0.0', to_tag: 'v3.10.2' }),
    action('level', 'enable', { from_tag: 'v0.10', to_tag: 'v0.10' }),
    action('level', 'replace', { from_tag: 'v0.10', to_tag: 'v0.10' })] }

test('groups package switches with the version action while preserving the complete details', () => {
  const rows = changePlanRows(plan)
  assert.deepEqual(rows.map(row => [row.nodeId, row.label, row.version]), [
    ['web', 'DIYLevel Web 前置', { from: 'v4.0.0', to: 'v3.10.2' }],
    ['level', 'DIYLevel - 自定义关卡', { from: 'v0.10', to: 'v0.10' }]])
  assert.deepEqual(rows[1].actions.map(({ kind }) => kind), ['enable', 'replace'])
  assert.equal(plan.actions.length, 3)
})

test('projects authoritative progress by node identity and includes newly discovered download targets', () => {
  const actions = [action('level', 'enable', { from_tag: 'v0.10', to_tag: 'v0.10', member_key: 'opaque-member-id' }),
    action('web', 'disable', { from_tag: 'v4.0.0', to_tag: 'v4.0.0' }),
    action('new', 'install')]
  const progress = { actions, items: [{ node_id: 'level', label: 'Level', tag: 'v0.10', stage: 'ready' },
    { node_id: 'new', label: 'New dependency', tag: 'v1.0', stage: 'queued' }] }
  const rows = changePlanRows(plan, progress)
  assert.deepEqual(rows.slice(0, 2).map(row => row.version), [undefined, undefined])
  assert.equal(rows[0].actions[0].member_key, 'opaque-member-id')
  assert.equal(rows[0].item.stage, 'ready')
  assert.equal(rows[1].item, undefined)
  assert.equal(rows[2].label, 'New dependency')
  assert.equal(rows[2].version.from, undefined)
  assert.equal(rows[2].version.to, 'v1.0')
})

test('uses the latest solved actions and removes candidates abandoned during preparation', () => {
  const actions = [action('web', 'upgrade', { from_tag: 'v4', to_tag: 'v5' })]
  const rows = changePlanRows(plan, { actions, items: [{ node_id: 'web', label: 'Web', tag: 'v5', stage: 'downloading' }] })
  assert.equal(rows.length, 1)
  assert.equal(rows[0].actions[0].kind, 'upgrade')
  assert.deepEqual(rows[0].version, { from: 'v4', to: 'v5' })
  assert.deepEqual(changePlanRows(plan, { actions: [], items: [] }), [])
})

test('measures only verified packages and does not treat downloaded bytes as validated contents', () => {
  assert.deepEqual(preparationCompletion(null), { completed: 0, total: 0, percentage: null })
  const items = [{ stage: 'ready' }, { stage: 'verifying', download: { downloaded: 100, total: 100, finished: true } }, { stage: 'failed' }]
  assert.deepEqual(preparationCompletion({ items }), { completed: 1, total: 3, percentage: 33 })
  assert.deepEqual(preparationCompletion({ items: [items[0]] }), { completed: 1, total: 1, percentage: 100 })
})

test('details keep individual actions and explain them with names and ranges instead of repeating opaque impacts', () => {
  const requirement = { kind: 'required_by', related_node_ids: ['level', 'web', 'unknown'], requirement: '>=3.10.2' }
  const model = changePlanDetails({ ...plan, impacts: ['mod:uuid', 'map:uuid'], actions: [
    action('web', 'disable', { from_tag: 'v4.0.0', to_tag: 'v4.0.0', reason_details: [
      { kind: 'conflicts_with', related_node_ids: ['level'], requirement: '<0.11', prerelease: false },
    ] }),
    action('web', 'downgrade', { from_tag: 'v4.0.0', to_tag: 'v3.10.2', reason_details: [requirement, requirement], caused_by: ['ignored'] }),
    action('level', 'enable', { caused_by: ['level', 'unknown', 'web'] }),
  ] })
  assert.deepEqual(model.rows.map(row => [row.kind, row.label, row.version]), [
    ['disable', 'DIYLevel Web 前置', undefined],
    ['downgrade', 'DIYLevel Web 前置', { from: 'v4.0.0', to: 'v3.10.2' }],
    ['enable', 'DIYLevel - 自定义关卡', undefined],
  ])
  assert.deepEqual(model.rows[0].reasons.map(({ kind, related, requirement, prerelease }) => [kind, related, requirement, prerelease]),
    [['conflicts_with', ['DIYLevel - 自定义关卡'], '<0.11', false]])
  assert.deepEqual(model.rows[1].reasons.map(({ kind, related, requirement }) => [kind, related, requirement]),
    [['required_by', ['DIYLevel - 自定义关卡'], '>=3.10.2']])
  assert.deepEqual(model.rows[1].causes, [])
  assert.deepEqual(model.rows[2].causes, ['DIYLevel Web 前置'])
  assert.doesNotMatch(JSON.stringify(model), /mod:uuid|map:uuid|unknown|ignored/)
})

test('details keep a selected version when not represented by an action and list only named package DLLs', () => {
  const dllFile = 'BepInEx\\plugins\\Package\\Level.dll.disabled'
  const model = changePlanDetails({ ...plan,
    roots: [{ node_id: 'level', label: 'Chosen Level', tag: 'v0.10' },
      { node_id: 'web', label: 'Chosen Web', tag: 'v3.10.2' },
      { node_id: 'missing', label: 'Chosen local map', tag: 'v1' }],
    nodes: [{ ...plan.nodes[0], members: [{ key: '', dll_file: '', enabled: true }] },
      { ...plan.nodes[1], members: [{ key: 'level-key', dll_file: dllFile, enabled: false }] }],
  }, { actions: [action('web', 'downgrade', { to_tag: 'v3.10.2' }), action('level', 'disable', { member_key: 'level-key' })],
    items: [{ node_id: 'web', stage: 'ready', label: 'Prepared Web' }, { node_id: 'new', stage: 'queued', label: 'Discovered' }] })
  assert.deepEqual(model.roots.map(({ label, tag }) => [label, tag]), [['Chosen Level', 'v0.10'], ['Chosen local map', 'v1']])
  assert.deepEqual(model.packages, [{ nodeId: 'level', label: 'DIYLevel - 自定义关卡', plugins: [{ name: 'Level.dll.disabled', path: dllFile, enabled: false }] }])
  assert.deepEqual(model.rows[1].member, { name: 'Level.dll.disabled', path: dllFile })
  assert.equal(model.rows[0].label, 'Prepared Web')
  assert.equal(model.rows[0].stage, 'ready')
  assert.deepEqual(model.pending.map(({ label }) => label), ['Discovered'])
})
