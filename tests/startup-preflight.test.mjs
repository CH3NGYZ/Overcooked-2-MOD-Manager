import assert from 'node:assert/strict'
import test from 'node:test'
import { createStartupPreflight } from '../src/startupPreflight.ts'

const deferred = () => {
  let resolve
  const promise = new Promise(done => { resolve = done })
  return { promise, resolve }
}

function setup(overrides = {}) {
  const events = []
  const gate = createStartupPreflight({
    desktop: true,
    region: async () => { events.push('region'); return 'global' },
    probe: async () => { events.push('probe'); return { reachable: true, failure_kind: null } },
    cancel: async () => { events.push('cancel') },
    beforeProbe: async () => { events.push('render') },
    ready: () => events.push('ready'),
    changed: (stage, failure) => events.push([stage, failure]),
    ...overrides,
  })
  return { gate, events }
}

test('international startup renders before probing and releases startup only once', async () => {
  const { gate, events } = setup()
  await Promise.all([gate.start(), gate.start()])
  await gate.start()
  assert.deepEqual(events, [
    ['region', ''], 'region', ['checking', ''], 'render', 'probe', ['completed', ''], 'ready',
  ])
})

test('mainland recheck skips probing; browser preview skips IPC', async () => {
  for (const overrides of [
    { region: async () => 'cn' },
    { region: async () => ' CN ' },
    { region: async () => 'chinese_mainland' },
    { region: async () => 'mainland-china' },
    { desktop: false, region: async () => { throw Error('must not invoke') } },
  ]) {
    const { gate, events } = setup(overrides)
    await gate.start()
    assert.ok(!events.includes('probe'))
    assert.equal(events.filter(value => value === 'ready').length, 1)
  }
})

test('failed probe waits for retry, which rechecks region and prevents concurrent attempts', async () => {
  let attempts = 0
  const { gate, events } = setup({
    probe: async () => ++attempts === 1
      ? { reachable: false, failure_kind: 'invalid_content' }
      : { reachable: true, failure_kind: null },
  })
  await gate.start()
  assert.ok(events.some(value => value[0] === 'failed' && value[1] === 'invalid_content'))
  assert.ok(!events.includes('ready'))
  await Promise.all([gate.retry(), gate.retry()])
  assert.equal(attempts, 2)
  assert.equal(events.filter(value => value === 'region').length, 2)
  assert.equal(events.filter(value => value === 'ready').length, 1)
})

test('closing waits for active request settlement and ignores its late success', async () => {
  const pending = deferred()
  const { gate, events } = setup({ probe: () => pending.promise })
  const started = gate.start()
  await new Promise(resolve => setImmediate(resolve))
  const closed = gate.close()
  await Promise.resolve()
  assert.ok(!events.includes('ready'))
  pending.resolve({ reachable: true, failure_kind: null })
  await Promise.all([started, closed, gate.close()])
  assert.equal(events.filter(value => value === 'ready').length, 1)
  assert.equal(events.filter(value => value === 'cancel').length, 1)
})

test('region failure stays recoverable and close before paint never starts a probe', async () => {
  const failed = setup({ region: async () => { throw Error('IPC unavailable') } })
  await failed.gate.start()
  assert.ok(!failed.events.includes('ready'))
  await failed.gate.close()
  assert.equal(failed.events.filter(value => value === 'ready').length, 1)
  const paint = deferred()
  const closing = setup({ beforeProbe: () => paint.promise })
  const started = closing.gate.start()
  await new Promise(resolve => setImmediate(resolve))
  const closed = closing.gate.close()
  paint.resolve()
  await Promise.all([started, closed])
  assert.ok(!closing.events.includes('probe'))
})
