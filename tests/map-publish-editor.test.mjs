import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync, readdirSync } from 'node:fs'
import { versionRowsOf, reconcileVersionRows, unregisteredReleaseVersions, upsertPublishRelease, releaseAssetRule } from '../src/mapPublishRelease.ts'
import { mapPublish } from '../src/locale/mapPublish/index.ts'
import { publishZipIssue } from '../src/mapPublishFlow.ts'

const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const release = {
  id: 1, tag: 'Map', title: 'Map', body: '', assets: [
    { id: 1, name: 'Map_v1.zip', size: 10 }, { id: 2, name: 'Map_v2.zip', size: 20 },
    { id: 3, name: 'Map_v3-fix.zip', size: 30 }, { id: 4, name: 'other_v9.zip', size: 40 },
  ],
  metadata: { asset: '', asset_versions: [{ mapset_version: '2', info_version: '1.9' }, { mapset_version: '1', info_version: '1' }, { mapset_version: 'missing', info_version: '' }] },
}

test('发布表保留作者顺序、追加表外版本并排除不匹配与缺失附件', () => {
  assert.equal(releaseAssetRule(release), 'Map_v${version}.zip')
  assert.deepEqual(versionRowsOf(release).map(r => [r.mapsetVersion, r.infoVersion, r.size]), [['2', '1.9', 20], ['1', '1', 10], ['3-fix', '', 30]])
  assert.deepEqual(unregisteredReleaseVersions(release), ['3-fix'])
  assert.deepEqual(unregisteredReleaseVersions({ ...release, metadata: null }), ['1', '2', '3-fix'])
  assert.deepEqual(versionRowsOf(null), [])
})

test('上传与删除合并保留待保存的排序和本次上传识别的 info 版本', () => {
  const rows = [{ mapsetVersion: '1', infoVersion: 'edited', size: 0 }, { mapsetVersion: '2', infoVersion: 'custom', size: 0 }]
  const updated = reconcileVersionRows(rows, release)
  assert.deepEqual(updated.map(r => [r.mapsetVersion, r.infoVersion]), [['1', 'edited'], ['2', 'custom'], ['3-fix', '']])
  assert.equal(rows[0].size, 0)
  assert.equal(updated[0].size, 10)
  const deleted = { ...release, assets: release.assets.filter(a => a.id !== 1) }
  assert.deepEqual(reconcileVersionRows(updated, deleted).map(r => r.mapsetVersion), ['2', '3-fix'])
})

test('显式附件规则优先，列表更新只替换目标 Release', () => {
  const explicit = { ...release, metadata: { asset: 'other_v${version}.zip', asset_versions: [] } }
  assert.deepEqual(versionRowsOf(explicit).map(r => r.mapsetVersion), ['9'])
  const other = { ...release, id: 2, tag: 'Other' }
  const saved = { ...release, body: 'saved' }
  assert.deepEqual(upsertPublishRelease([release, other], saved), [saved, other])
  assert.equal(upsertPublishRelease([release, other], saved)[1], other)
  assert.deepEqual(upsertPublishRelease([other], saved), [saved, other])
})

test('View 单次会话与四步子组件保持 400 行边界', () => {
  const view = read('src/views/MapPublishView.vue')
  assert.equal((view.match(/useMapPublish\(\)/g) ?? []).length, 1)
  const files = readdirSync(new URL('../src/components/publish/', import.meta.url)).map(n => 'src/components/publish/' + n)
  files.push('src/views/MapPublishView.vue', 'src/composables/useMapPublishEditor.ts', 'src/composables/useMapPublishDraft.ts', 'src/composables/usePublishValidation.ts', 'src/composables/usePublishLeaveGuard.ts', 'src/styles/publish.css', 'src/mapPublishRelease.ts')
  for (const file of files) {
    const content = read(file)
    assert.ok(content.split('\n').length <= 400, file)
    if (!file.endsWith('MapPublishView.vue')) assert.doesNotMatch(content, /useMapPublish\(\)/, file)
  }
  assert.match(view, /!canSelectStep\(step\)/)
})

test('编辑页复用标题覆盖与受控分段导航，返回仍走原离开保护', () => {
  const view = read('src/views/MapPublishView.vue')
  assert.match(view, /watch\(mode, value => setPageTitleOverride\(value === 'list' \? null : 'mapPublish.editTitle'\)/)
  assert.match(view, /onBeforeUnmount\(\(\) => setPageTitleOverride\(null\)\)/)
  const header = view.slice(view.indexOf('<Teleport'), view.indexOf('</Teleport>'))
  assert.match(header, /<template v-if="mode === 'list'">[\s\S]*PublishSettingsPopover/)
  assert.match(header, /<n-button v-else[^>]*:disabled="busyAny"[^>]*@click="backToList"/)
  assert.match(header, /mapPublish.backToMapSets/)
  assert.doesNotMatch(view, /publish-head|identityTitle|identityVersions|mapPublish.mapManagement/)
  assert.match(view, /<n-tabs v-if="!selectingZip" :value="activeStep" type="segment"/)
  assert.match(view, /@update:value="selectStep"/)
  assert.match(view, /<n-tab-pane v-for="\(step, index\) in PUBLISH_STEP_ORDER"[^>]*:name="step"[\s\S]*?:disabled="busyAny \|\| !canSelectStep\(step\)"/)
  assert.match(view, /<template #tab>\{\{ index \+ 1 \}\}. \{\{ stepTitle\(step\) \}\}<\/template>/)
  assert.doesNotMatch(view, /v-model:value="activeStep"/)
  const css = read('src/styles/publish.css')
  assert.match(css, /\.publish-step-nav \.n-tab-pane \{ display: none; \}/)
  assert.match(css, /\.publish-step-nav \.n-tabs-rail \{ align-items: stretch; \}/)
  assert.match(css, /\.publish-step-nav \.n-tabs-tab-wrapper \{[^}]*min-width: 0;[^}]*align-items: stretch;/)
  assert.match(css, /\.publish-step-nav \+ \.panel \{ margin-top: 16px; \}/)
  assert.equal(mapPublish.editTitle['zh-CN'], '地图编辑')
})

test('四步保存、info 版本与前置同名附件检查不退回一键发布', () => {
  const editor = read('src/composables/useMapPublishEditor.ts')
  assert.match(editor, /function openManagement[\s\S]*?activeStep\.value = 'versions'/)
  assert.match(editor, /function openCreate[\s\S]*?activeStep\.value = 'release'/)
  assert.match(editor, /const version = result\.info_version\.trim\(\)/)
  assert.match(editor, /asset: existingRule\.value/)
  assert.match(editor, /current\.assets\.some\(asset => asset\.name === name\)/)
  assert.match(editor, /metadata: complete \? buildMetadata\(\) : metadataForStep\(step, release.metadata, buildMetadata\(\)\)/)
  assert.match(editor, /body: complete \? form.body : release.body/)
  assert.match(editor, /const complete = step === 'versions' && !release.metadata/)
  assert.match(editor, /if \(complete\) \{[\s\S]*metadataDirty.value = false[\s\S]*form.body = saved.body/)
  assert.doesNotMatch(editor, /pendingUpload|awaitingMetadata/)
  assert.doesNotMatch(editor, /publishVersion|missingPublishInputs|versionFromFileName|resetPublishSteps/)
  const session = read('src/composables/useMapPublish.ts')
  assert.doesNotMatch(session.slice(session.indexOf('async function saveRelease('), session.indexOf('async function deleteRelease(')), /loadReleases\(/)
  assert.doesNotMatch(session.slice(session.indexOf('async function uploadAsset('), session.indexOf('async function inspectZip(')), /loadReleases\(/)
})

test('本地准备先于远端创建，确认上传与独立维护使用不同保存边界', () => {
  const editor = read('src/composables/useMapPublishEditor.ts')
  const release = editor.slice(editor.indexOf('async function saveReleaseStep('), editor.indexOf('async function uploadPending('))
  assert.ok(release.indexOf('if (preparing.value)') < release.indexOf('publish.saveRelease('))
  assert.doesNotMatch(release, /releaseId: null/)
  const upload = editor.slice(editor.indexOf('async function uploadPending('), editor.indexOf('function buildMetadata('))
  assert.match(upload, /validate\('release'\)[\s\S]*validate\('metadata'\)[\s\S]*validate\('asset'\)/)
  assert.match(upload, /const current = workingRelease.value/)
  assert.match(upload, /const rows = \[\.\.\.assetRows.value[\s\S]*pending.inspection.file_size/)
  assert.match(upload, /releaseId: current\?\.id \?\? null/)
  assert.match(upload, /body: form.body, metadata: buildMetadata\(rows\)/)
  assert.ok(upload.indexOf('publish.saveRelease(') < upload.indexOf('publish.uploadAsset('))
  assert.match(upload, /assetRows.value = rows\.map\(row => row\.mapsetVersion === version \?/)
  assert.ok(editor.includes('if (!workingRelease.value && !tagEdited)'))
  assert.match(read('src/components/publish/PublishReleaseStep.vue'), /@update:value="setTag"/)
  const guard = read('src/composables/usePublishLeaveGuard.ts')
  assert.match(editor, /usePublishLeaveGuard\(operationBusy, localDirty\)/)
  assert.match(guard, /return usePageLeaveGuard\(\{\s+busy,\s+dirty,/)
  assert.match(guard, /mapPublish.discardDraftConfirm/)
  assert.match(read('src/components/publish/PublishAssetStep.vue'), /pending.inspection.file_size/)
})

test('必填按钮可点、缺项脉冲可重播并清理，动画只改边框', () => {
  for (const step of ['Release', 'Asset', 'Metadata', 'Versions']) {
    const text = read(`src/components/publish/Publish${step}Step.vue`)
    assert.match(text, /:disabled="busyAny"/)
    assert.doesNotMatch(text, /canPublish|missingInputs|sequenceNeed|bodyHint/)
  }
  const validation = read('src/composables/usePublishValidation.ts')
  assert.match(validation, /if \(field\.startsWith\('i18n-'\)\) metadataSection\.value = 'i18n'/)
  assert.match(validation, /void fieldTarget\.offsetWidth/)
  assert.match(validation, /setTimeout\(clear, 1600\)/)
  assert.match(validation, /onBeforeUnmount\(clear\)/)
  const css = read('src/styles/publish.css')
  const keyframe = css.slice(css.indexOf('@keyframes'), css.indexOf('@media', css.indexOf('@keyframes')))
  assert.ok(keyframe.includes('border-color'))
  assert.doesNotMatch(keyframe, /background/)
  assert.match(css, /prefers-reduced-motion: reduce/)
})

test('资料只保留多语言和默认关系，独立第四步负责版本表和资产覆盖', () => {
  const step = read('src/components/publish/PublishMetadataStep.vue')
  assert.match(step, /<n-tabs v-model:value="metadataSection"[^>]*type="segment"/)
  for (const name of ['i18n', 'relations']) assert.match(step, new RegExp(`<n-tab-pane name="${name}"[^>]*><template #tab>`))
  // 空面板只当分段按钮，表格在外层 v-if 里渲染，草稿更新不经过组件库缓存的插槽。
  assert.doesNotMatch(step, /<n-tab-pane[^>]*>(?!<template #tab>)/)
  assert.match(step, /<section v-if="metadataSection === 'i18n'"/)
  assert.doesNotMatch(step, /name="versions"|VersionOrderTable|draft.releaseRelations/)
  assert.match(step, /<n-table class="metadata-table i18n-table"/)
  assert.match(step, /v-for="language in MAP_I18N_LANGUAGES"/)
  assert.match(step, /v-for="field in I18N_FIELD_ORDER"/)
  assert.match(step, /:data-validation-field="`i18n-\$\{field\.id\}-\$\{language\}`"/)
  assert.equal((step.match(/<PublishRelationTables /g) ?? []).length, 1)
  const versions = read('src/components/publish/PublishVersionsStep.vue')
  assert.match(versions, /<VersionOrderTable/)
  assert.match(versions, /@relations="editRelations"/)
  assert.match(versions, /<PublishRelationTables/)
  assert.match(versions, /saveDocument\('versions'\)/)
  const relations = read('src/components/publish/PublishRelationTables.vue')
  assert.match(relations, /<section v-for="kind in kinds"/)
  assert.match(relations, /<n-table class="metadata-table relations-table"/)
  assert.match(relations, /mapPublish.relationAddRow/)
  assert.match(versions, /draft.releaseRelations/)
  assert.match(versions, /assetRelationOverride\(draft, draft.releaseRelations, row.mapsetVersion\)/)
  assert.match(versions, /<AppModal :show="relationsOpen"/)
  assert.match(versions, /const validation = usePublishValidation\(relationRoot\)/)
  assert.match(versions, /relationIssue\(emptyRelationTables\(\), \[entry\]\)/)
  assert.match(versions, /withAssetRelations\(draft.releaseRelations, editingVersion.value, entry\)/)
  assert.doesNotMatch(versions, /addOverride|v-model:value="entry.version"|relation-scope-head|mapPublish.versionRelations/)
  assert.doesNotMatch(step, /mapPublish.relationsOptional|mapPublish.defaultRelations/)
  assert.doesNotMatch(relations, /rowgroup|rowspan/)
  assert.doesNotMatch(step, /n-collapse|relation-row|i18n-rows/)
  const table = read('src/components/publish/VersionOrderTable.vue')
  assert.equal((table.match(/<th scope="col">/g) ?? []).length, 7)
  assert.doesNotMatch(table, /NInput|n-input|v-model:value="row\.(infoVersion|mapsetVersion)"/)
  assert.match(table, /\{\{ row.infoVersion \|\| '—' \}\}/)
  assert.equal(mapPublish.assetRelations['zh-CN'], '依赖')
  assert.match(table, /const displayRows = computed\(\(\) => \[\.\.\.rows\.value\]\.reverse\(\)\)/)
  assert.match(table, /reorderVersionRows\(rows\.value, from, to\)/)
  assert.match(table, /event\.altKey/)
  assert.match(table, /rect\.top \+ rect\.height \/ 2/)
  const css = read('src/styles/publish.css')
  for (const cls of ['metadata-tabs', 'metadata-section', 'metadata-table', 'i18n-cell', 'relation-table-head', 'metadata-footer', 'version-table', 'drag-handle']) {
    assert.match(css, new RegExp(`\\.map-publish \\.${cls}\\b`), cls)
  }
  assert.doesNotMatch(css, /\.i18n-rows|\.relation-row|\.relation-block|\.field-section-title|\.map-publish \.advanced\b/)
})

test('ZIP 只读预览与可上传性分开：三字段严格匹配、多图和无效版本均拦截', () => {
  const zip = { level_set_name: 'Map', level_set_name_zh: '地图', author: 'Author', set_count: 1, info_version: '1.2', mapset_version: '99' }
  assert.equal(publishZipIssue(zip, null), null)
  assert.equal(publishZipIssue(zip, zip), null)
  for (const field of ['level_set_name', 'level_set_name_zh', 'author']) {
    assert.equal(publishZipIssue({ ...zip, [field]: 'Different' }, zip), 'identity')
    assert.equal(publishZipIssue({ ...zip, [field]: '' }, zip), 'identity')
  }
  assert.equal(publishZipIssue({ ...zip, set_count: 2 }, null), 'multiple')
  assert.equal(publishZipIssue({ ...zip, set_count: 0 }, null), 'multiple')
  assert.equal(publishZipIssue({ ...zip, info_version: '' }, null), 'version')
  assert.equal(publishZipIssue({ ...zip, info_version: 'bad' }, null), 'version')
  assert.equal(zip.mapset_version, '99')
})

test('识别表先挂载再校验，不符保留表格且不得污染草稿或上传', () => {
  const editor = read('src/composables/useMapPublishEditor.ts')
  const load = editor.slice(editor.indexOf('async function loadPendingZip('), editor.indexOf('async function pickPendingZip('))
  const preview = load.indexOf('inspection: result')
  const flush = load.indexOf('await nextTick()')
  const check = load.indexOf('publishZipIssue(result, locked)')
  const stop = load.indexOf('if (pending.issue) return')
  const apply = load.indexOf('document.applyInspection(result, !workingRelease.value?.metadata)')
  assert.ok(preview > 0 && preview < flush && flush < check && check < stop && stop < apply)
  assert.doesNotMatch(load.slice(preview), /rejectPending\(/)
  assert.match(editor, /step === 'asset' && !pending\.accepted/)
  const asset = read('src/components/publish/PublishAssetStep.vue')
  assert.match(asset, /<n-table class="inspection-table"/)
  assert.match(asset, /<th scope="row">/)
  assert.ok(asset.indexOf('</n-table>') < asset.indexOf('v-if="inspectionError"'))
  assert.doesNotMatch(asset, /inspection-grid/)
})

test('地图发布实际源表的所有文案五语全部登记', () => {
  for (const [key, translations] of Object.entries(mapPublish)) {
    for (const language of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) assert.ok(translations[language]?.trim(), `${key}/${language}`)
  }
})
