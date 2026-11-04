import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 发布页附件名渲染（`src/mapPublishAsset.ts`）。
 *
 * 这条链路是「tag = 一张地图、版本号只在附件名里」能成立的前提：管理器用
 * `versioned_asset_rule` 从附件名抽 `[0-9]+(\.[0-9]+)*`，抽不到就没有版本，
 * 更新判断也全部失效。所以版本号形状、模板占位符、留空时的默认模板都锁在这里。
 */
const {
  ASSET_VERSION_PLACEHOLDER,
  compareMapVersions,
  defaultTemplate,
  isMapVersion,
  latestVersionOf,
  levelSetSlug,
  renderAssetName,
  safeAssetName,
  versionFromAssetName,
} = await import('../src/mapPublishAsset.ts')

test('版本号只认数字和点', () => {
  assert.equal(isMapVersion('1'), true)
  assert.equal(isMapVersion('1.2.0'), true)
  assert.equal(isMapVersion(' 1.2.3 '), true)
  assert.equal(isMapVersion('v1.2.0'), false)
  assert.equal(isMapVersion('1.2.0-beta.1'), false)
  assert.equal(isMapVersion('1..2'), false)
  assert.equal(isMapVersion(''), false)
})

test('关卡集名按后端规则变成默认模板前缀', () => {
  assert.equal(levelSetSlug('GAODA'), 'gaoda')
  assert.equal(levelSetSlug('Map 2'), 'map-2')
  assert.equal(levelSetSlug('大厨'), '')
  assert.equal(defaultTemplate('GAODA'), `gaoda-${ASSET_VERSION_PLACEHOLDER}.zip`)
})

test('模板 + 版本号渲染出真正会上传的附件名', () => {
  assert.equal(renderAssetName('gaoda-${version}.zip', '', '1.2.0'), 'gaoda-1.2.0.zip')
  // 模板留空：与后端一样按关卡集名生成
  assert.equal(renderAssetName('', 'GAODA', '1.2.0'), 'gaoda-1.2.0.zip')
  // 非法版本号 / 模板里没有占位符：返回空串，让上传前拦下来
  assert.equal(renderAssetName('gaoda-${version}.zip', '', 'v1.2.0'), '')
  assert.equal(renderAssetName('gaoda.zip', 'GAODA', '1.2.0'), '')
  // 中文与空格在附件名里必须变成 -
  assert.equal(renderAssetName('Gaoda ${version} 合集.zip', '', '1.2.0'), 'Gaoda-1.2.0-.zip')
  // 纯中文关卡集名的默认模板会剩下前导 -，与后端逐字一致，不能被「清理」掉
  assert.equal(renderAssetName('', '大厨', '1.2.0'), '-1.2.0.zip')
})

test('附件名只保留 GitHub 查询串放心的字符', () => {
  assert.equal(safeAssetName('gaoda-1.2.0.zip'), 'gaoda-1.2.0.zip')
  assert.equal(safeAssetName('.hidden'), 'hidden')
  assert.equal(safeAssetName('a  b.zip'), 'a-b.zip')
})

test('从附件名反解版本号（与后端同名规则）', () => {
  assert.equal(versionFromAssetName('gaoda-1.2.0.zip'), '1.2.0')
  assert.equal(versionFromAssetName('Gaoda ${version} 合集-0.9.zip'), '0.9')
  assert.equal(versionFromAssetName('map.zip'), '')
  assert.equal(versionFromAssetName('v2'), '2')
  assert.equal(versionFromAssetName('2026-11-04.zip'), '04')
  // 版本号本身不能被当成扩展名剥掉
  assert.equal(versionFromAssetName('1.2.0'), '1.2.0')
})

test('版本比较与取最新', () => {
  assert.ok(compareMapVersions('1.10.0', '1.9.9') > 0)
  assert.equal(compareMapVersions('1.2', '1.2.0'), 0)
  assert.equal(latestVersionOf(['gaoda-1.2.0.zip', 'gaoda-1.10.0.zip', 'map.zip']), '1.10.0')
  assert.equal(latestVersionOf(['map.zip']), '')
  assert.equal(latestVersionOf([]), '')
})
