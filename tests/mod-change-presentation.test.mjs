import assert from 'node:assert/strict'
import test from 'node:test'
import { changePlanDetails, changeStatusKey, preparationCompletion } from '../src/modChanges/presentation.ts'

const action = (node, kind = 'downgrade', extra = {}) => ({ node_id: node, kind, reason: '', caused_by: [], ...extra })
const node = (id, label) => ({ node_id: id, label, members: [], dll_files: [], candidates: [] })
const plan = { status: 'ready', blockers: [], nodes: [node('web', 'DIYLevel Web 前置'), node('level', 'DIYLevel - 自定义关卡')], roots: [],
  actions: [action('web', 'downgrade', { from_tag: 'v4.0.0', to_tag: 'v3.10.2' }),
    action('level', 'enable', { from_tag: 'v0.10', to_tag: 'v0.10' }),
    action('level', 'replace', { from_tag: 'v0.10', to_tag: 'v0.10' })] }

test('keeps each action in solve order and only shows versions for package changes', () => {
  const { rows } = changePlanDetails(plan)
  assert.deepEqual(rows.map(row => [row.kind, row.label, row.version]), [
    ['downgrade', 'DIYLevel Web 前置', { from: 'v4.0.0', to: 'v3.10.2' }],
    ['enable', 'DIYLevel - 自定义关卡', undefined],
    ['replace', 'DIYLevel - 自定义关卡', { from: 'v0.10', to: 'v0.10' }],
  ])
})

test('projects authoritative progress by node identity and includes newly discovered download targets', () => {
  const actions = [action('level', 'enable', { from_tag: 'v0.10', to_tag: 'v0.10', member_key: 'opaque-member-id' }),
    action('web', 'disable', { from_tag: 'v4.0.0', to_tag: 'v4.0.0' }),
    action('new', 'install')]
  const progress = { actions, items: [{ node_id: 'level', label: 'Level', tag: 'v0.10', stage: 'ready' },
    { node_id: 'new', label: 'New dependency', tag: 'v1.0', stage: 'queued' }] }
  const { rows } = changePlanDetails(plan, progress)
  assert.deepEqual(rows.slice(0, 2).map(row => row.version), [undefined, undefined])
  assert.equal(rows[0].statusKey, changeStatusKey('level', 'opaque-member-id'))
  assert.equal(rows[1].preparationStage, undefined)
  assert.equal(rows[2].label, 'New dependency')
  assert.equal(rows[2].version.from, undefined)
  assert.equal(rows[2].version.to, 'v1.0')
})

test('uses the latest solved actions and removes candidates abandoned during preparation', () => {
  const actions = [action('web', 'upgrade', { from_tag: 'v4', to_tag: 'v5' })]
  const { rows } = changePlanDetails(plan, { actions, items: [{ node_id: 'web', label: 'Web', tag: 'v5', stage: 'downloading' }] })
  assert.equal(rows.length, 1)
  assert.equal(rows[0].kind, 'upgrade')
  assert.deepEqual(rows[0].version, { from: 'v4', to: 'v5' })
  assert.equal(rows[0].preparationStage, 'downloading')
  assert.deepEqual(changePlanDetails(plan, { actions: [], items: [] }).rows, [])
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

test('uncovered selected targets and newly discovered candidates remain visible in the same table', () => {
  const { rows } = changePlanDetails({ ...plan,
    roots: [{ node_id: 'level', label: 'Chosen Level', tag: 'v0.10' },
      { node_id: 'web', label: 'Chosen Web', tag: 'v3.10.2' },
      { node_id: 'missing', label: 'Chosen local map', tag: 'v1' }],
  }, { actions: [action('web', 'downgrade', { to_tag: 'v3.10.2' })],
    items: [{ node_id: 'web', stage: 'ready', label: 'Prepared Web' }, { node_id: 'new', stage: 'queued', label: 'Discovered', tag: 'v2' }] })
  assert.deepEqual(rows.map(({ label, version }) => [label, version?.to]).sort(), [
    ['Chosen Level', 'v0.10'], ['Chosen local map', 'v1'], ['Discovered', 'v2'], ['Prepared Web', 'v3.10.2'],
  ])
})

test('a rejected Release keeps the selected tag distinct from the DLL version actually evaluated', () => {
  const { rows } = changePlanDetails({ ...plan, status: 'unsatisfiable', roots: [{ node_id: 'web', label: 'Web', tag: 'v4.0.0' }],
    blockers: [{ traces: [[{ owner_id: 'web', owner_version: '3.10.2', relation: { guid: 'com.example.missing' }, targets: [],
      state: 'missing', reason: 'required_plugin_missing' }]] }],
  })
  assert.deepEqual(rows.map(row => [row.version?.to, row.selected, row.diagnostic?.path]), [
    ['v4.0.0', true, undefined], ['3.10.2', false, '1'], [undefined, undefined, undefined],
  ])
})

const diagnosticEdge = (owner_id, owner_version, guid, version, targets, state = 'satisfied') => ({
  owner_id, owner_version, relation: { kind: 'requires', guid, version }, targets, state, reason: state,
})
const provider = (node_id, version, enabled = true) => ({ node_id, version, enabled })

test('failed paths share their exact common prefix but retain both DIYLevel versions and distinct starts', () => {
  const entry = diagnosticEdge('web', 'v4.0.0', 'runtime', '>=4', [provider('runtime', '4.1')])
  const level = version => diagnosticEdge('runtime', '4.1', 'diylevel', '>=0.11', [provider('level', version)])
  const fast = version => diagnosticEdge('level', version, 'fastinit', '>=1.9', [provider('old', '1.6.4')], 'version_mismatch')
  const reverse = diagnosticEdge('guard', '1.0', 'web', '<4', [provider('web', 'v4.0.0')], 'conflict')
  const missing = diagnosticEdge('level', '0.11.0', 'missing', '>=2', [], 'missing')
  const { rows } = changePlanDetails({ ...plan, status: 'unsatisfiable',
    roots: [{ node_id: 'web', label: 'Web', tag: 'v4.0.0' }], blockers: [
      { traces: [[entry, level('0.11.0'), fast('0.11.0')], [reverse]] },
      { traces: [[{ ...entry, relation: { ...entry.relation } }, level('0.11.1'), fast('0.11.1')],
        [entry, level('0.11.0'), missing], [{ ...entry, owner_version: 'v3.10.2' }, level('0.11.0'), fast('0.11.0')]] },
    ] })
  assert.deepEqual(rows.map(row => [row.nodeId, row.version?.to, row.diagnostic?.path, row.diagnostic?.depth, row.diagnostic?.terminal?.state]), [
    ['web', 'v4.0.0', '1', undefined, undefined],
    ['runtime', '4.1', undefined, 1, undefined],
    ['level', '0.11.0', '1.1', 2, undefined],
    ['old', '1.6.4', '1.1.1', 3, 'version_mismatch'], ['level', undefined, '1.1.2', 3, 'missing'],
    ['level', '0.11.1', '1.2', 2, undefined],
    ['old', '1.6.4', undefined, 3, 'version_mismatch'],
    ['guard', '1.0', '2', undefined, undefined],
    ['web', 'v4.0.0', undefined, 1, 'conflict'],
    ['web', 'v3.10.2', '3', undefined, undefined],
    ['runtime', '4.1', undefined, 1, undefined],
    ['level', '0.11.0', undefined, 2, undefined],
    ['old', '1.6.4', undefined, 3, 'version_mismatch'],
  ])
  assert.deepEqual(rows.filter(row => row.selected).map(row => row.version?.to), ['v4.0.0'])
})

test('a terminal and a continuing prefix stay separate even when their evidence is otherwise identical', () => {
  const edge = diagnosticEdge('web', 'v4.0.0', 'diylevel', '>=0.11', [provider('level', '0.11.0')])
  const unknown = { owner_id: 'level', owner_version: '0.11.0', targets: [], state: 'unknown', reason: 'candidate_relations_unknown' }
  const differentRange = { ...edge, relation: { ...edge.relation, version: '>=0.11.0,<0.12' } }
  const { rows } = changePlanDetails({ ...plan, status: 'incomplete', blockers: [
    { traces: [[edge], [edge, unknown], [differentRange, unknown]] },
  ] })
  assert.deepEqual(rows.map(row => [row.diagnostic?.path, row.diagnostic?.edge?.step.relation.version, row.diagnostic?.terminal?.reason]), [
    ['1', undefined, undefined], ['1.1', '>=0.11', 'satisfied'],
    ['1.2', '>=0.11', undefined], [undefined, undefined, 'candidate_relations_unknown'],
    ['1.3', '>=0.11.0,<0.12', undefined], [undefined, undefined, 'candidate_relations_unknown'],
  ])
})

test('bundle rows follow their map, merge explicit member actions and exclude external map DLLs', () => {
  const member = (key, dllFile, enabled = true) => ({ key, dll_file: dllFile, enabled })
  const internal = 'BepInEx/plugins/OC2DIYLevel/Levels/Map/Addon.dll'
  const second = 'BepInEx/plugins/OC2DIYLevel/Levels/Map/Guard.dll'
  const alreadyDisabled = 'BepInEx/plugins/OC2DIYLevel/Levels/Map/Preserved.dll.disabled'
  const external = 'BepInEx/plugins/External.dll'
  const externalId = `map-dll:${external.toLowerCase()}`
  const { rows } = changePlanDetails({ ...plan, nodes: [
    { ...node('map', '地图合集'), kind: 'map', map_id: 'test-map', members: [member('addon', internal), member('guard', second), member('preserved', alreadyDisabled, false)] },
    { ...node(externalId, '外置插件'), kind: 'mod', map_id: 'test-map', members: [member('external', external)] },
    ...plan.nodes,
  ], actions: [
    action('map', 'disable'), action('web', 'downgrade', { to_tag: 'v3.10.2' }),
    action('map', 'disable', { member_key: 'guard', reason_details: [{ kind: 'requested', related_node_ids: [] }] }),
    action(externalId, 'disable', { member_key: 'external' }),
  ] })
  const children = rows.filter(row => row.bundled)
  assert.equal(rows[0].label, '地图合集')
  assert.deepEqual(rows.slice(1, 1 + children.length), children)
  assert.deepEqual(children.map(row => row.member.path).sort(), [internal, second].sort())
  assert.ok(children.every(row => row.kind === 'disable' && row.version === undefined))
  const explicit = children.find(row => row.member.path === second)
  assert.equal(explicit.statusKey, changeStatusKey('map', 'guard'))
  assert.deepEqual(explicit.reasons.map(reason => reason.kind), ['requested'])
  const implicit = children.find(row => row.member.path === internal)
  assert.equal(implicit.statusKey, changeStatusKey('map'))
  assert.equal(implicit.bundleReason, 'disabled')
  assert.equal(rows.filter(row => row.member?.path === second).length, 1)
  assert.equal(rows.filter(row => row.member?.path === external && !row.bundled).length, 1)
})
