import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return nextResolve(`${specifier}.ts`, context) } catch {}
      try { return nextResolve(`${specifier}/index.ts`, context) } catch {}
    }
    return nextResolve(specifier, context)
  },
})

const notes = await import('../src/modInstallNotes.ts')
const { locale } = await import('../src/i18n.ts')
const { useInstallPlanGate } = await import('../src/composables/useInstallPlanGate.ts')
const repo = (id, overrides = {}) => ({
  id, name: id, provider: 'github', username: 'owner', repository: id,
  note: `# ${id}`, plugin_guid: `guid.${id}`, cached_releases: [], ...overrides,
})

test('首次安装说明按当前语言选取，去重并跳过空说明、有清单的仓库或 GUID 已安装项', () => {
  locale.value = 'ja'
  const a = repo('a', { note_i18n: { ja: '**日本語**', en: 'English' } })
  const result = notes.firstInstallNotes([
    a, a, repo('empty', { note: '  ' }), repo('bound'), repo('guid'),
  ], [{ repository_id: 'bound', package_id: 'bound' },
    { guid: ' GUID.GUID ', enabled: false, package_id: 'manual-local' }])
  assert.deepEqual(result, [{ id: 'a', name: 'a', text: '**日本語**' }])
  locale.value = 'zh-CN'
})

test('手动放入 plugins 后绑定仓库或 GUID 仍需说明；安装清单才可跳过', () => {
  const target = repo('manual')
  for (const binding of [
    { repository_id: target.id },
    { repository_ids: [target.id] },
    { repository_binding: { provider: 'github', owner: 'OWNER', repository: target.repository } },
    { guid: ' GUID.MANUAL ', enabled: false },
  ]) {
    for (const package_id of [undefined, '', '  ']) {
      assert.equal(notes.firstInstallNotes([target], [{ ...binding, package_id }]).length, 1)
    }
    for (const package_id of ['online-install', 'manual-local-file']) {
      assert.deepEqual(notes.firstInstallNotes([target], [{ ...binding, package_id }]), [])
    }
  }
  assert.equal(notes.firstInstallNotes([target], [{ package_id: 'other', guid: 'other' }]).length, 1)
})

test('空说明直接放行；5秒前不可继续；每项重新计时，全部通过才放行', async (t) => {
  t.mock.timers.enable({ apis: ['Date', 'setInterval'] })
  t.after(() => notes.finishModInstallNotes())
  assert.equal(await notes.confirmModInstallNotes([]), true)
  let result
  const pending = notes.confirmModInstallNotes(notes.firstInstallNotes([repo('a'), repo('b')], []))
    .then((value) => { result = value })
  notes.continueModInstallNote()
  t.mock.timers.tick(4999)
  notes.continueModInstallNote()
  assert.equal(notes.modInstallNote.value.id, 'a')
  assert.equal(result, undefined)
  t.mock.timers.tick(1)
  notes.continueModInstallNote()
  assert.equal(notes.modInstallNote.value.id, 'b')
  assert.equal(notes.noteSecondsRemaining.value, 5)
  notes.continueModInstallNote()
  assert.equal(notes.modInstallNote.value.id, 'b')
  t.mock.timers.tick(5000)
  notes.continueModInstallNote()
  await pending
  assert.equal(result, true)
  assert.equal(notes.modInstallNote.value, null)
})

test('取消不受倒计时限制，并清理后续项；并行入口不能替换当前说明', async () => {
  const pending = notes.confirmModInstallNotes(notes.firstInstallNotes([repo('a'), repo('b')], []))
  assert.equal(await notes.confirmModInstallNotes(notes.firstInstallNotes([repo('c')], [])), false)
  assert.equal(notes.modInstallNote.value.id, 'a')
  notes.finishModInstallNotes()
  assert.equal(await pending, false)
  assert.equal(notes.modInstallNote.value, null)
  assert.equal(notes.noteSecondsRemaining.value, 0)
})

test('真实安装计划入口：取消说明不进入关系处理；有清单更新与地图不弹，手动绑定更新需说明', async () => {
  const a = repo('a')
  const gate = useInstallPlanGate({ mods: () => [], repositories: () => [a],
    gameBlocked: () => false, onApplied: () => {} })
  const pending = gate.run(a)
  assert.equal(notes.modInstallNote.value.id, 'a')
  notes.finishModInstallNotes()
  assert.equal(await pending, false)
  assert.equal(gate.open.value, false)
  const updating = useInstallPlanGate({ mods: () => [{ repository_id: a.id, package_id: a.id }],
    repositories: () => [a], gameBlocked: () => false, onApplied: () => {} })
  assert.equal(await updating.run(a), true)
  const unmanaged = useInstallPlanGate({ mods: () => [{ repository_id: a.id }],
    repositories: () => [a], gameBlocked: () => false, onApplied: () => {} })
  const manualUpdate = unmanaged.run(a)
  assert.equal(notes.modInstallNote.value.id, 'a')
  notes.finishModInstallNotes()
  assert.equal(await manualUpdate, false)
  assert.equal(unmanaged.open.value, false)
  assert.equal(await gate.run(repo('map')), true)
  assert.equal(notes.modInstallNote.value, null)
})
