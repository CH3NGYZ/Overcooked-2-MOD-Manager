import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { LOCALE_CODES } from '../src/locale/types.ts'
import { setup, snapshot, tick, deferred, valid, shallowRef, useDashboard } from './fixtures/diychef-editor.mjs'

const liveTarget = { version: '2.4.0', compatibility: 'editable', supports_live_save: true }
const gameEvent = running => globalThis.__chefGameEvent({ path: 'fixture', kitchen_running: running })

test('live-save confirmation only reports saved in all supported languages', async () => {
  const { diychefEditor } = await import('../src/locale/diychef/editor.ts')
  const expected = {
    'zh-CN': '已保存', 'zh-TW': '已儲存', en: 'Saved', ko: '저장했습니다', ja: '保存しました',
    de: 'Gespeichert', fr: 'Enregistré', it: 'Salvato', 'es-ES': 'Guardado', ru: 'Сохранено', pl: 'Zapisano', 'pt-BR': 'Salvo',
  }
  assert.deepEqual(Object.keys(diychefEditor.savedLive).sort(), [...LOCALE_CODES].sort())
  for (const locale of LOCALE_CODES) assert.equal(diychefEditor.savedLive[locale], expected[locale], locale)
})

for (const version of ['2.4', '2.4.0', '2.4.7', '2.4.7.9']) {
  test(`backend live-save capability permits saving ${version} while the game runs`, async t => {
    const { editor } = await setup(t, { running: true, target: { ...liveTarget, version } })
    await editor.open()
    editor.setLobby(true)
    let writes = 0
    const save = globalThis.__chefApi.diyChefSave
    globalThis.__chefApi.diyChefSave = (...args) => { writes++; return save(...args) }
    assert.equal(editor.gameRunning.value, true)
    assert.equal(editor.saveBlockedByGame.value, false)
    assert.equal(editor.canSave.value, true)
    assert.equal(await editor.save(), true)
    assert.equal(writes, 1)
    assert.equal(editor.dirty.value, false)
    assert.equal(editor.snapshot.value.selection.lobby_switch, true)
    assert.equal(editor.snapshot.value.target.supports_live_save, true)
    assert.equal(globalThis.__chefNotices.at(-1), 'diychef.savedLive')
  })
}

for (const target of [
  { version: '2.3.9', supports_live_save: false },
  { version: '2.4.0' },
  { version: '2.5.0', supports_live_save: false },
]) {
  test(`stopped-game save retains the restart notice without live-save capability ${JSON.stringify(target)}`, async t => {
    const { editor } = await setup(t, { target })
    await editor.open()
    editor.setLobby(true)
    assert.equal(await editor.save(), true)
    assert.equal(globalThis.__chefNotices.at(-1), 'diychef.saved')
  })
}

for (const supports_live_save of [true, false]) {
  test(`save notice uses the returned target capability (${supports_live_save})`, async t => {
    const { editor } = await setup(t, { target: { ...liveTarget, supports_live_save: !supports_live_save } })
    const save = globalThis.__chefApi.diyChefSave
    globalThis.__chefApi.diyChefSave = async (...args) => {
      const result = await save(...args)
      return { ...result, target: { ...result.target, supports_live_save } }
    }
    await editor.open()
    editor.setLobby(true)
    assert.equal(await editor.save(), true)
    assert.equal(globalThis.__chefNotices.at(-1), supports_live_save ? 'diychef.savedLive' : 'diychef.saved')
  })
}

for (const target of [
  { version: null, supports_live_save: false },
  { version: '2.3.9', supports_live_save: false },
  { version: '2.5.0', supports_live_save: false },
  { version: '3.0.0', supports_live_save: false },
  { version: '2.4.0', supports_live_save: false },
  { version: '2.4.0' },
]) {
  test(`running save stays blocked for backend capability ${JSON.stringify(target)}`, async t => {
    let writes = 0, validations = 0
    const { editor } = await setup(t, { running: true, target, api: {
      diyChefValidate: async () => { validations++; return valid() },
      diyChefSave: async () => { writes++; return snapshot() },
    } })
    await editor.open()
    editor.setLobby(true)
    assert.equal(editor.editable.value, true, 'in-memory editing remains available')
    assert.equal(editor.saveBlockedByGame.value, true)
    assert.equal(editor.canSave.value, false)
    assert.equal(await editor.save(), false)
    assert.equal(writes, 0)
    assert.equal(validations, 0)
    assert.equal(editor.dirty.value, true)
  })
}

test('a missing config initializes once during gameplay only with the backend live-save capability', async t => {
  const { editor } = await setup(t, { running: true, missing: true, target: liveTarget })
  let initialized = 0
  const initialize = globalThis.__chefApi.diyChefInitialize
  globalThis.__chefApi.diyChefInitialize = (...args) => { initialized++; return initialize(...args) }
  await editor.open()
  assert.equal(initialized, 1)
  assert.equal(editor.snapshot.value.source.state, 'present')
  assert.equal(editor.snapshot.value.source.revision, 'initialized')
  assert.equal(editor.snapshot.value.target.supports_live_save, true)
  assert.equal(editor.gameRunning.value, true)
  assert.equal(editor.saveBlockedByGame.value, false)
  gameEvent(false); await tick()
  assert.equal(initialized, 1, 'stopping the game does not initialize a present config again')
})

for (const supports_live_save of [false, undefined]) {
  test(`missing config waits until game stops when live-save capability is ${supports_live_save}`, async t => {
    let initialized = 0
    const { editor } = await setup(t, { running: true, missing: true, target: { version: '2.4.0', supports_live_save } })
    const initialize = globalThis.__chefApi.diyChefInitialize
    globalThis.__chefApi.diyChefInitialize = (...args) => { initialized++; return initialize(...args) }
    await editor.open()
    assert.equal(initialized, 0)
    assert.equal(editor.snapshot.value.source.state, 'missing')
    gameEvent(false); await tick()
    assert.equal(initialized, 1)
    assert.equal(editor.snapshot.value.source.state, 'present')
  })
}

for (const phase of ['validation', 'confirmation']) {
  for (const supports_live_save of [true, false]) {
    test(`game starting during ${phase} only permits the authorized target (${supports_live_save})`, async t => {
      const pending = deferred()
      let writes = 0
      const { editor } = await setup(t, { target: { ...liveTarget, supports_live_save }, api: {
        diyChefValidate: () => phase === 'validation' ? pending.promise : Promise.resolve({ ...valid(), changes: [{ id: 'normalize', before: 'a', after: 'b' }] }),
      }, confirmChanges: () => pending.promise })
      const save = globalThis.__chefApi.diyChefSave
      globalThis.__chefApi.diyChefSave = (...args) => { writes++; return save(...args) }
      await editor.open(); editor.setLobby(true)
      const saving = editor.save()
      await tick()
      gameEvent(true); await tick()
      pending.resolve(phase === 'validation' ? valid() : true)
      assert.equal(await saving, supports_live_save)
      assert.equal(writes, Number(supports_live_save))
      assert.equal(editor.saving.value, false)
      assert.equal(editor.dirty.value, !supports_live_save)
    })
  }
}

test('live-save does not bypass read-only, stale scan, conflict or resource migration gates', async t => {
  const { editor } = await setup(t, { running: true, target: liveTarget, api: {
    diyChefOpen: async () => ({ status: 'ready', candidates: [], snapshot: { ...snapshot(), target: { ...snapshot().target, ...liveTarget } },
      legacy_targets: [{ ...snapshot().target, id: 'legacy-live-save', version: '1.9' }], migration_available: true }),
  } })
  await editor.open(); editor.setLobby(true)
  assert.equal(editor.canSave.value, true)
  assert.equal(editor.migrationVisible.value, true)
  assert.equal(editor.canMigrate.value, false)
  let migrations = 0
  globalThis.__chefApi.diyChefMigrate = async () => { migrations++ }
  await editor.migrate()
  assert.equal(migrations, 0)
  const current = editor.snapshot.value
  editor.snapshot.value = { ...current, target: { ...current.target, compatibility: 'read_only' } }
  assert.equal(editor.canSave.value, false)
  editor.snapshot.value = current
  useDashboard().modsGeneration.value++
  assert.equal(editor.canSave.value, false)
  useDashboard().modsGeneration.value--
  assert.equal(editor.canSave.value, true)
  editor.reportOperationFailure('diychef.conflict')
  assert.equal(editor.canSave.value, false)
  assert.equal(editor.dirty.value, true)
})

test('live-save capability never authorizes tutorial writes or missing read-only initialization', async t => {
  const demo = shallowRef(null)
  let writes = 0, initializations = 0
  const { editor } = await setup(t, { running: true, missing: true, target: { ...liveTarget, compatibility: 'read_only' }, demoSnapshot: () => demo.value, api: {
    diyChefInitialize: async () => { initializations++; return snapshot() },
    diyChefSave: async () => { writes++; return snapshot() },
  } })
  await editor.open()
  assert.equal(initializations, 0)
  assert.equal(editor.readOnly.value, true)
  demo.value = { ...snapshot(true), session_id: 'demo-live-save', target: { ...snapshot().target, ...liveTarget } }
  editor.setLobby(true)
  assert.equal(editor.canSave.value, false)
  assert.equal(await editor.save(), false)
  assert.equal(initializations, 0)
  assert.equal(writes, 0)
})

test('deferred missing initialization retains the running target capability after the resource read', async t => {
  const opening = deferred(), reading = deferred()
  let opens = 0, initializations = 0
  const current = { ...snapshot(true), target: { ...snapshot().target, ...liveTarget } }
  const { editor } = await setup(t, { missing: true, running: true, target: liveTarget, api: {
    diyChefOpen: () => ++opens === 1 ? opening.promise : reading.promise,
    diyChefInitialize: async () => { initializations++; return { ...current, source: { ...snapshot().source, revision: 'initialized' } } },
  } })
  const open = editor.open(); await tick()
  globalThis.__chefFilesEvent({ target_id: 'target', generation: 7, resources_changed: true })
  opening.resolve({ status: 'ready', candidates: [], snapshot: current })
  await open; await tick()
  assert.equal(opens, 2)
  assert.equal(initializations, 0)
  reading.resolve({ status: 'ready', candidates: [], snapshot: { ...current, session_id: 'fresh-missing' } })
  await tick()
  assert.equal(initializations, 1)
  assert.equal(editor.snapshot.value.source.state, 'present')
  assert.equal(editor.gameRunning.value, true)
})

test('notice consumes the same save gate while delete retains the raw game-running guard', () => {
  const read = path => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8')
  const notice = read('components/diychef/EditorNotice.vue')
  const view = read('views/DiyChefView.vue')
  assert.match(notice, /v-if="saveBlockedByGame"[^>]*>\{\{ t\('diychef\.gameRunning'\) \}\}/)
  assert.match(view, /<EditorNotice[^>]*:save-blocked-by-game="saveBlockedByGame"/)
  assert.match(view, /const canDeleteChef =[^\n]*!gameRunning\.value/)
  assert.match(view, /if \(!chef[^\n]*gameRunning\.value[^\n]*\) return/)
  assert.match(view, /onPositiveClick: async \(\) => \{\s*if \(gameRunning\.value\) return false/)
  assert.doesNotMatch(read('composables/useDiyChefEditor.ts'), /compareVersions|compareModVersions|parseVersion|version\s*[<>]|version\.startsWith/)
})
