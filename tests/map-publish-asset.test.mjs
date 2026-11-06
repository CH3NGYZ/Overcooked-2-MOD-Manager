import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 发布页附件名与版本号文本（`src/mapPublishAsset.ts`）。
 *
 * 这条链路是「tag = 一张地图、版本号只在附件名里」能成立的前提：管理器用
 * `map_note::template_rule` 把块里的 `asset` 模板编译成整串锚定的正则，再从
 * 附件名里抽版本号。v2 的排序表把「资产版本」放宽成允许后缀的文本
 * （`3.4.1-fix`），所以这里同时锁两套形状：数字版本号（`isMapVersion`，给数字
 * 比较用）与版本号文本（`isVersionText`，给附件名、排序表用）。
 */
const {
  ASSET_VERSION_PLACEHOLDER,
  assetNamePattern,
  assetRuleFrom,
  compareMapVersions,
  composeAssetRule,
  defaultTemplate,
  isMapVersion,
  isVersionText,
  latestVersionOf,
  levelSetSlug,
  parseVersionedZipName,
  renderAssetName,
  safeAssetName,
  versionFromAssetName,
  versionFromRule,
} = await import('../src/mapPublishAsset.ts')

test('数字版本号只认数字和点', () => {
  assert.equal(isMapVersion('1'), true)
  assert.equal(isMapVersion('1.2.0'), true)
  assert.equal(isMapVersion(' 1.2.3 '), true)
  assert.equal(isMapVersion('v1.2.0'), false)
  assert.equal(isMapVersion('1.2.0-beta.1'), false)
  assert.equal(isMapVersion('1..2'), false)
  assert.equal(isMapVersion(''), false)
})

test('版本号文本允许后缀，但必须以数字开头', () => {
  assert.equal(isVersionText('1'), true)
  assert.equal(isVersionText(' 3.4.1 '), true)
  assert.equal(isVersionText('3.4.1-fix'), true)
  assert.equal(isVersionText('1.0.0+build.7'), true)
  assert.equal(isVersionText('0.69'), true)
  // 数字版本号一定是版本号文本的子集
  assert.equal(isVersionText('1.2.0'), true)
  assert.equal(isMapVersion('1.2.0-beta.1'), false)
  assert.equal(isVersionText('1.2.0-beta.1'), true)
  // 不以数字开头、带空格或路径分隔符 → 不是
  assert.equal(isVersionText('v1.2.0'), false)
  assert.equal(isVersionText(''), false)
  assert.equal(isVersionText('   '), false)
  assert.equal(isVersionText('-1.2.0'), false)
  assert.equal(isVersionText('1.2.0 beta'), false)
  assert.equal(isVersionText('1.2.0/2'), false)
})

test('关卡集名按后端规则变成默认模板前缀', () => {
  assert.equal(levelSetSlug('GAODA'), 'gaoda')
  assert.equal(levelSetSlug('Map 2'), 'map-2')
  assert.equal(levelSetSlug('大厨'), '')
  // v2：默认模板带 `_v`，与后端 save_publish_release 的默认值一致
  assert.equal(defaultTemplate('GAODA'), `gaoda_v${ASSET_VERSION_PLACEHOLDER}.zip`)
})

test('模板 + 版本号渲染出真正会上传的附件名', () => {
  assert.equal(renderAssetName('gaoda-${version}.zip', '', '1.2.0'), 'gaoda-1.2.0.zip')
  // 模板留空：与后端一样按关卡集名生成
  assert.equal(renderAssetName('', 'GAODA', '1.2.0'), 'gaoda_v1.2.0.zip')
  // 排序表里的带后缀版本号也能渲染
  assert.equal(renderAssetName('', 'GAODA', '3.4.1-fix'), 'gaoda_v3.4.1-fix.zip')
  // 非法版本号 / 模板里没有占位符：返回空串，让上传前拦下来
  assert.equal(renderAssetName('gaoda-${version}.zip', '', 'v1.2.0'), '')
  assert.equal(renderAssetName('gaoda.zip', 'GAODA', '1.2.0'), '')
  // 已有显式模板和后缀原样保留，与后端上传名一致。
  assert.equal(renderAssetName('Gaoda ${version} 合集.zip', '', '1.2.0'), 'Gaoda 1.2.0 合集.zip')
  assert.equal(renderAssetName('Map_v${version}.zip', '', '1.0+build'), 'Map_v1.0+build.zip')
  assert.equal(renderAssetName('../${version}.zip', '', '1'), '')
  assert.equal(renderAssetName('dir/${version}.zip', '', '1'), '')
  assert.equal(renderAssetName('a'.repeat(201) + '${version}.zip', '', '1'), '')
  // 纯中文关卡集名的默认模板里 slug 为空，只剩 `_v`，与后端逐字一致，不能被「清理」掉
  assert.equal(renderAssetName('', '大厨', '1.2.0'), '_v1.2.0.zip')
})

test('从 xxx_v<version>.zip 拆出 tag 与版本号', () => {
  assert.deepEqual(parseVersionedZipName('dmm_v3.4.1.zip'), { tag: 'dmm', version: '3.4.1' })
  assert.deepEqual(parseVersionedZipName('dmm_v3.4.1-fix.ZIP'), {
    tag: 'dmm',
    version: '3.4.1-fix',
  })
  // 带目录（POSIX / Windows）只看文件名本身
  assert.deepEqual(parseVersionedZipName('maps/dmm_v3.4.1.zip'), { tag: 'dmm', version: '3.4.1' })
  assert.deepEqual(parseVersionedZipName('E:\\maps\\dmm_v0.7.zip'), { tag: 'dmm', version: '0.7' })
  // tag 去首尾空白与 - .
  assert.deepEqual(parseVersionedZipName('  -dmm.  _v0.7.zip'), { tag: 'dmm', version: '0.7' })
  // 不是这个形状 → null
  assert.equal(parseVersionedZipName('map.zip'), null)
  assert.equal(parseVersionedZipName('dmm-3.4.1.zip'), null)
  assert.equal(parseVersionedZipName('dmm_v.zip'), null)
  assert.equal(parseVersionedZipName('dmm_vX.zip'), null)
  assert.equal(parseVersionedZipName('dmm_v3.4.1.7z'), null)
  assert.equal(parseVersionedZipName('._v1.0.zip'), null)
  assert.equal(parseVersionedZipName(''), null)
})

test('新地图的资产规则由 tag 组合，抽版本号按整串匹配', () => {
  assert.equal(composeAssetRule('dmm'), `dmm_v${ASSET_VERSION_PLACEHOLDER}.zip`)
  assert.equal(composeAssetRule(' Gaoda Map '), `Gaoda Map_v${ASSET_VERSION_PLACEHOLDER}.zip`)

  const rule = composeAssetRule('dmm')
  assert.equal(versionFromRule(rule, 'dmm_v3.4.1.zip'), '3.4.1')
  assert.equal(versionFromRule(rule, 'dmm_v3.4.1-fix.zip'), '3.4.1-fix')
  assert.equal(versionFromRule(rule, 'dmm_v.zip'), null)
  assert.equal(versionFromRule(rule, 'dmm-3.4.1.zip'), null)
  assert.equal(versionFromRule(rule, 'other_v1.0.0.zip'), null)
  // 模板里的正则元字符按字面匹配，不能被当成语法
  const literal = 'gaoda(1)_v${version}.zip'
  assert.equal(versionFromRule(literal, 'gaoda(1)_v2.0.zip'), '2.0')
  assert.equal(versionFromRule(literal, 'gaoda1_v2.0.zip'), null)
  // 模板没有占位符 → 抽不出版本，不能当规则用
  assert.equal(assetNamePattern('dmm.zip'), null)
  assert.equal(versionFromRule('dmm.zip', 'dmm.zip'), null)
})

test('模板里的空格与中文按字面匹配，版本号照样抽得出', () => {
  const rule = 'Gaoda ${version} 合集.zip'
  const pattern = assetNamePattern(rule)
  assert.ok(pattern instanceof RegExp)
  assert.equal(pattern.test('Gaoda 1.2.0 合集.zip'), true)
  assert.equal(pattern.test('Gaoda 合集.zip'), false)
  assert.equal(versionFromRule(rule, 'Gaoda 3.4.1-fix 合集.zip'), '3.4.1-fix')
  // 多段版本号要整段吃掉，不能只留最后一段
  assert.equal(versionFromRule('daxue_v${version}.zip', 'daxue_v0.3.2.zip'), '0.3.2')
})

test('已有地图的模板：有占位符原样用，否则从附件名反推', () => {
  assert.equal(assetRuleFrom('dmm_v${version}.zip', ['dmm_v1.0.0.zip']), 'dmm_v${version}.zip')
  assert.equal(assetRuleFrom('  dmm_v${version}.zip  ', []), 'dmm_v${version}.zip')
  // 老地图没写模板：拿第一个 zip 附件、把第一段数字版本号换成占位符
  assert.equal(assetRuleFrom('', ['dmm_v3.4.1.zip']), 'dmm_v${version}.zip')
  assert.equal(assetRuleFrom('', ['readme.txt', 'dmm_v3.4.1.zip']), 'dmm_v${version}.zip')
  assert.equal(assetRuleFrom('', ['Gaoda 1.2.0 合集.zip']), 'Gaoda ${version} 合集.zip')
  // 只认 .zip；反推出来的规则能原样匹配回那个文件
  assert.equal(assetRuleFrom('', ['dmm_v1.0.7z', 'dmm_v2.0.zip']), 'dmm_v${version}.zip')
  assert.equal(
    versionFromRule(assetRuleFrom('', ['Gaoda 1.2.0 合集.zip']), 'Gaoda 1.2.0 合集.zip'),
    '1.2.0',
  )
  // 推不出：没有 zip、zip 里没有数字版本号
  assert.equal(assetRuleFrom('', ['map.zip']), '')
  assert.equal(assetRuleFrom('', []), '')
  assert.equal(assetRuleFrom('', ['dmm_v1.0.7z']), '')
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
