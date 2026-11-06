import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { stepAvailable } from '../src/mapPublishFlow.ts'
const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8').replace(/\r\n/g, '\n')

test('上传完成但未登记时，只可编辑第三、四步；已有地图不锁步', () => {
  for (const exists of [false, true]) {
    assert.equal(stepAvailable('release', exists, true), false)
    assert.equal(stepAvailable('asset', exists, true), false)
    assert.equal(stepAvailable('metadata', exists, true), exists)
    assert.equal(stepAvailable('versions', exists, true), exists)
  }
  for (const step of ['release', 'asset', 'metadata', 'versions']) assert.equal(stepAvailable(step, true, false), true)
})

test('本次上传按服务器 ID 和固定仓库清理，保存失败不丢清理记录', () => {
  const editor = read('src/composables/useMapPublishEditor.ts')
  assert.match(editor, /pendingUpload\.value = \{ id: asset.id, name: asset.name, owner: repository.owner, repository: repository.name \}/)
  assert.match(editor, /if \(!await publish.deleteAsset\(uploaded.id, uploaded\)\) return false\s+pendingUpload.value = null/)
  assert.match(editor, /if \(!saved \|\| epoch !== editorEpoch\) return false\s+if \(step === 'versions'\) \{\s+pendingUpload.value = null/)
  assert.doesNotMatch(editor.slice(editor.indexOf('async function saveDocument(')), /resetToList\(\)/)
  assert.match(editor, /if \(await leave.requestLeave\(\)\) resetToList\(\)/)
  for (const action of ['loadPendingZip', 'pickPendingZip', 'saveReleaseStep', 'uploadPending']) {
    const body = editor.slice(editor.indexOf(`async function ${action}(`))
    assert.match(body.slice(0, body.indexOf('\n  }')), /awaitingMetadata.value/)
  }
  const session = read('src/composables/useMapPublish.ts')
  const deletion = session.slice(session.indexOf('async function deleteAsset('))
  assert.match(deletion, /api.deletePublishAsset\(coordinates.owner, coordinates.repository, assetId\)/)
  assert.match(deletion, /assets: release.assets.filter\(asset => asset.id !== assetId\)/)
  assert.doesNotMatch(deletion, /loadReleases\(/)
})

test('退出弹窗取消不删除、失败可重试，路由与关窗共用离开门', () => {
  const guard = read('src/composables/usePublishLeaveGuard.ts')
  assert.match(guard, /onBeforeRouteLeave\(\(\) => requestLeave\(\)\)/)
  assert.match(guard, /onCloseRequested[\s\S]*event.preventDefault\(\)[\s\S]*await requestLeave\(\)/)
  assert.match(guard, /if \(request\) return request/)
  assert.match(guard, /if \(busy.value\) return Promise.resolve\(false\)/)
  assert.match(guard, /if \(await discard\(\)\) \{ finish\(true\); return true \}/)
  assert.match(guard, /instance.loading = false[\s\S]*return false/)
  assert.match(guard, /onNegativeClick: \(\) => \{ if \(instance.loading\) return false; finish\(false\) \}/)
  assert.match(guard, /window.removeEventListener\('beforeunload', preventReload\)/)
  assert.doesNotMatch(guard.slice(guard.indexOf('onBeforeUnmount(() =>')), /discard\(/)
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
  assert.match(view, /:disabled="!authenticated \|\| busyAny \|\| awaitingMetadata"/)
  assert.match(view, /:disabled="busyAny \|\| awaitingMetadata"/)
  assert.match(view, /busy: \(\) => busyAny.value \|\| awaitingMetadata.value/)
  assert.match(read('src/components/publish/PublishVersionsStep.vue'), /:remove-disabled="awaitingMetadata"/)
  assert.doesNotMatch(view, /mapPublish.subtitle|mapPublish.flowHint/)
})
