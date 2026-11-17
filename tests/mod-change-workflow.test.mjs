import assert from 'node:assert/strict'
import test from 'node:test'
import { createChangeWorkflow } from '../src/modChanges/workflow.ts'

const plan = (id = 'plan-1', extra = {}) => ({ plan_id: id, status: 'ready', actions: [], evaluations: [],
  blockers: [], impacts: [], roots: [{ node_id: 'mod:owner', label: 'Owner', tag: 'v1' }], nodes: [], prepared: false, ...extra })
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

test('one confirmation prepares the whole plan and executes exactly one planId', async () => {
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
