import assert from 'node:assert/strict'
import test from 'node:test'
import { createChangeWorkflow } from '../src/modChanges/workflow.ts'

const plan = (id = 'plan-1', extra = {}) => ({ plan_id: id, status: 'ready', actions: [], evaluations: [],
  blockers: [], impacts: [], roots: [{ node_id: 'mod:owner', label: 'Owner', tag: 'v1' }], nodes: [], install_notes: [], prepared: false, ...extra })
const request = { roots: [{ node_id: 'mod:owner', candidate_id: 'release:owner:v1', enabled: true }] }
const manualRequest = { roots: [{ node_id: 'mod:owner', enabled: true }] }
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const tick = () => new Promise(resolve => setImmediate(resolve))
const rootInstall = { node_id: 'mod:owner', kind: 'install', to_candidate_id: 'release:owner:v1' }
const dependency = (id, owner, kind = 'install') => ({
  node_id: id, kind, to_candidate_id: `${id}:candidate`,
  from_candidate_id: kind === 'install' ? null : `${id}:candidate`,
  reason_details: [{ kind: 'required_by', related_node_ids: [owner] }],
})
function fixture(overrides = {}) {
  const calls = [], states = []
  const backend = {
    plan: async value => { calls.push(['plan', value]); return plan() },
    prepare: async id => { calls.push(['prepare', id]); return plan(id, { prepared: true }) },
    execute: async id => { calls.push(['execute', id]); return { ok: true, message: 'modChanges.completed' } },
    cancel: async id => { calls.push(['cancel', id]) }, ...overrides,
  }
  const workflow = createChangeWorkflow(backend, value => states.push(value))
  return { ...workflow, calls, states, get state() { return states.at(-1) } }
}

test('one confirmation still applies a plan whose actions need no package', async () => {
  const f = fixture(), pending = f.run(manualRequest)
  await tick()
  assert.equal(f.state.phase, 'confirming')
  assert.deepEqual(f.calls, [['plan', manualRequest]])
  await f.confirm()
  assert.deepEqual(await pending, { ok: true, message: 'modChanges.completed' })
  assert.deepEqual(f.calls.map(call => call[0]), ['plan', 'prepare', 'execute'])
  assert.equal(f.calls[2][1], 'plan-1')
  assert.equal(f.state, null)
})

test('exact install, upgrade, RAW and ordinary dependency repair apply without review', async () => {
  const dependencies = [dependency('mod:transitive', 'mod:dependency', 'enable'), dependency('mod:dependency', 'mod:owner')]
  const raw = { roots: [{ ...request.roots[0], candidate_id: 'raw:owner' }] }
  for (const [intent, rootActions] of [
    [request, [rootInstall]],
    [request, [{ ...rootInstall, kind: 'upgrade', from_candidate_id: 'installed:owner' }]],
    [raw, [{ ...rootInstall, to_candidate_id: 'raw:owner' }]],
    [{ repair_dependencies_of: 'mod:owner' }, []],
  ]) {
    const f = fixture({ prepare: async () => plan('verified', { prepared: true, actions: [...rootActions, ...dependencies],
      impacts: ['mod:owner', 'mod:dependency', 'mod:transitive'] }) })
    assert.equal((await f.run(intent)).ok, true)
    assert.deepEqual(f.calls, [['plan', intent], ['execute', 'verified']])
    assert.deepEqual(f.states.filter(Boolean).map(state => state.phase), ['planning', 'preparing', 'prepared', 'executing'])
  }
})

test('ordinary repair that is already satisfied executes the backend no-op', async () => {
  const f = fixture(), intent = { repair_dependencies_of: 'mod:owner' }
  assert.equal((await f.run(intent)).ok, true)
  assert.deepEqual(f.calls, [['plan', intent], ['prepare', 'plan-1'], ['execute', 'plan-1']])
})

test('routine repairs can reach a missing dependency through an unchanged compatible provider', async () => {
  const final = plan('verified', {
    prepared: true,
    actions: [dependency('mod:missing', 'mod:bridge')],
    evaluations: [{ node_id: 'mod:owner', runtime: [{ state: 'satisfied', relation: { kind: 'requires' }, targets: ['mod:bridge'] }] }],
    impacts: ['mod:owner', 'mod:missing'],
  })
  const f = fixture({ prepare: async () => final })
  assert.equal((await f.run({ repair_dependencies_of: 'mod:owner' })).ok, true)
  assert.deepEqual(f.calls.at(-1), ['execute', 'verified'])
})

test('high-impact and unproven actions get one final review', async () => {
  const outside = dependency('mod:outside', 'mod:unrelated')
  for (const extra of [
    { actions: [rootInstall, { ...dependency('mod:dependency', 'mod:owner'), kind: 'upgrade' }] },
    { actions: [rootInstall, { node_id: 'mod:dependent', kind: 'disable' }] },
    { actions: [{ ...rootInstall, to_candidate_id: 'release:owner:v2' }] },
    { actions: [rootInstall, outside] },
    { actions: [rootInstall, outside], evaluations: [{ node_id: 'mod:owner', runtime: [
      { state: 'satisfied', relation: { kind: 'requires' }, targets: ['mod:other', 'mod:unrelated'] },
    ] }] },
    { actions: [rootInstall, { ...outside, reason_details: [] }] },
    { actions: [rootInstall], impacts: ['mod:unexplained'] },
    { notices: ['modChanges.targetChanged'] },
    { install_notes: [{ id: 'new-note' }] },
    { untracked_overwrites: [{ node_id: 'mod:owner', path: 'loose.dll' }] },
    { retired_mod_copies: { 'mod:owner': [{ member_key: 'old-dll' }] } },
  ]) {
    const f = fixture({ prepare: async () => plan('verified', { prepared: true, actions: [rootInstall], ...extra }) })
    const pending = f.run(request)
    await tick()
    assert.equal(f.state.phase, 'reconfirming')
    assert.equal(f.state.applyReview, 'prepared')
    assert.deepEqual(f.calls, [['plan', request]])
    await f.cancel()
    assert.equal(await pending, null)
    assert.deepEqual(f.calls.at(-1), ['cancel', 'verified'])
  }
})

for (const stalePhase of ['prepare', 'execute']) test(`refreshes typed ${stalePhase} staleness once with the exact original asset`, async () => {
  const calls = [], ids = ['original', 'replanned'], scope = 'game/source/intent'
  let plans = 0
  const submitted = { roots: [{ ...request.roots[0], asset_name: 'exact.zip' }] }
  const f = fixture({
    plan: async value => { calls.push(structuredClone(value)); return plan(ids[plans++], { replan_scope: scope }) },
    prepare: async id => {
      if (id === ids[0] && stalePhase === 'prepare') throw new Error('modChanges.inventoryChanged')
      return plan(id, { prepared: true, replan_scope: scope, actions: [rootInstall] })
    },
    execute: async id => {
      calls.push(id)
      return id === ids[0] && stalePhase === 'execute' ? { ok: false, message: 'modChanges.inventoryChanged' } : { ok: true, message: '' }
    },
  })
  const pending = f.run(submitted)
  submitted.roots[0].asset_name = 'different.zip'
  assert.equal((await pending).ok, true)
  const frozen = { roots: [{ ...request.roots[0], asset_name: 'exact.zip' }] }
  assert.deepEqual(calls, stalePhase === 'prepare' ? [frozen, frozen, 'replanned'] : [frozen, 'original', frozen, 'replanned'])
  assert.deepEqual(f.calls, [['cancel', 'original']])
})

for (const change of ['new-upgrade', 'different-version', 'different-candidate', 'different-files', 'same-upgrade']) {
  test(`automatic replanning reviews an upgrade only when its installed target changed: ${change}`, async () => {
    const upgrade = { ...rootInstall, kind: 'upgrade', from_candidate_id: 'installed:owner', from_tag: 'v0.9', to_tag: 'v1' }
    const beforeAction = change === 'new-upgrade' ? rootInstall : upgrade
    const afterAction = change === 'different-version' ? { ...upgrade, from_tag: 'v0.8' }
      : change === 'different-candidate' ? { ...upgrade, from_candidate_id: 'installed:other' } : upgrade
    const beforePath = 'BepInEx/plugins/Owner/Main.dll'
    const afterPath = change === 'different-files' ? 'BepInEx/plugins/NewOwner/Main.dll' : beforePath
    const helperPath = 'BepInEx/plugins/Owner/Helper.dll'
    const installed = (dll_files) => ({ node_id: 'mod:owner', kind: 'mod', dll_files })
    const beforeNodes = [installed(change === 'new-upgrade' ? [] : [beforePath, helperPath])]
    const afterNodes = [installed([helperPath, afterPath])]
    const tentative = change === 'different-files' ? beforeNodes : afterNodes
    let plans = 0
    const f = fixture({
      plan: async () => ++plans === 1
        ? plan('original', { replan_scope: 'same', actions: [beforeAction], nodes: beforeNodes })
        : plan('fresh', { replan_scope: 'same', actions: [afterAction], nodes: tentative }),
      prepare: async id => {
        if (id === 'original') throw new Error('modChanges.inventoryChanged')
        return plan(id, { prepared: true, replan_scope: 'same', actions: [afterAction], nodes: afterNodes })
      },
    })
    const pending = f.run(request)
    await tick()
    if (change !== 'same-upgrade') {
      assert.equal(f.state?.phase, 'reconfirming')
      assert.equal(f.state.plan.actions[0].kind, 'upgrade')
      assert.deepEqual(f.calls, [['cancel', 'original']])
      await f.confirm()
    }
    assert.equal((await pending).ok, true)
    assert.deepEqual(f.calls, [['cancel', 'original'], ['execute', 'fresh']])
  })
}

for (const [tag, previous, fresh, refreshes] of [
  ['modChanges.inventoryChanged', 'same', 'same', true],
  ['modChanges.inventoryChanged', undefined, 'same', false],
  ['modChanges.inventoryChanged', 'same', undefined, true],
  ['modChanges.inventoryChanged', 'old', 'new', true],
  ['modChanges.targetChanged', 'same', 'same', false],
  ['modChanges.recoveryRequired', 'same', 'same', false],
  ['download.cancelled', 'same', 'same', false],
]) test(`bounded refresh when ${tag} with ${previous} -> ${fresh}`, async () => {
  let plans = 0, prepares = 0
  const f = fixture({
    plan: async () => ++plans === 1 ? plan('original', { replan_scope: previous }) : plan('replanned', { replan_scope: fresh }),
    prepare: async () => { prepares++; throw new Error(tag) },
  })
  const pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'failed')
  assert.equal(plans, refreshes ? 2 : 1)
  assert.equal(prepares, refreshes && previous === fresh ? 2 : 1)
  assert.equal(f.calls.some(call => call[0] === 'execute'), false)
  await f.cancel(); assert.equal(await pending, null)
})

test('auto-replanning cancels the old plan and discards destructive consent', async () => {
  let plans = 0, executions = 0
  const f = fixture({
    plan: async () => plan(++plans === 1 ? 'original' : 'new', { replan_scope: 'same' }),
    prepare: async id => plan(id, { prepared: true, replan_scope: 'same', actions: [rootInstall],
      untracked_overwrites: [{ node_id: 'mod:owner', path: id === 'original' ? 'old.dll' : 'new.dll' }] }),
    execute: async () => ({ ok: ++executions > 1, message: 'modChanges.inventoryChanged' }),
  })
  const pending = f.run(request)
  await tick()
  await f.confirm()
  assert.equal(f.state.phase, 'reconfirming')
  assert.equal(f.state.plan.plan_id, 'new')
  assert.equal(f.state.plan.untracked_overwrites[0].path, 'new.dll')
  assert.equal(executions, 1)
  assert.deepEqual(f.calls, [['cancel', 'original']])
  await f.confirm()
  assert.equal((await pending).ok, true)
})

test('update operation gate prevents package work across preparation', async () => {
  let blocked = true
  const f = fixture({ operationsBlocked: () => blocked,
    prepare: async id => { blocked = true; return plan(id, { prepared: true, actions: [rootInstall] }) } })
  await assert.rejects(f.run(request), /update.inProgress/)
  blocked = false
  const pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'failed')
  assert.equal(f.state.error.message, 'update.inProgress')
  assert.deepEqual(f.calls, [['plan', request]])
  await f.cancel(); assert.equal(await pending, null)
})

for (const kind of ['downgrade', 'replace']) test(`exact ${kind} gets one confirmation after preparation`, async () => {
  const presentation = deferred(), actions = [{ node_id: 'mod:owner', kind, to_candidate_id: request.roots[0].candidate_id }]
  const f = fixture({ plan: async () => plan('same', { actions }),
    prepare: async () => plan('same', { actions, prepared: true }), presentPrepared: () => presentation.promise })
  const pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'prepared')
  await f.confirm()
  await f.cancel()
  assert.equal(f.state.phase, 'prepared')
  assert.deepEqual(f.calls, [])
  presentation.resolve()
  await tick()
  assert.equal(f.state.phase, 'reconfirming')
  assert.equal(f.state.applyReview, 'prepared')
  assert.equal(f.state.plan.plan_id, 'same')
  assert.deepEqual(f.calls, [])
  await f.confirm()
  assert.equal((await pending).ok, true)
  assert.deepEqual(f.calls, [['execute', 'same']])
})

test('a prepared local preview or staged ZIP can be cancelled without writing', async () => {
  for (const [request, node_id] of [
    [{ roots: [{ node_id: 'preview:payload', enabled: true }] }, 'preview:payload'],
    [{ roots: [], map_import: { zip_path: 'local.zip', selected: ['map'] } }, 'map:local'],
  ]) {
    const actions = [{ node_id, kind: 'replace' }]
    const f = fixture({ plan: async () => plan('unchanged', { actions, prepared: true }) })
    const pending = f.run(request)
    await tick()
    assert.equal(f.state.phase, 'reconfirming')
    assert(f.states.some(state => state?.phase === 'prepared'))
    assert.deepEqual(f.calls, [])
    await f.cancel(); assert.equal(await pending, null)
    assert.deepEqual(f.calls, [['cancel', 'unchanged']])
  }
})

test('changed downloaded content requires one confirmation of the actual downgrade', async () => {
  const f = fixture({ prepare: async () => plan('verified-2', { prepared: true,
    actions: [{ node_id: 'mod:dependency', kind: 'downgrade', to_tag: 'v2' }] }) })
  const pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'reconfirming')
  assert.equal(f.state.plan.plan_id, 'verified-2')
  assert.equal(f.calls.some(call => call[0] === 'execute'), false)
  await f.confirm(); await pending
  assert.deepEqual(f.calls.filter(call => call[0] === 'execute'), [['execute', 'verified-2']])
})

function toggle(kind, enabled, memberKey) {
  const id = `${kind}:owner`, candidate = `installed:${id}`
  const root = { node_id: id, enabled, ...(memberKey ? { member_key: memberKey } : {}) }
  const node = { node_id: id, kind, dll_files: kind === 'mod' ? ['Owner.dll'] : [],
    members: memberKey ? [{ key: memberKey, dll_file: 'Owner.dll' }] : [] }
  // A single DLL's physical enablement can appear as a node-level action.
  const action = { node_id: id, kind: enabled ? 'enable' : 'disable', member_key: null,
    from_candidate_id: candidate, to_candidate_id: candidate }
  return { request: { roots: [root], toggle_scope: { generation: 41, revision: 'scope-1' } },
    choice: plan('switch', { roots: [root], nodes: [node], actions: [action], impacts: [id] }) }
}

test('a single map or MOD switch commits silently through normal preparation', async () => {
  for (const [kind, enabled, member] of [['map', true], ['map', false], ['mod', false, 'owner-dll'], ['mod', true, 'owner-dll']]) {
    const { request, choice } = toggle(kind, enabled, member), checked = []
    const f = fixture({ plan: async () => choice, prepare: async id => { checked.push(id); return { ...choice, prepared: true } } })
    assert.equal((await f.run(request)).ok, true)
    assert.deepEqual(checked, ['switch'])
    assert.deepEqual(f.calls, [['execute', 'switch']])
    assert.deepEqual(f.states.filter(Boolean).map(state => [state.phase, state.silent]),
      [['planning', true], ['preparing', true], ['executing', true]])
  }
})

test('an extra dependency action, a different package member, a notice, or a blocker still opens review', async () => {
  const { request, choice } = toggle('mod', true, 'owner-dll')
  const [action] = choice.actions
  for (const plan of [
    { ...choice, actions: [action, { ...action, node_id: 'mod:dependent' }], impacts: ['mod:owner', 'mod:dependent'] },
    { ...choice, actions: [action, { ...action, kind: 'disable', node_id: 'mod:conflict' }], impacts: ['mod:owner', 'mod:conflict'] },
    { ...choice, actions: [{ ...action, member_key: 'different-dll' }] },
    { ...choice, notices: ['levelSet.localMapSourceConflict'] },
    { ...choice, status: 'incomplete', blockers: [{ code: 'search_exhausted' }] },
  ]) {
    const f = fixture({ plan: async () => plan }), pending = f.run(request)
    await tick()
    assert.equal(f.state.phase, 'confirming')
    assert.equal(f.state.silent, undefined)
    assert.deepEqual(f.calls, [])
    await f.cancel(); assert.equal(await pending, null)
  }
})

test('a silent switch cannot authorize extra changes discovered during preparation', async () => {
  const { request, choice } = toggle('map', true)
  const f = fixture({ plan: async () => choice, prepare: async () => ({ ...choice, plan_id: 'verified', prepared: true,
    actions: [...choice.actions, { node_id: 'mod:dependency', kind: 'enable' }], impacts: [...choice.impacts, 'mod:dependency'] }) })
  const pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'reconfirming')
  assert.equal(f.state.silent, undefined)
  assert.deepEqual(f.calls, [])
  await f.confirm(); assert.equal((await pending).ok, true)
  assert.deepEqual(f.calls, [['execute', 'verified']])
})

test('untracked replacements require approval even for the same plan with no package action', async () => {
  const { request, choice } = toggle('mod', true, 'owner-dll')
  const overwrites = [{ node_id: 'mod:owner', path: 'BepInEx/plugins/Owner.dll' }]
  for (const finalAction of ['cancel', 'confirm']) {
    const f = fixture({ plan: async () => choice, prepare: async id => {
      assert.equal(id, choice.plan_id)
      return { ...choice, prepared: true, untracked_overwrites: overwrites }
    } })
    const pending = f.run(request)
    await tick()
    assert.equal(f.state.phase, 'reconfirming')
    assert.equal(f.state.applyReview, 'prepared')
    assert.equal(f.state.silent, undefined)
    assert.deepEqual(f.state.plan.untracked_overwrites, overwrites)
    assert.deepEqual(f.calls, [])
    if (finalAction === 'cancel') {
      await f.cancel()
      assert.equal(await pending, null)
      assert.deepEqual(f.calls, [['cancel', choice.plan_id]])
    } else {
      await Promise.all([f.confirm(), f.confirm()])
      assert.equal((await pending).ok, true)
      assert.deepEqual(f.calls, [['execute', choice.plan_id]])
    }
  }
})

test('a failed direct switch displays its error and does not retry by itself', async () => {
  const { request, choice } = toggle('map', false)
  let calls = 0
  const f = fixture({ plan: async () => ({ ...choice, prepared: true }), execute: async () => {
    calls++
    return { ok: false, message: 'modChanges.targetChanged' }
  } })
  const pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'failed')
  assert.equal(f.state.error, 'modChanges.targetChanged')
  assert.equal(f.state.silent, undefined)
  assert.equal(calls, 1)
  await f.cancel(); assert.equal(await pending, null)
})

test('a stale click scope stays attached on retry and never applies another context', async () => {
  const { request, choice } = toggle('map', false)
  const scopes = []
  const f = fixture({ plan: async value => {
    scopes.push(value.toggle_scope)
    if (value.toggle_scope) throw new Error('modChanges.targetChanged')
    return choice
  } })
  const pending = f.run(request)
  await tick(); await f.retry()
  assert.deepEqual(scopes, [request.toggle_scope, request.toggle_scope])
  assert.equal(f.state.phase, 'failed')
  assert.match(f.state.error.message, /modChanges.targetChanged/)
  assert.deepEqual(f.calls, [])
  await f.cancel(); assert.equal(await pending, null)
})

for (const status of ['unsatisfiable', 'incomplete']) test(`${status} plans cannot prepare or execute`, async () => {
  const f = fixture({ plan: async () => plan('blocked', { status, blockers: [{ code: 'relation_has_no_solution' }] }) })
  const pending = f.run(request)
  await tick(); await f.confirm()
  assert.equal(f.state.plan.status, status)
  assert.deepEqual(f.calls, [])
  await f.cancel(); assert.equal(await pending, null)
})

test('validation that becomes blocked never executes and leaves its explanation visible', async () => {
  const f = fixture({ prepare: async () => plan('blocked', { status: 'unsatisfiable', prepared: true,
    blockers: [{ code: 'root_ownership_unsafe', message: 'root_ownership_unsafe' }] }) })
  const pending = f.run(request)
  await tick(); await f.confirm(); await f.confirm()
  assert.equal(f.state.phase, 'reconfirming')
  assert.equal(f.state.plan.blockers[0].code, 'root_ownership_unsafe')
  assert.equal(f.calls.some(call => call[0] === 'execute'), false)
  await f.cancel(); await pending
})

test('cancelling planning closes immediately and cleans a late backend plan', async () => {
  const delayed = deferred(), f = fixture({ plan: () => delayed.promise }), pending = f.run(request)
  await f.cancel()
  assert.equal(f.state, null)
  delayed.resolve(plan('late'))
  assert.equal(await pending, null)
  assert.deepEqual(f.calls, [['cancel', 'late']])
})

test('preparing/executing cannot be cancelled and duplicate confirmations never run a second commit', async () => {
  const preparing = deferred(), executing = deferred()
  const f = fixture({ prepare: () => preparing.promise, execute: id => { f.calls.push(['execute', id]); return executing.promise } })
  const pending = f.run(request)
  await tick()
  const confirming = f.confirm()
  await f.cancel(); await f.confirm()
  assert.equal(f.state.phase, 'preparing')
  preparing.resolve(plan('plan-1', { prepared: true }))
  await tick(); await f.cancel(); await f.confirm()
  assert.equal(f.state.phase, 'executing')
  executing.resolve({ ok: true, message: '' })
  await confirming; await pending
  assert.deepEqual(f.calls.filter(call => call[0] === 'execute'), [['execute', 'plan-1']])
})

test('execution failure stays visible with no automatic retry or partial-row continuation', async () => {
  const f = fixture({ execute: async id => { f.calls.push(['execute', id]); return { ok: false, message: 'modChanges.recoveryRequired' } } })
  const pending = f.run(request)
  await tick(); await f.confirm(); await tick()
  assert.equal(f.state.phase, 'failed')
  assert.equal(f.state.error, 'modChanges.recoveryRequired')
  assert.deepEqual(f.calls.map(call => call[0]), ['plan', 'prepare', 'execute'])
  await f.cancel(); await pending
})

test('an old rejected plan cannot replace a newer request after cancellation', async () => {
  const old = deferred(); let count = 0
  const f = fixture({ plan: () => ++count === 1 ? old.promise : Promise.resolve(plan('new')) })
  const pendingOld = f.run(request)
  await f.cancel()
  const pendingNew = f.run(manualRequest)
  await tick(); old.reject(new Error('stale')); await pendingOld
  assert.equal(f.state.plan.plan_id, 'new')
  assert.equal(f.state.phase, 'confirming')
  await f.cancel(); await pendingNew
})

test('retry freezes the initial request, cleans failed preparation and requires a new confirmation', async () => {
  const submitted = { roots: [{ node_id: 'mod:owner', candidate_id: 'v1', enabled: true }] }
  let attempts = 0
  const f = fixture({
    plan: async value => { f.calls.push(['plan', value]); return plan('planned', { replan_scope: 'same' }) },
    prepare: async id => {
      if (!attempts++) throw new Error('download.failed')
      return plan(id, { prepared: true, replan_scope: 'same' })
    },
  })
  const pending = f.run(submitted)
  submitted.roots[0].candidate_id = 'different'
  await tick(); await f.confirm()
  assert.equal(f.state.phase, 'failed')
  await f.retry()
  assert.equal(f.state.phase, 'reconfirming')
  assert.deepEqual(f.calls.filter(call => call[0] === 'plan').at(-1)[1], { roots: [{ node_id: 'mod:owner', candidate_id: 'v1', enabled: true }] })
  assert.equal(f.calls.some(call => call[0] === 'execute'), false)
  assert.equal(f.calls.filter(call => call[0] === 'cancel').length, 1)
  await f.confirm(); assert.equal((await pending).ok, true)
})

test('manual retries cannot rebind a failed request to a changed target scope', async () => {
  let plans = 0
  const f = fixture({
    plan: async value => { f.calls.push(['plan', value]); return plans++ === 0
      ? plan('original', { replan_scope: 'game-a' }) : plan('other', { replan_scope: 'game-b' }) },
    prepare: async id => { f.calls.push(['prepare', id]); throw new Error('download.failed') },
  })
  const pending = f.run(request)
  await tick()
  await f.retry(); await f.retry(); await f.confirm()
  assert.equal(f.state.phase, 'failed')
  assert.equal(f.state.error, 'modChanges.targetChanged')
  assert.deepEqual(f.calls, [['plan', request], ['prepare', 'original'], ['cancel', 'original'],
    ['plan', request], ['cancel', 'other'], ['plan', request], ['cancel', 'other']])
  await f.cancel(); assert.equal(await pending, null)
})

test('cancelling a replan rejects its late progress and plan, without reviving the dialog', async () => {
  const delayed = deferred(); let plans = 0
  const f = fixture({ plan: async () => ++plans === 1 ? plan() : delayed.promise,
    prepare: async () => { throw new Error('network') } })
  const pending = f.run(request)
  await tick(); await f.confirm()
  const retrying = f.retry()
  await tick(); await f.cancel()
  delayed.resolve(plan('late-retry')); await retrying
  assert.equal(f.state, null); assert.equal(await pending, null)
  assert.deepEqual(f.calls.filter(call => call[0] === 'cancel').at(-1), ['cancel', 'late-retry'])
})
