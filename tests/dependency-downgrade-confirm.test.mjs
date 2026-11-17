import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'

// Execute the real shared table's setup. Only visual dependencies and i18n are
// replaced; no router, network or filesystem service can be invoked here.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'dependency-confirm:naive', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* regular resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fake = source => ({ format: 'module', shortCircuit: true, source })
    if (url === 'dependency-confirm:naive') return fake('export const NButton={}, NTag={} ')
    if (url.endsWith('/src/components/DependencyTable.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'dependency-confirm-test' }).content }
    }
    if (url.endsWith('.vue')) return fake('export default {}')
    if (url.endsWith('/src/i18n.ts')) return fake('export const t=key=>key')
    if (url.endsWith('/src/theme.ts')) return fake('export const tagColorForAny=()=>({})')
    return next(url, context)
  },
})
const { default: DependencyTable } = await import('../src/components/DependencyTable.vue')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const downgradeRow = () => ({
  key: 'runtime:requires:any', name: 'Web Runtime', requirement: '>=3.10.2,<4',
  status: 'tools.downgradable', jump: { id: 'runtime-source', title: 'Open MOD',
    downgrade: { installedVersion: '4.0', targetVersion: 'v3.10.2' } },
})
function mounted(t, initialRows = [downgradeRow()]) {
  const rows = shallowRef(initialRows)
  const jumps = []
  const app = renderer.createApp({ render: () => h({ ...DependencyTable, render: () => null }, {
    rows: rows.value, onJump: row => jumps.push(row),
  }) })
  app.mount({})
  t.after(() => app.unmount())
  return { rows, jumps, state: app._instance.subTree.component.setupState }
}

test('downgrade click opens a prompt before navigation; confirmation emits the current row once', t => {
  const { state, rows, jumps } = mounted(t)
  state.clickStatus(rows.value[0])
  assert.equal(state.downgradeOpen, true)
  assert.deepEqual(jumps, [])
  assert.deepEqual(state.pendingDowngrade.jump.downgrade,
    { installedVersion: '4.0', targetVersion: 'v3.10.2' })
  state.confirmDowngrade()
  state.confirmDowngrade()
  assert.equal(state.downgradeOpen, false)
  assert.deepEqual(jumps, [rows.value[0]])
})

test('cancel preserves the prompt during its exit animation and never navigates', t => {
  const { state, rows, jumps } = mounted(t)
  state.clickStatus(rows.value[0])
  state.downgradeOpen = false
  state.confirmDowngrade()
  assert.equal(state.pendingDowngrade.name, 'Web Runtime')
  assert.deepEqual(jumps, [])
  state.afterDowngradeLeave()
  assert.equal(state.pendingDowngrade, undefined)
})

test('refresh invalidates a confirmed downgrade when its row, source or either version changes', async t => {
  for (const change of [
    row => [],
    row => [{ ...row, key: 'different:requires:any' }],
    row => [{ ...row, jump: undefined }],
    row => [{ ...row, jump: { ...row.jump, id: 'different-source' } }],
    row => [{ ...row, jump: { ...row.jump, downgrade: { ...row.jump.downgrade, installedVersion: '4.1' } } }],
    row => [{ ...row, jump: { ...row.jump, downgrade: { ...row.jump.downgrade, targetVersion: 'v3.10.1' } } }],
  ]) {
    const { state, rows, jumps } = mounted(t)
    state.clickStatus(rows.value[0])
    rows.value = change(rows.value[0])
    await nextTick()
    state.confirmDowngrade()
    assert.equal(state.downgradeOpen, false)
    assert.deepEqual(jumps, [])
  }
})

test('reopening during the previous exit keeps the newly selected dependency', t => {
  const second = { ...downgradeRow(), key: 'diy:requires:any', name: 'DIYLevel',
    jump: { id: 'diy-source', downgrade: { installedVersion: '0.11', targetVersion: 'v0.10.6' } } }
  const { state, rows, jumps } = mounted(t, [downgradeRow(), second])
  state.clickStatus(rows.value[0])
  state.downgradeOpen = false
  state.clickStatus(rows.value[1])
  state.afterDowngradeLeave()
  assert.equal(state.pendingDowngrade.name, 'DIYLevel')
  state.confirmDowngrade()
  assert.deepEqual(jumps, [second])
})

test('upgrade and installation still navigate directly; a non-navigable status does nothing', t => {
  const row = downgradeRow()
  delete row.jump.downgrade
  row.status = 'tools.updatable'
  const { state, jumps } = mounted(t, [row])
  state.clickStatus(row)
  state.clickStatus({ ...row, jump: undefined })
  assert.deepEqual(jumps, [row])
  assert.equal(state.downgradeOpen, false)
})

test('actionable dependency activation does not bubble into background card collapse', t => {
  const { state, rows } = mounted(t)
  for (const type of ['click', 'keydown']) {
    const event = new Event(type, { bubbles: true })
    state.clickStatus(rows.value[0], event)
    assert.equal(event.cancelBubble, true, `${type}: downgrade stays isolated`)
    state.downgradeOpen = false
    const upgrade = { ...rows.value[0], jump: { id: 'upgrade-source', title: 'Open MOD' } }
    const upgradeEvent = new Event(type, { bubbles: true })
    state.clickStatus(upgrade, upgradeEvent)
    assert.equal(upgradeEvent.cancelBubble, true, `${type}: direct jump stays isolated`)
    const plainEvent = new Event(type, { bubbles: true })
    state.clickStatus({ ...rows.value[0], jump: undefined }, plainEvent)
    assert.equal(plainEvent.cancelBubble, false, `${type}: read-only status remains detail background`)
  }
})
