/** 直接读取运行时语言表登记；递归审计源文件，防止拆分后的表静默漏装。 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { ALL_ENTRIES, TEMPLATE_PARAMS } from '../scripts/backend-tag-map.mjs'
import { TABLES, buildMessages } from '../src/locale/catalog.ts'
import { LOCALE_CODES } from '../src/locale/types.ts'
import { auditLocaleSources, placeholders, readLocaleTables } from './helpers/locale-tables.mjs'

const { registered: REGISTERED, errors, duplicates } = readLocaleTables()

// 历史 flat/domain 兼容重叠。固定现有集合和次数，不允许拆分新增覆盖；本批不改译文。
const LEGACY_OVERLAPS = Object.fromEntries([
  'archive.invalidZipName', 'archive.noInstallableContent', 'archive.pathEscape',
  'archive.targetExists', 'archive.unsafePath', 'backup.noReclaimableFiles',
  'backup.notFound', 'backup.outsideRoot', 'backup.overlap', 'backup.recycleFailed',
  'backup.sameDirectory', 'backup.unavailable', 'backup.wrongKind',
  'download.cancelled', 'download.empty', 'download.incomplete', 'game.notFound',
  'game.running', 'game.unsupportedPlatform', 'mods.dllNotFound',
  'mods.dllNotInPackage', 'mods.notInPlugins', 'mods.unknownFiles',
  'recycle.partialFailure', 'source.addressInvalid', 'source.builtinFileMissing',
  'source.proxyAddressInvalid', 'source.tomlInvalid',
].map(tag => [tag, 2]))

test('能读出域声明与表', () => {
  assert.ok(TABLES.length >= 20)
  assert.ok(REGISTERED.size > 400, `只拼出 ${REGISTERED.size} 个 tag`)
})

test('后端 tag 全部在语言表登记', () => {
  const missing = [...new Set(Object.values(ALL_ENTRIES))].filter(tag => !REGISTERED.has(tag))
  assert.deepEqual(missing, [], `以下后端 tag 没有语言表条目：\n${missing.join('\n')}`)
})

test('每条文案五种语言齐全、非空且占位符一致', () => {
  assert.deepEqual(errors, [])
})

test('模板 tag 的占位符与后端参数名一致', () => {
  for (const [tag, paramNames] of Object.entries(TEMPLATE_PARAMS)) {
    const entry = REGISTERED.get(tag)
    assert.ok(entry, `语言表没有 ${tag}`)
    for (const code of LOCALE_CODES) {
      for (const name of paramNames) assert.ok(placeholders(entry[code]).includes(name), `${tag}/${code}: 缺 {${name}}`)
    }
  }
})

test('语言表里没有遗留的中文 key', () => {
  assert.deepEqual([...REGISTERED.keys()].filter(tag => /[\u4e00-\u9fff]/.test(tag)), [])
})

test('所有子目录的语言表都被登记且对象内无重复字段', async () => {
  const audit = await auditLocaleSources('src/locale')
  assert.ok(audit.declarations >= 50, `只发现 ${audit.declarations} 张表`)
  assert.deepEqual(audit.errors, [])
})

test('拆分不得增加历史兼容表之外的重复 key', () => {
  assert.deepEqual(duplicates, LEGACY_OVERLAPS)
})

test('测试消费的完整 tag 与运行时五语嵌套树一致', () => {
  for (const code of LOCALE_CODES) {
    const messages = buildMessages(code)
    const shadowed = []
    for (const [tag, entry] of REGISTERED) {
      const actual = tag.split('.').reduce((value, key) => value?.[key], messages)
      if (actual === undefined) shadowed.push(tag)
      else assert.equal(actual, entry[code], `${tag}/${code}`)
    }
    // 旧地图抽屉的子键被 frameworkUi.mapPacks 覆盖；本批保持运行时行为并禁止新增遮蔽。
    assert.deepEqual(shadowed.sort(), [
      'framework.mapPacks.count', 'framework.mapPacks.empty',
      'framework.mapPacks.openDir', 'framework.mapPacks.title',
    ])
  }
})
