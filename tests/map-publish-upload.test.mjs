import assert from 'node:assert/strict'
import test from 'node:test'
import vm from 'node:vm'
import { readFileSync } from 'node:fs'
import ts from 'typescript'
import * as vue from 'vue'
import * as assets from '../src/mapPublishAsset.ts'
import * as flow from '../src/mapPublishFlow.ts'
import * as releases from '../src/mapPublishRelease.ts'
import * as i18n from '../src/mapPublishI18n.ts'
import * as relations from '../src/mapPublishRelations.ts'

// Execute the production editor and draft with real Vue reactivity/lifecycle;
// only the session boundary, notices and page-leave host are supplied here.
const plain = value => JSON.parse(JSON.stringify(value))
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done }); return { promise, resolve } }
const tick = async () => { await vue.nextTick(); await new Promise(resolve => setImmediate(resolve)) }
function load(path, deps) {
  const exports = {}
  const code = ts.transpileModule(readFileSync(path, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText
  vm.runInNewContext(code, {
    exports, require: name => { assert.ok(name in deps, `${path}: ${name}`); return deps[name] },
    setTimeout, clearTimeout,
  }, { filename: path })
  return exports
}
const renderer = vue.createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const inspection = () => ({
  file_name: 'source-99.zip', file_size: 300, mapset_version: '99', info_version: '3',
  level_set_name: 'Map', level_set_name_zh: '地图', author: 'Author',
  set_count: 1, level_count: 4, file_count: 5, requires: { 'example.base': '>=1' },
})
const metadata = () => ({
  level_set_name: 'Map', level_set_name_zh: '地图', author: 'Author',
  name_i18n: { 'zh-CN': '旧地图', en: 'Saved map', fr: 'Carte' },
  group_i18n: { 'zh-CN': '作者', en: 'Author' }, note_i18n: {},
  asset: 'legacy_${version}_bundle.zip', latest_mapset_version: '1', latest_info_version: '0.9',
  asset_versions: [{ mapset_version: '2', info_version: '1.9' },
    { mapset_version: '1', info_version: '0.9' }],
  requires: { 'saved.base': '*' }, compatible: {}, conflicts: {}, release_relations: [],
})
const existingRelease = (hasMetadata = true) => ({
  id: 'release-existing', tag: 'Map', title: 'Map', body: '## 旧版\n保留历史说明',
  draft: false, prerelease: false, created_at: '', published_at: null,
  metadata: hasMetadata ? metadata() : null,
  assets: hasMetadata
    ? [{ id: 'a2', name: 'legacy_2_bundle.zip', size: 200 }, { id: 'a1', name: 'legacy_1_bundle.zip', size: 100 }]
    : [{ id: 'a1', name: 'Map_v1.zip', size: 100 }],
})

function fixture(t, { initial = null, zip = inspection(), saveResults = [], uploadResults = [], saveGate } = {}) {
  const calls = [], notices = [], leaveCalls = [], guards = []
  const publish = {
    provider: vue.ref('gitcode'),
    selectedRepository: vue.ref({ owner: 'fixture', name: 'maps', full_name: 'fixture/maps' }),
    selectedFullName: vue.ref('fixture/maps'), releases: vue.ref(initial ? [plain(initial)] : []),
  }
  for (const key of ['savingRelease', 'uploading', 'inspecting', 'deletingAsset', 'deletingRelease', 'repositoriesLoading', 'releasesLoading']) publish[key] = vue.ref(false)
  let editor
  publish.inspectZip = async path => { calls.push({ kind: 'inspect', path }); return plain(zip) }
  publish.saveRelease = async input => {
    calls.push({ kind: 'save', input: plain(input), rows: plain(editor.assetRows.value) })
    if (saveGate) await saveGate.promise
    if (saveResults.shift() === false) return null
    const before = publish.releases.value.find(item => item.id === input.releaseId)
    const savedMetadata = input.metadata ? plain(input.metadata) : before?.metadata ?? null
    if (savedMetadata && !savedMetadata.asset) savedMetadata.asset = assets.composeAssetRule(input.tag)
    const saved = {
      ...(before ? plain(before) : { id: 'release-created', title: input.tag, assets: [], draft: false, prerelease: false }),
      tag: input.tag, body: input.body.replace(/\r\n/g, '\n'), metadata: savedMetadata,
    }
    publish.releases.value = releases.upsertPublishRelease(publish.releases.value, saved)
    return saved
  }
  publish.uploadAsset = async (release, path) => {
    calls.push({ kind: 'upload', release: plain(release), path, rows: plain(editor.assetRows.value) })
    if (uploadResults.shift() === false) return null
    const asset = { id: 'asset-new', name: assets.renderAssetName(releases.releaseAssetRule(release), '', zip.info_version), size: zip.file_size }
    const current = publish.releases.value.find(item => item.id === release.id) ?? release
    publish.releases.value = releases.upsertPublishRelease(publish.releases.value, {
      ...current, assets: [...current.assets.filter(item => item.id !== asset.id), asset],
    })
    return asset
  }
  publish.deleteAsset = async (...args) => { calls.push({ kind: 'delete', args: plain(args) }); return true }
  const draftModule = load('src/composables/useMapPublishDraft.ts', {
    vue, '../mapPublishI18n': i18n, '../mapPublishRelations': relations,
  })
  const validationModule = load('src/composables/usePublishValidation.ts', { vue })
  const { useMapPublishEditor } = load('src/composables/useMapPublishEditor.ts', {
    vue,
    '../services/tauri': { api: { chooseLevelSetZip: async () => '/fixture/source.zip' } },
    '../i18n': { t: key => key },
    '../useNotify': { useNotify: () => Object.fromEntries(['error', 'warning', 'success'].map(kind => [kind, text => notices.push({ kind, text })])) },
    '../locale/backend': { backendMessageOf: String },
    '../mapPublishAsset': assets, '../mapPublishFlow': flow, '../mapPublishRelease': releases,
    './useMapPublishDraft': draftModule, './usePublishValidation': validationModule,
    './usePublishLeaveGuard': { usePublishLeaveGuard: (...args) => {
      guards.push(args)
      return { confirming: vue.ref(false), requestLeave: async () => {
        if (args[0].value) return false
        if (args[1].value) leaveCalls.push('confirm-draft')
        return true
      } }
    } },
  })
  const app = renderer.createApp({ setup() { editor = useMapPublishEditor(publish, vue.ref(null)); return () => null } })
  app.mount({})
  t.after(() => app.unmount())
  return { editor, publish, calls, notices, leaveCalls, guards }
}
const writes = calls => calls.filter(call => call.kind !== 'inspect')
const versions = value => value.asset_versions.map(row => [row.mapset_version, row.info_version])
async function choose(f) {
  if (f.publish.releases.value.length) f.editor.openManagement(f.publish.releases.value[0])
  else f.editor.openCreate()
  await f.editor.loadPendingZip('/fixture/source.zip')
  assert.equal(f.editor.pending.accepted, true)
}

test('新图确认前只准备本地草稿，确认时先保存完整正文与版本块再上传', async t => {
  const gate = deferred()
  const f = fixture(t, { saveGate: gate })
  await choose(f)
  f.editor.form.body = '## 3\r\n本次更新说明'
  assert.equal(await f.editor.saveReleaseStep(), true)
  assert.equal(await f.editor.saveDocument('metadata'), true)
  assert.deepEqual(writes(f.calls), [])
  const uploading = f.editor.uploadPending()
  await tick()
  const beforeUpload = writes(f.calls)
  assert.equal(beforeUpload.length, 1)
  assert.equal(beforeUpload[0].kind, 'save')
  assert.equal(beforeUpload[0].input.releaseId, null)
  assert.equal(beforeUpload[0].input.tag, 'Map')
  assert.equal(beforeUpload[0].input.body, '## 3\r\n本次更新说明')
  assert.deepEqual(versions(beforeUpload[0].input.metadata), [['3', '3']])
  assert.equal(beforeUpload[0].input.metadata.latest_mapset_version, '3')
  assert.equal(beforeUpload[0].input.metadata.latest_info_version, '3')
  assert.equal(beforeUpload[0].input.metadata.level_set_name, 'Map')
  assert.deepEqual(beforeUpload[0].input.metadata.requires, { 'example.base': '>=1' })
  assert.deepEqual(beforeUpload[0].rows, [], '生成上传快照不能预先修改可见版本表')
  assert.deepEqual(plain(f.editor.assetRows.value), [])
  gate.resolve()
  assert.equal(await uploading, true)
  const completed = writes(f.calls)
  assert.deepEqual(completed.map(call => call.kind), ['save', 'upload'])
  assert.equal(completed[1].release.id, 'release-created')
  assert.equal(completed[1].release.body, '## 3\n本次更新说明')
  assert.deepEqual(versions(completed[1].release.metadata), [['3', '3']])
  assert.equal(completed[1].path, '/fixture/source.zip')
  assert.deepEqual(plain(f.editor.assetRows.value).map(row => [row.mapsetVersion, row.infoVersion, row.size]), [['3', '3', 300]])
  assert.equal(f.editor.pending.path, '')
  assert.equal(f.editor.activeStep.value, 'versions')
  assert.equal(f.editor.preparing.value, false)
  assert.equal(f.editor.metadataDirty.value, false)
  assert.equal(f.editor.versionsDirty.value, false)
  assert.equal(f.editor.form.body, f.editor.workingRelease.value.body)
  assert.equal('awaitingMetadata' in f.editor, false)
  assert.equal(f.guards[0].length, 2, '离开保护只接忙碌与草稿，不再持有成功附件清理回调')
  await f.editor.backToList()
  assert.equal(f.editor.mode.value, 'list')
  assert.deepEqual(f.leaveCalls, [])
  assert.equal(writes(f.calls).some(call => call.kind === 'delete'), false)
})

test('新图上传保存trim身份后，原ZIP仍通过真实编辑器身份校验', async t => {
  const zip = { ...inspection(), level_set_name: ' Map ', level_set_name_zh: ' 地图 ', author: ' Author ' }
  const f = fixture(t, { zip })
  await choose(f)
  assert.equal(await f.editor.uploadPending(), true)
  assert.deepEqual(writes(f.calls).map(call => call.kind), ['save', 'upload'])
  const [save, upload] = writes(f.calls)
  assert.equal(save.input.metadata.level_set_name, 'Map')
  assert.equal(save.input.metadata.level_set_name_zh, '地图')
  assert.equal(save.input.metadata.author, 'Author')
  assert.equal(flow.zipIdentityMismatch(upload.release.metadata, zip), false)
  await f.editor.loadPendingZip('/fixture/source.zip')
  assert.equal(f.editor.pending.issue, null)
  assert.equal(f.editor.pending.accepted, true)
})

test('已有地图上传新版前保存说明、多语言、显式附件模板及作者历史版本顺序', async t => {
  const f = fixture(t, { initial: existingRelease() })
  await choose(f)
  f.editor.form.body += '\n\n## 3\n新版本说明'
  f.editor.document.setI18nValue('nameI18n', 'en', 'Edited map')
  f.editor.document.setI18nValue('noteI18n', 'zh-CN', '新介绍')
  f.editor.document.setI18nValue('noteI18n', 'en', 'New description')
  assert.equal(await f.editor.uploadPending(), true)
  const [save, upload] = writes(f.calls)
  assert.deepEqual(writes(f.calls).map(call => call.kind), ['save', 'upload'])
  assert.equal(save.input.releaseId, 'release-existing')
  assert.equal(save.input.body, '## 旧版\n保留历史说明\n\n## 3\n新版本说明')
  assert.equal(save.input.metadata.asset, 'legacy_${version}_bundle.zip')
  assert.deepEqual(save.input.metadata.name_i18n, { 'zh-CN': '旧地图', en: 'Edited map', fr: 'Carte' })
  assert.deepEqual(save.input.metadata.note_i18n, { 'zh-CN': '新介绍', en: 'New description' })
  assert.deepEqual(versions(save.input.metadata), [['2', '1.9'], ['1', '0.9'], ['3', '3']])
  assert.deepEqual(save.rows.map(row => row.mapsetVersion), ['2', '1'])
  assert.deepEqual(upload.rows.map(row => row.mapsetVersion), ['2', '1'], '保存响应尚无附件时不丢原顺序或预先插入新行')
  assert.equal(upload.release.metadata.asset, 'legacy_${version}_bundle.zip')
  assert.deepEqual(plain(f.editor.assetRows.value).map(row => row.mapsetVersion), ['2', '1', '3'])
  assert.deepEqual(plain(f.editor.unregistered.value), [])
})

test('旧无metadata的Release上传前补齐ZIP身份和完整声明，复用已有Release与附件', async t => {
  const f = fixture(t, { initial: existingRelease(false) })
  await choose(f)
  f.editor.form.body += '\n新版本日志'
  assert.equal(await f.editor.uploadPending(), true)
  const [save, upload] = writes(f.calls)
  assert.equal(save.input.releaseId, 'release-existing')
  assert.equal(save.input.body, '## 旧版\n保留历史说明\n新版本日志')
  assert.equal(save.input.metadata.level_set_name, 'Map')
  assert.equal(save.input.metadata.level_set_name_zh, '地图')
  assert.equal(save.input.metadata.author, 'Author')
  assert.deepEqual(versions(save.input.metadata), [['1', ''], ['3', '3']])
  assert.equal(upload.release.id, 'release-existing')
  assert.deepEqual(upload.release.assets.map(asset => asset.id), ['a1'])
  assert.deepEqual(plain(f.editor.assetRows.value).map(row => row.mapsetVersion), ['1', '3'])
})

test('旧无metadata的Release补齐trim声明后，原ZIP仍通过真实编辑器身份校验', async t => {
  const zip = { ...inspection(), level_set_name: ' Map ', level_set_name_zh: ' 地图 ', author: ' Author ' }
  const f = fixture(t, {
    initial: existingRelease(false),
    zip,
  })
  await choose(f)
  assert.equal(await f.editor.uploadPending(), true)
  assert.deepEqual(writes(f.calls).map(call => call.kind), ['save', 'upload'])
  const [save, upload] = writes(f.calls)
  assert.equal(save.input.releaseId, 'release-existing')
  assert.equal(save.input.metadata.level_set_name, 'Map')
  assert.equal(save.input.metadata.level_set_name_zh, '地图')
  assert.equal(save.input.metadata.author, 'Author')
  assert.equal(flow.zipIdentityMismatch(upload.release.metadata, zip), false)
  await f.editor.loadPendingZip('/fixture/source.zip')
  assert.equal(f.editor.pending.issue, null)
  assert.equal(f.editor.pending.accepted, true)
})

test('Release保存失败立即停止，保留ZIP和本地草稿且不执行上传', async t => {
  const f = fixture(t, { saveResults: [false] })
  await choose(f)
  f.editor.form.body = '失败后保留的说明'
  assert.equal(await f.editor.uploadPending(), false)
  assert.deepEqual(writes(f.calls).map(call => call.kind), ['save'])
  assert.equal(f.editor.pending.path, '/fixture/source.zip')
  assert.equal(f.editor.pending.accepted, true)
  assert.equal(f.editor.form.body, '失败后保留的说明')
  assert.equal(f.editor.activeTag.value, '')
  assert.equal(f.editor.workingRelease.value, null)
  assert.deepEqual(plain(f.editor.assetRows.value), [])
  assert.equal(f.editor.busyAny.value, false)
})

for (const hasExisting of [false, true]) {
  test(`${hasExisting ? '已有地图' : '新图'}上传失败保留ZIP，重试复用Release且不重复追加版本`, async t => {
    const f = fixture(t, { initial: hasExisting ? existingRelease() : null, uploadResults: [false, true] })
    await choose(f)
    f.editor.form.body = '本次说明'
    const originalRows = plain(f.editor.assetRows.value)
    assert.equal(await f.editor.uploadPending(), false)
    assert.equal(f.editor.activeTag.value, 'Map')
    const id = f.editor.workingRelease.value.id
    assert.equal(id, hasExisting ? 'release-existing' : 'release-created')
    assert.equal(f.editor.pending.path, '/fixture/source.zip')
    assert.equal(f.editor.pending.accepted, true)
    assert.equal(f.editor.form.body, '本次说明')
    assert.deepEqual(plain(f.editor.assetRows.value), originalRows)
    assert.equal(f.editor.busyAny.value, false)
    assert.equal(await f.editor.uploadPending(), true)
    assert.deepEqual(writes(f.calls).map(call => call.kind), ['save', 'upload', 'save', 'upload'])
    const saves = writes(f.calls).filter(call => call.kind === 'save')
    assert.equal(saves[0].input.releaseId, hasExisting ? id : null)
    assert.equal(saves[1].input.releaseId, id)
    for (const save of saves) {
      assert.equal(save.input.metadata.asset_versions.filter(row => row.mapset_version === '3').length, 1)
      assert.deepEqual(versions(save.input.metadata), hasExisting ? [['2', '1.9'], ['1', '0.9'], ['3', '3']] : [['3', '3']])
    }
    assert.equal(f.publish.releases.value.length, 1)
    assert.equal(f.editor.pending.path, '')
    await f.editor.backToList()
    assert.deepEqual(f.leaveCalls, [])
    assert.equal(writes(f.calls).some(call => call.kind === 'delete'), false)
  })
}

test('同名附件在保存Release之前拦截，避免覆盖既有版本', async t => {
  const initial = existingRelease()
  initial.assets.push({ id: 'already-uploaded', name: 'legacy_3_bundle.zip', size: 300 })
  const f = fixture(t, { initial })
  await choose(f)
  assert.equal(await f.editor.uploadPending(), false)
  assert.deepEqual(writes(f.calls), [])
  assert.equal(f.editor.pending.path, '/fixture/source.zip')
})

test('多地图ZIP在本地校验阶段停止，不保存Release或上传文件', async t => {
  const f = fixture(t, { zip: { ...inspection(), set_count: 2 } })
  f.editor.openCreate()
  await f.editor.loadPendingZip('/fixture/source.zip')
  assert.equal(f.editor.pending.accepted, false)
  assert.equal(f.editor.pending.issue, 'multiple')
  assert.equal(await f.editor.uploadPending(), false)
  assert.deepEqual(writes(f.calls), [])
})

for (const domain of ['release', 'metadata', 'versions']) {
  test(`日常${domain}保存保持三个草稿域隔离`, async t => {
    const f = fixture(t, { initial: existingRelease() })
    f.editor.openManagement(f.publish.releases.value[0])
    f.editor.form.body = '尚未独立保存的日志'
    f.editor.document.setI18nValue('nameI18n', 'en', 'Draft map')
    f.editor.document.draft.requires = [{ guid: 'draft.base', range: '*' }]
    f.editor.document.draft.releaseRelations = [{
      version: '==2', requires: [{ guid: 'draft.version', range: '*' }], compatible: [], conflicts: [],
    }]
    f.editor.assetRows.value = [...f.editor.assetRows.value].reverse()
    const action = domain === 'release' ? f.editor.saveReleaseStep() : f.editor.saveDocument(domain)
    assert.equal(await action, true)
    const [save] = writes(f.calls)
    assert.deepEqual(writes(f.calls).map(call => call.kind), ['save'])
    assert.equal(save.input.releaseId, 'release-existing')
    if (domain === 'release') {
      assert.equal(save.input.body, '尚未独立保存的日志')
      assert.equal('metadata' in save.input, false)
      assert.equal(f.editor.metadataDirty.value, true)
      assert.equal(f.editor.versionsDirty.value, true)
    } else {
      assert.equal(save.input.body, '## 旧版\n保留历史说明')
      assert.equal(f.editor.form.body, '尚未独立保存的日志')
      assert.equal(save.input.metadata.asset, 'legacy_${version}_bundle.zip')
      if (domain === 'metadata') {
        assert.equal(save.input.metadata.name_i18n.en, 'Draft map')
        assert.deepEqual(save.input.metadata.requires, { 'draft.base': '*' })
        assert.deepEqual(versions(save.input.metadata), [['2', '1.9'], ['1', '0.9']])
        assert.deepEqual(save.input.metadata.release_relations, [])
        assert.equal(f.editor.versionsDirty.value, true)
      } else {
        assert.equal(save.input.metadata.name_i18n.en, 'Saved map')
        assert.deepEqual(save.input.metadata.requires, { 'saved.base': '*' })
        assert.deepEqual(versions(save.input.metadata), [['1', '0.9'], ['2', '1.9']])
        assert.equal(save.input.metadata.release_relations[0].relations[0].guid, 'draft.version')
        assert.equal(f.editor.metadataDirty.value, true)
      }
    }
  })
}
