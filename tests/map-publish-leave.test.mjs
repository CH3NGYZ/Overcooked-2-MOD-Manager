import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { stepAvailable } from '../src/mapPublishFlow.ts'
const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8').replace(/\r\n/g, '\n')

test('确认上传先写完整声明，成功后直接进入版本管理', () => {
  for (const exists of [false, true]) {
    assert.equal(stepAvailable('release', exists, true), true)
    assert.equal(stepAvailable('asset', exists, true), true)
    assert.equal(stepAvailable('metadata', exists, true), true)
    assert.equal(stepAvailable('versions', exists, true), exists)
  }
  for (const step of ['release', 'asset', 'metadata', 'versions']) assert.equal(stepAvailable(step, true, false), true)
})

test('确认上传按统一顺序保存正文与声明，再按服务器 Release ID 上传附件', () => {
  const editor = read('src/composables/useMapPublishEditor.ts')
  assert.doesNotMatch(editor, /pendingUpload|awaitingMetadata/)
  assert.match(editor, /const current = workingRelease.value/)
  assert.match(editor, /const rows = \[\.\.\.assetRows.value[\s\S]*pending.inspection.file_size/)
  assert.match(editor, /releaseId: current\?\.id \?\? null/)
  assert.match(editor, /body: form.body, metadata: buildMetadata\(rows\)/)
  assert.ok(editor.indexOf('publish.saveRelease(') < editor.indexOf('publish.uploadAsset('))
  assert.match(editor, /assetRows.value = rows\.map\(row => row\.mapsetVersion === version \?/)
  assert.doesNotMatch(editor.slice(editor.indexOf('async function saveDocument(')), /resetToList\(\)/)
  assert.match(editor, /if \(await leave.requestLeave\(\)\) resetToList\(\)/)
  const session = read('src/composables/useMapPublish.ts')
  const deletion = session.slice(session.indexOf('async function deleteAsset('))
  assert.match(deletion, /api.deletePublishAsset\(coordinates.owner, coordinates.repository, assetId, tag, coordinates.provider\)/)
  assert.match(deletion, /assets: release.assets.filter\(asset => asset.id !== assetId\)/)
  assert.doesNotMatch(deletion, /loadReleases\(/)
})

test('退出弹窗取消不删除、失败可重试，路由与关窗共用离开门', () => {
  const guard = read('src/composables/usePublishLeaveGuard.ts')
  const shared = read('src/composables/usePageLeaveGuard.ts')
  const close = read('src/composables/useAppCloseGuard.ts')
  assert.match(guard, /return usePageLeaveGuard\(/)
  assert.match(guard, /usePublishLeaveGuard\(\s*busy: Readonly<Ref<boolean>>,[\s\S]*dirty: Readonly<Ref<boolean>>/)
  assert.doesNotMatch(guard, /uploadedName|discardUpload|deleteAndLeave|discard: \(\)/)
  assert.match(shared, /onBeforeRouteLeave\(\(\) => requestLeave\(\)\)/)
  assert.match(shared, /registerPageLeaveGuard\(\{ requestLeave, blocked \}\)/)
  assert.doesNotMatch(shared, /onCloseRequested|getCurrentWindow/)
  assert.match(close, /onCloseRequested[\s\S]*event.preventDefault\(\)[\s\S]*await guard.requestLeave\(\)[\s\S]*await api.exitApp\(\)/)
  assert.doesNotMatch(close, /appWindow\.close\(|\.destroy\(/)
  assert.equal((read('src/App.vue').match(/<AppCloseGuard\b/g) ?? []).length, 1)
  assert.match(read('src/components/AppCloseGuard.vue'), /useAppCloseGuard\(\)/)
  assert.match(shared, /if \(request\) return request/)
  assert.match(shared, /if \(options.busy.value\) return Promise.resolve\(false\)/)
  assert.match(guard, /title: t\('mapPublish.discardDraftTitle'\)/)
  assert.match(guard, /content: t\('mapPublish.discardDraftConfirm'\)/)
  assert.match(guard, /onPositiveClick: \(\) => \{ finish\(true\) \}/)
  assert.match(guard, /onNegativeClick: \(\) => \{ finish\(false\) \}/)
  assert.match(shared, /window.removeEventListener\('beforeunload', preventReload\)/)
  assert.doesNotMatch(shared.slice(shared.indexOf('onBeforeUnmount(() =>')), /discard\(/)
})

test('地图集为带表头的表格，上传后的设置/刷新/删除不得绕过门控', () => {
  const list = read('src/components/publish/PublishReleaseList.vue')
  assert.match(list, /<n-table class="release-table"/)
  assert.equal((list.match(/scope="col"/g) ?? []).length, 8)
  assert.deepEqual([...list.matchAll(/<th scope="col">\{\{ t\('mapPublish\.(\w+)'\)/g)].map(match => match[1]),
    ['mapSetColumn', 'levelSetNameZh', 'levelSetName', 'author', 'latestVersion', 'versionCount', 'statusColumn', 'actionsColumn'])
  assert.doesNotMatch(list, /displayName|name_i18n|row\.name/)
  assert.match(list, /row\.release\.metadata\?\.level_set_name_zh/)
  assert.match(list, /row\.release\.metadata\?\.level_set_name \|\|/)
  assert.match(read('src/styles/publish.css'), /\.release-table td \{[^}]*text-align: center/)
  assert.match(read('src/styles/publish.css'), /\.release-actions \{[^}]*justify-content: center/)
  assert.match(list, /scope="row"/)
  assert.doesNotMatch(list, /<ul|repositoryScopeHint/)
  const view = read('src/views/MapPublishView.vue')
  assert.match(view, /:disabled="!authenticated \|\| busyAny"/)
  assert.match(view, /:disabled="busyAny"/)
  assert.match(view, /busy: \(\) => busyAny.value/)
  assert.doesNotMatch(view, /awaitingMetadata/)
  assert.doesNotMatch(read('src/components/publish/PublishVersionsStep.vue'), /remove-disabled|awaitingMetadata/)
  assert.doesNotMatch(view, /mapPublish.subtitle|mapPublish.flowHint/)
})
