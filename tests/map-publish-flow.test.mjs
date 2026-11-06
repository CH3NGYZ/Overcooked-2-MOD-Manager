import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 发布页的流程逻辑（`src/mapPublishFlow.ts`）。
 *
 * 锁这几件事：四个步骤的导航顺序与可达条件（无 Release 也可准备草稿）、
 * 每一步各自的前置（互不捆绑）、哪些版本已上传但还没登记、拖动排序只改存储
 * 顺序、默认 tag 的形状，以及 403 的两类原因能不能从后端 tag 里认出来。
 */
const {
  PUBLISH_STEP_ORDER,
  defaultMapTag,
  isMapPublishTag,
  zipIdentityMismatch,
  stepAvailable,
  stepIssues,
  unregisteredVersions,
  parsePublishPermissionFailure,
  reorderVersionRows,
  permissionScopeKey,
} = await import('../src/mapPublishFlow.ts')

const ready = {
  tag: 'gaoda',
  releaseExists: true,
  assetName: 'gaoda_v1.2.0.zip',
  versionText: '1.2.0',
  levelSetName: 'GAODA',
  versionCount: 1,
}

test('四步导航顺序固定：发布信息、地图信息、上传确认、版本管理', () => {
  assert.deepEqual([...PUBLISH_STEP_ORDER], ['release', 'metadata', 'asset', 'versions'])
})

test('未选 ZIP 留在选择入口，本地识别通过即可准备前三步', () => {
  assert.equal(stepAvailable('release', false), false)
  for (const step of ['release', 'metadata', 'asset']) assert.equal(stepAvailable(step, false, false, true), true)
  assert.equal(stepAvailable('versions', false, false, true), false)
  assert.equal(stepAvailable('asset', false), false)
  assert.equal(stepAvailable('metadata', false), false)
  assert.equal(stepAvailable('asset', true), true)
  assert.equal(stepAvailable('metadata', true), true)
  assert.equal(stepAvailable('versions', false), false)
  assert.equal(stepAvailable('versions', true), true)
})

test('输入齐全时四步各自都能保存', () => {
  for (const step of PUBLISH_STEP_ORDER) assert.deepEqual(stepIssues(step, ready), [])
})

test('Release 信息只要 tag，不要求 ZIP 或地图资料', () => {
  assert.deepEqual(
    stepIssues('release', { ...ready, releaseExists: false, assetName: '', versionText: '', levelSetName: '' }),
    [],
  )
  assert.deepEqual(stepIssues('release', { ...ready, tag: '  ' }), ['tag'])
})

test('上传 ZIP 不要求地图资料齐全', () => {
  assert.deepEqual(stepIssues('asset', { ...ready, levelSetName: '', versionCount: 0 }), [])
  assert.deepEqual(stepIssues('asset', { ...ready, releaseExists: false }), [])
  assert.deepEqual(stepIssues('asset', { ...ready, releaseExists: false, tag: '' }), ['tag'])
  assert.deepEqual(stepIssues('asset', { ...ready, assetName: ' ' }), ['zip'])
  // 带 v 前缀或空格的不是版本号文本：附件名里抽不出版本，先拦住
  assert.deepEqual(stepIssues('asset', { ...ready, versionText: 'v1.2.0' }), ['version'])
  assert.deepEqual(stepIssues('asset', { ...ready, versionText: '1.2.0 beta' }), ['version'])
  // 带后缀的版本号是合法输入
  assert.deepEqual(stepIssues('asset', { ...ready, versionText: '3.4.1-fix' }), [])
})

test('地图资料与版本分别校验：只有版本管理要求版本表非空', () => {
  assert.deepEqual(stepIssues('metadata', { ...ready, levelSetName: '  ' }), ['levelSetName'])
  assert.deepEqual(stepIssues('metadata', { ...ready, versionCount: 0 }), [])
  assert.deepEqual(stepIssues('versions', { ...ready, versionCount: 0 }), ['versionRows'])
  // 与 ZIP 是否选好无关
  assert.deepEqual(stepIssues('metadata', { ...ready, assetName: '', versionText: '' }), [])
})

test('已上传未登记的版本按表顺序列出', () => {
  assert.deepEqual(unregisteredVersions(['0.1', '0.2', '0.3'], ['0.1']), ['0.2', '0.3'])
  assert.deepEqual(unregisteredVersions(['0.1'], [' 0.1 ']), [])
  assert.deepEqual(unregisteredVersions(['', '0.2'], []), ['0.2'])
})

test('拖动只改存储顺序：显示反转，存储仍是旧→新', () => {
  const rows = [{ id: 'a' }, { id: 'b' }, { id: 'c' }]
  // 显示顺序是 c, b, a；把显示第一行（最新版）拖到最后一行
  const moved = reorderVersionRows(rows, 0, 2)
  assert.deepEqual(
    moved.map((row) => row.id),
    ['c', 'a', 'b'],
  )
  // 原数组不被修改
  assert.deepEqual(
    rows.map((row) => row.id),
    ['a', 'b', 'c'],
  )
})

test('拖动到原位或越界时不改变顺序', () => {
  const rows = ['a', 'b', 'c']
  assert.deepEqual(reorderVersionRows(rows, 1, 1), ['a', 'b', 'c'])
  assert.deepEqual(reorderVersionRows(rows, 9, 0), ['a', 'b', 'c'])
  // 越界夹到最后一行：最新版被拖到显示末尾，存储里就落到最前
  assert.deepEqual(reorderVersionRows(rows, 0, 99), ['c', 'a', 'b'])
})

test('默认 tag 来自 info 英文名，保留大小写 ASCII 字母数字', () => {
  assert.equal(defaultMapTag('GAODA'), 'GAODA')
  assert.equal(defaultMapTag('My Kitchen! 2'), 'MyKitchen2')
  // 中文名没有 ASCII 字母数字，给不出默认值就留空让作者自己填。
  assert.equal(defaultMapTag('大厨'), '')
  assert.equal(defaultMapTag('  -GAODA-  '), 'GAODA')
})

test('发布 tag 只允许 ASCII 字母数字', () => {
  assert.equal(isMapPublishTag('Map2026'), true)
  for (const value of ['', 'has space', 'pin-yin', '中文', 'a_b', 'a.b']) {
    assert.equal(isMapPublishTag(value), false)
  }
})

test('ZIP 身份比较三个字段并严格比较空值', () => {
  const metadata = { level_set_name: 'Map', level_set_name_zh: '地图', author: 'Alice' }
  assert.equal(zipIdentityMismatch(metadata, { ...metadata }), false)
  assert.equal(zipIdentityMismatch(metadata, { ...metadata, author: 'Bob' }), true)
  assert.equal(zipIdentityMismatch(metadata, { ...metadata, level_set_name_zh: '' }), true)
  assert.equal(zipIdentityMismatch({ ...metadata, author: '' }, { ...metadata, author: 'Alice' }), true)
})

test('权限失败的 tag 解出权限名与限流时间', () => {
  const permission = parsePublishPermissionFailure(
    'mapPublish.missingPermission?permission=contents%3Dwrite',
  )
  assert.deepEqual(permission, {
    tag: 'mapPublish.missingPermission',
    permission: 'contents=write',
    retryAfter: '',
    rateLimited: false,
  })

  const rateLimited = parsePublishPermissionFailure('mapPublish.rateLimited?reset=1762264817')
  assert.equal(rateLimited?.rateLimited, true)
  assert.equal(rateLimited?.retryAfter, '1762264817')
})

test('两个参数都带、且值里有需要编码的字符时也能解', () => {
  const parsed = parsePublishPermissionFailure(
    'mapPublish.missingPermission?permission=administration%3Dwrite%3B%20contents%3Dwrite&reset=1',
  )
  assert.equal(parsed?.permission, 'administration=write; contents=write')
  assert.equal(parsed?.retryAfter, '1')
})

test('其它失败与非法输入不亮权限横幅', () => {
  assert.equal(
    parsePublishPermissionFailure('mapPublish.forbidden?reason=Resource%20not%20accessible'),
    null,
  )
  assert.equal(parsePublishPermissionFailure('mapPublish.missingPermission'), null)
  assert.equal(parsePublishPermissionFailure(''), null)
  assert.equal(parsePublishPermissionFailure(null), null)
  assert.equal(parsePublishPermissionFailure(403), null)
})

test('权限名归到语言表里的四个固定范围', () => {
  assert.equal(permissionScopeKey('contents=write'), 'contents')
  assert.equal(permissionScopeKey('Contents: Read and write'), 'contents')
  assert.equal(permissionScopeKey('administration=write'), 'administration')
  assert.equal(permissionScopeKey('metadata=read'), 'metadata')
  assert.equal(permissionScopeKey('workflows=write'), 'other')
  assert.equal(permissionScopeKey(''), 'other')
})
