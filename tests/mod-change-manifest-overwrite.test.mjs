import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createChangeWorkflow } from '../src/modChanges/workflow.ts'
import * as types from '../src/modChanges/types.ts'

const release = { roots: [{ node_id: 'mod:owner', candidate_id: 'release:owner:v2.0.3', enabled: true }] }
const path = 'manifests/owner.toml'
const offer = { repositoryId: 'owner', manifestPath: path, recordedPaths: ['BepInEx/plugins/v2.0.1/Host.dll'], fingerprint: 'proof-1' }
const intent = { repositoryId: offer.repositoryId, fingerprint: offer.fingerprint }
const action = { node_id: 'mod:owner', kind: 'upgrade', from_candidate_id: 'installed:owner', from_tag: 'v2.0.2', to_candidate_id: 'release:owner:v2.0.3' }
const plan = (plan_id, properties = {}) => ({ plan_id, status: 'ready', prepared: false, replan_scope: 'same-target', actions: [action],
  evaluations: [], blockers: [], impacts: ['mod:owner'], roots: [{ node_id: 'mod:owner', label: 'Host Utilities' }],
  nodes: [{ node_id: 'mod:owner', dll_files: ['BepInEx/plugins/v2.0.2/Host.dll'] }], install_notes: [], ...properties })
const tick = () => new Promise(resolve => setImmediate(resolve))

function fixture({ alterScope, executeResult, prepareError, nextPrepareError, installNotes } = {}) {
  const calls = []
  let state, attempt = 0
  const workflow = createChangeWorkflow({
    plan: async request => {
      calls.push(['plan', request])
      attempt++
      return plan(`plan-${attempt}`, alterScope && attempt > 1 ? { replan_scope: 'new-target' } : {})
    },
    prepare: async id => {
      calls.push(['prepare', id])
      if (attempt === 1) throw prepareError ?? new types.ChangePreparationError('modChanges.installTargetClaimed', offer)
      if (attempt === 2 && nextPrepareError) throw nextPrepareError
      return plan(id, { prepared: true, install_notes: installNotes ?? [] })
    },
    execute: async id => { calls.push(['execute', id]); return executeResult ?? { ok: true, message: 'done' } },
    cancel: async id => { calls.push(['cancel', id]) },
  }, value => { state = value })
  return { ...workflow, calls, get state() { return state } }
}

test('a backend offer permits only the exact overwrite retry, followed by final confirmation even for a routine upgrade', async () => {
  const flow = fixture(), result = flow.run(release)
  await tick()
  assert.equal(flow.state.phase, 'failed')
  assert.deepEqual(flow.state.manifestOverwrite, offer)
  await flow.forceOverwriteManifest()
  assert.equal(flow.state.phase, 'reconfirming')
  assert.deepEqual(flow.state.manifestOverwrite, offer)
  assert.deepEqual(flow.calls, [['plan', release], ['prepare', 'plan-1'], ['cancel', 'plan-1'],
    ['plan', { ...release, manifestOverwrite: intent }], ['prepare', 'plan-2']])
  await flow.confirm()
  assert.equal((await result).ok, true)
  assert.deepEqual(flow.calls.at(-1), ['execute', 'plan-2'])
})

test('cancelling before or after opting in never executes, and unstructured errors cannot opt in', async () => {
  for (const { prepareError, optIn } of [{}, { optIn: true }, { prepareError: new Error('modChanges.installTargetClaimed') }]) {
    const flow = fixture({ prepareError }), result = flow.run(release)
    await tick()
    if (prepareError) {
      assert.equal(flow.state.manifestOverwrite, undefined)
      await flow.forceOverwriteManifest()
      assert.equal(flow.calls.length, 2)
    } else if (optIn) await flow.forceOverwriteManifest()
    await flow.cancel()
    assert.equal(await result, null)
    assert.equal(flow.calls.some(([operation]) => operation === 'execute'), false)
  }
})

test('a changed target scope stops an overwrite retry before preparation', async () => {
  const flow = fixture({ alterScope: true }), result = flow.run(release)
  await tick()
  await flow.forceOverwriteManifest()
  assert.equal(flow.state.error, 'modChanges.targetChanged')
  assert.deepEqual(flow.calls.slice(-2), [['plan', { ...release, manifestOverwrite: intent }], ['cancel', 'plan-2']])
  await flow.cancel()
  await result
})

test('stale overwrite consent never automatically replans or carries into a generic retry', async () => {
  const flow = fixture({ executeResult: { ok: false, message: 'modChanges.inventoryChanged' } }), result = flow.run(release)
  await tick()
  await flow.forceOverwriteManifest()
  await flow.confirm()
  assert.equal(flow.state.phase, 'failed')
  assert.equal(flow.state.manifestOverwrite, undefined)
  assert.deepEqual(flow.calls.at(-1), ['execute', 'plan-2'])
  await flow.retry()
  assert.deepEqual(flow.calls.slice(-2), [['plan', release], ['prepare', 'plan-3']])
  await flow.cancel()
  await result
})

test('the presence of an install note cannot turn the overwrite review into read-and-apply', async () => {
  const flow = fixture({ installNotes: [{ id: 'latest-notes' }] }), result = flow.run(release)
  await tick()
  await flow.forceOverwriteManifest()
  assert.equal(flow.state.phase, 'reconfirming')
  assert.equal(flow.state.noteOnlyReview, false)
  assert.equal(flow.calls.some(([operation]) => operation === 'execute'), false)
  await flow.cancel()
  await result
})

test('refreshed overwrite proof requires a new click and supplies the new fingerprint', async () => {
  const newOffer = { ...offer, fingerprint: 'proof-2', recordedPaths: ['BepInEx/plugins/Another.dll'] }
  const flow = fixture({ nextPrepareError: new types.ChangePreparationError('modChanges.installTargetClaimed', newOffer) })
  const result = flow.run(release)
  await tick()
  await flow.forceOverwriteManifest()
  assert.equal(flow.state.phase, 'failed')
  assert.deepEqual(flow.state.manifestOverwrite, newOffer)
  assert.deepEqual(flow.calls.at(-1), ['prepare', 'plan-2'])
  await flow.forceOverwriteManifest()
  assert.deepEqual(flow.calls.slice(-2), [['plan', { ...release, manifestOverwrite: { ...intent, fingerprint: 'proof-2' } }], ['prepare', 'plan-3']])
  await flow.cancel()
  await result
})

test('the dialog displays backend-provided paths and gates the explicit overwrite button by the offer', () => {
  const read = name => readFileSync(new URL(`../src/${name}`, import.meta.url), 'utf8')
  const dialog = read('modChanges/ChangePlanDialog.vue')
  assert.match(dialog, /v-if="manifestOverwrite"[^>]+data-manifest-overwrite/)
  assert.match(dialog, /manifestOverwrite\.manifestPath/)
  assert.match(dialog, /v-for="path in manifestOverwrite\.recordedPaths"/)
  assert.match(dialog, /v-if="state\?\.phase === 'failed' && manifestOverwrite" data-plan-action="overwrite-manifest"[^>]+@click="changes\.forceOverwriteManifest"/)
  assert.match(read('services/tauri.ts'), /new ChangePreparationError\(outcome\.error, outcome\.manifestOverwrite\)/)
})
