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
const plan = (plan_id, properties = {}) => ({ plan_id, status: 'ready', prepared: false, actions: [action],
  evaluations: [], blockers: [], impacts: ['mod:owner'], roots: [{ node_id: 'mod:owner', label: 'Host Utilities' }],
  nodes: [{ node_id: 'mod:owner', dll_files: ['BepInEx/plugins/v2.0.2/Host.dll'] }], install_notes: [], ...properties })
const tick = () => new Promise(resolve => setImmediate(resolve))

function fixture({ nextPlan, executeResult, prepareError, nextPrepareError } = {}) {
  const calls = []
  let state, attempt = 0
  const workflow = createChangeWorkflow({
    plan: async request => {
      calls.push(['plan', request])
      attempt++
      if (attempt === 2 && nextPlan) return nextPlan()
      return plan(`plan-${attempt}`)
    },
    prepare: async id => {
      calls.push(['prepare', id])
      if (attempt === 1) throw prepareError ?? new types.ChangePreparationError('modChanges.installTargetClaimed', offer)
      if (attempt === 2 && nextPrepareError) throw nextPrepareError
      return plan(id, { prepared: true })
    },
    execute: async id => { calls.push(['execute', id]); return executeResult ?? { ok: true, message: 'done' } },
    cancel: async id => { calls.push(['cancel', id]) },
  }, value => { state = value })
  return { ...workflow, calls, get state() { return state } }
}

async function beginUpgrade(flow) {
  await tick()
  assert.equal(flow.state.phase, 'confirming')
  await flow.confirm()
  assert.equal(flow.state.phase, 'failed')
}

test('a backend offer permits only the exact overwrite retry, followed by final confirmation even for a routine upgrade', async () => {
  const flow = fixture(), result = flow.run(release)
  await beginUpgrade(flow)
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
    await beginUpgrade(flow)
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

test('a backend-rejected forced retry returns to a plain failure with no overwrite offer', async () => {
  const flow = fixture({ nextPlan() { throw new Error('modChanges.targetChanged') } }), result = flow.run(release)
  await beginUpgrade(flow)
  await flow.forceOverwriteManifest()
  assert.equal(flow.state.phase, 'failed')
  assert.equal(flow.state.error.message, 'modChanges.targetChanged')
  assert.equal(flow.state.manifestOverwrite, undefined)
  assert.deepEqual(flow.calls.at(-1), ['plan', { ...release, manifestOverwrite: intent }])
  await flow.retry()
  assert.deepEqual(flow.calls.at(-1), ['plan', release])
  await flow.cancel()
  await result
})

test('stale overwrite consent never automatically replans or carries into a generic retry', async () => {
  const flow = fixture({ executeResult: { ok: false, message: 'modChanges.inventoryChanged' } }), result = flow.run(release)
  await beginUpgrade(flow)
  await flow.forceOverwriteManifest()
  await flow.confirm()
  assert.equal(flow.state.phase, 'failed')
  assert.equal(flow.state.manifestOverwrite, undefined)
  assert.deepEqual(flow.calls.at(-1), ['execute', 'plan-2'])
  await flow.retry()
  assert.equal(flow.state.phase, 'confirming')
  assert.deepEqual(flow.calls.at(-1), ['plan', release])
  await flow.cancel()
  await result
})

test('refreshed overwrite proof requires a new click and supplies the new fingerprint', async () => {
  const newOffer = { ...offer, fingerprint: 'proof-2', recordedPaths: ['BepInEx/plugins/Another.dll'] }
  const flow = fixture({ nextPrepareError: new types.ChangePreparationError('modChanges.installTargetClaimed', newOffer) })
  const result = flow.run(release)
  await beginUpgrade(flow)
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
