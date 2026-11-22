import assert from 'node:assert/strict'
import { registerHooks } from 'node:module'
import test from 'node:test'

const channels = []
const boundary = globalThis.__chainProgressChannel = { channels, invoke: () => { throw Error('unexpected IPC') } }
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('@tauri-apps/')) return { url: `chain-channel:${specifier}`, shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url.endsWith('/src/i18n.ts')) return fake('export const locale={value:"zh-CN"}')
    if (url === 'chain-channel:@tauri-apps/api/core') return fake(`
      export class Channel {constructor(){globalThis.__chainProgressChannel.channels.push(this)}}
      export const invoke=(...args)=>globalThis.__chainProgressChannel.invoke(...args)`)
    if (url.startsWith('chain-channel:')) return fake('export const listen=()=>Promise.resolve(()=>{});export const open=()=>{}')
    return next(url, context)
  },
})
const { api } = await import('../src/services/tauri.ts')
const defer = () => { let resolve; const promise = new Promise(yes => { resolve = yes }); return { promise, resolve } }
const progress = (stage, count = 1) => ({ plan_id: 'one', stage, actions: [],
  items: Array.from({ length: count }, (_, i) => ({ node_id: `node-${i}`, label: `Package ${i}`, candidate_id: 'v1', stage: stage === 'ready' ? 'ready' : 'downloading' })) })

test('preparation IPC publishes authoritative large terminal snapshot before returning and drops late Channel data', async () => {
  const outcome = defer(), seen = []
  boundary.invoke = (name, args) => { assert.equal(name, 'prepare_mod_changes'); assert.equal(args.planId, 'one'); return outcome.promise }
  const pending = api.prepareModChanges('one', value => seen.push(value))
  const channel = channels.at(-1)
  channel.onmessage(progress('downloading'))
  const terminal = progress('ready', 200)
  assert.ok(JSON.stringify(terminal).length > 8192, 'covers channel payloads delivered using asynchronous data fetch')
  const plan = { plan_id: 'revised', prepared: true }
  outcome.resolve({ plan, error: null, progress: terminal })
  assert.strictEqual(await pending, plan)
  assert.strictEqual(seen.at(-1), terminal)
  channel.onmessage(progress('downloading'))
  assert.strictEqual(seen.at(-1), terminal)
})

test('failed invocation delivers the final item context before rejecting and ignores delayed old events', async () => {
  const terminal = progress('failed'), seen = []
  boundary.invoke = async () => ({ plan: null, error: 'download.cancelled', progress: terminal })
  await assert.rejects(api.prepareModChanges('one', value => seen.push(value)), /download.cancelled/)
  assert.strictEqual(seen.at(-1), terminal)
  channels.at(-1).onmessage(progress('downloading'))
  assert.equal(seen.length, 1)
})

test('a resolving incomplete outcome stays unresolved and is never manufactured as ready', async () => {
  const terminal = progress('resolving'), plan = { plan_id: 'blocked', status: 'incomplete', prepared: false }, seen = []
  boundary.invoke = async () => ({ plan, error: null, progress: terminal })
  assert.strictEqual(await api.prepareModChanges('one', value => seen.push(value)), plan)
  assert.equal(seen.at(-1).stage, 'resolving')
})

test('application shows the rollback and exact failed row even when the terminal channel is delayed', async () => {
  const outcome = defer(), seen = [], failures = []
  boundary.invoke = (name, args) => {
    if (name === 'record_ipc_failure') {
      failures.push(args)
      return Promise.resolve()
    }
    assert.equal(name, 'execute_mod_changes')
    assert.equal(args.planId, 'one')
    return outcome.promise
  }
  const pending = api.executeModChanges('one', value => seen.push(value))
  const channel = channels.at(-1)
  const provisional = { plan_id: 'one', items: [{ node_id: 'first', stage: 'processed' }] }
  channel.onmessage(provisional)
  const terminal = { plan_id: 'one', items: [
    { node_id: 'first', stage: 'rolled_back' }, { node_id: 'second', stage: 'failed' },
  ] }
  const error = { ok: false, message: 'recycle.payloadModified' }
  outcome.resolve({ ...error, progress: terminal })
  assert.deepEqual(await pending, error)
  channel.onmessage(provisional)
  assert.deepEqual(seen, [provisional, terminal])
  assert.deepEqual(failures.map(({ elapsedMs, ...failure }) => failure), [
    { command: 'execute_mod_changes', rejected: false, tag: 'recycle.payloadModified' },
  ])
})
