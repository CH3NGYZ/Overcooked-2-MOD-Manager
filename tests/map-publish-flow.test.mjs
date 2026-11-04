import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 发布页三步式流程（`src/mapPublishFlow.ts`）。
 *
 * 锁三件事：一键执行还缺哪些输入、默认 tag 的形状（tag 是地图身份，必须是
 * 合法 git ref）、以及 403 的两类原因能不能从后端 tag 里认出来（权限不足要
 * 引导改 App，限流只要等一下）。
 */
const {
  PUBLISH_STEP_ORDER,
  defaultMapTag,
  missingPublishInputs,
  parsePublishPermissionFailure,
  permissionScopeKey,
} = await import('../src/mapPublishFlow.ts')

test('三步顺序固定：先 Release，再 ZIP，最后写块', () => {
  assert.deepEqual([...PUBLISH_STEP_ORDER], ['release', 'asset', 'metadata'])
})

test('什么都没填时三步全缺', () => {
  assert.deepEqual(
    missingPublishInputs({ tag: '', zipReady: false, assetVersion: '', levelSetName: '' }),
    ['release', 'asset', 'metadata'],
  )
})

test('新建页只要输入齐就能一键执行，Release 还没建不算缺', () => {
  assert.deepEqual(
    missingPublishInputs({
      tag: 'gaoda',
      zipReady: true,
      assetVersion: '1.2.0',
      levelSetName: 'GAODA',
    }),
    [],
  )
})

test('ZIP 没拖进来或版本号不能用时，资产这一步算没准备好', () => {
  const base = { tag: 'gaoda', levelSetName: 'GAODA' }
  assert.deepEqual(missingPublishInputs({ ...base, zipReady: false, assetVersion: '1.2.0' }), [
    'asset',
  ])
  assert.deepEqual(missingPublishInputs({ ...base, zipReady: true, assetVersion: '' }), ['asset'])
  // 带 v 前缀的不是地图版本号：附件名里抽不出版本，先拦住。
  assert.deepEqual(missingPublishInputs({ ...base, zipReady: true, assetVersion: 'v1.2.0' }), [
    'asset',
  ])
})

test('关卡集名缺失时第三步不放过', () => {
  assert.deepEqual(
    missingPublishInputs({
      tag: 'gaoda',
      zipReady: true,
      assetVersion: '1.2.0',
      levelSetName: '  ',
    }),
    ['metadata'],
  )
})

test('默认 tag 符合 git ref：小写、只留字母数字与横线', () => {
  assert.equal(defaultMapTag('GAODA'), 'gaoda')
  assert.equal(defaultMapTag('Map 2'), 'map-2')
  // 中文名没有 ASCII 字母数字，给不出默认值就留空让作者自己填。
  assert.equal(defaultMapTag('大厨'), '')
  assert.equal(defaultMapTag('  -GAODA-  '), 'gaoda')
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

  const rateLimited = parsePublishPermissionFailure(
    'mapPublish.rateLimited?reset=1762264817',
  )
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
  assert.equal(parsePublishPermissionFailure('mapPublish.forbidden?reason=Resource%20not%20accessible'), null)
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
