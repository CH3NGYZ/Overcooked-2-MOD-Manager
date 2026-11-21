import assert from 'node:assert/strict'
import test from 'node:test'
import { createChangeWorkflow } from '../src/modChanges/workflow.ts'

const plan = (id = 'plan-1', extra = {}) => ({ plan_id: id, status: 'ready', actions: [], evaluations: [],
  blockers: [], impacts: [], roots: [{ node_id: 'mod:owner', label: 'Owner', tag: 'v1' }], nodes: [], install_notes: [], prepared: false, ...extra })
const request = { roots: [{ node_id: 'mod:owner', candidate_id: 'release:owner:v1', enabled: true }] }
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const tick = () => new Promise(resolve => setImmediate(resolve))
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
  const f = fixture(), pending = f.run(request)
  await tick()
  assert.equal(f.state.phase, 'confirming')
  assert.deepEqual(f.calls, [['plan', request]])
  await f.confirm()
  assert.deepEqual(await pending, { ok: true, message: 'modChanges.completed' })
  assert.deepEqual(f.calls.map(call => call[0]), ['plan', 'prepare', 'execute'])
  assert.equal(f.calls[2][1], 'plan-1')
  assert.equal(f.state, null)
})

for (const kind of ['install', 'upgrade', 'downgrade', 'replace']) test(`unchanged ${kind} must show completion and get separate approval to write`, async () => {
  const presentation = deferred(), actions = [{ node_id: 'mod:owner', kind }]
  const f = fixture({ plan: async () => plan('same', { actions }),
    prepare: async () => plan('same', { actions, prepared: true }), presentPrepared: () => presentation.promise })
  const pending = f.run(request)
  await tick()
  const staging = f.confirm()
  await tick()
  assert.equal(f.state.phase, 'prepared')
  await f.confirm()
  await f.cancel()
  assert.equal(f.state.phase, 'prepared')
  assert.deepEqual(f.calls, [])
  presentation.resolve()
  await staging
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
    await tick(); await f.confirm()
    assert.equal(f.state.phase, 'reconfirming')
    assert(f.states.some(state => state?.phase === 'prepared'))
    assert.deepEqual(f.calls, [])
    await f.cancel(); assert.equal(await pending, null)
    assert.deepEqual(f.calls, [['cancel', 'unchanged']])
  }
})

test('changed downloaded content requires a second confirmation of the complete new plan', async () => {
  const f = fixture({ prepare: async () => plan('verified-2', { prepared: true,
    actions: [{ node_id: 'mod:dependency', kind: 'downgrade', to_tag: 'v2' }] }) })
  const pending = f.run(request)
  await tick(); await f.confirm()
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

test('an extra dependency action, a different package member, or a blocker still opens review', async () => {
  const { request, choice } = toggle('mod', true, 'owner-dll')
  const [action] = choice.actions
  for (const plan of [
    { ...choice, actions: [action, { ...action, node_id: 'mod:dependent' }], impacts: ['mod:owner', 'mod:dependent'] },
    { ...choice, actions: [action, { ...action, kind: 'disable', node_id: 'mod:conflict' }], impacts: ['mod:owner', 'mod:conflict'] },
    { ...choice, actions: [{ ...action, member_key: 'different-dll' }] },
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
  const pendingNew = f.run(request)
  await tick(); old.reject(new Error('stale')); await pendingOld
  assert.equal(f.state.plan.plan_id, 'new')
  assert.equal(f.state.phase, 'confirming')
  await f.cancel(); await pendingNew
})

test('retry freezes the initial request, cleans failed preparation and requires a new confirmation', async () => {
  const submitted = { roots: [{ node_id: 'mod:owner', candidate_id: 'v1', enabled: true }] }
  let attempts = 0
  const f = fixture({ prepare: async id => {
    if (!attempts++) throw new Error('download.failed')
    return plan(id, { prepared: true })
  } })
  const pending = f.run(submitted)
  submitted.roots[0].candidate_id = 'different'
  await tick(); await f.confirm()
  assert.equal(f.state.phase, 'failed')
  await f.retry()
  assert.equal(f.state.phase, 'confirming')
  assert.deepEqual(f.calls.filter(call => call[0] === 'plan').at(-1)[1], { roots: [{ node_id: 'mod:owner', candidate_id: 'v1', enabled: true }] })
  assert.equal(f.calls.some(call => call[0] === 'execute'), false)
  assert.equal(f.calls.filter(call => call[0] === 'cancel').length, 1)
  await f.confirm(); assert.equal((await pending).ok, true)
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
