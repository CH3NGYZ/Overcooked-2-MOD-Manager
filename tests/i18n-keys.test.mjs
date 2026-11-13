import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { sourceFiles } from './helpers/source-scan.mjs'
import { readLocaleTables } from './helpers/locale-tables.mjs'
import { i18n } from '../src/locale/index.ts'
import { LOCALE_CODES } from '../src/locale/types.ts'
import test from 'node:test'

/**
 * 所有 `t('…')` 的字面量 key 都必须登记在语言表里。
 *
 * 真实事故：安装来源弹窗（现行实现是 `InstallSourceCard`）里写「…下载并**解析**」，
 * 语言表里是「…下载并**安装**」。key 拼错不会报错、不会崩，界面上就是中文原文，
 * 切到英/日/韩也不变 —— 只能靠扫描发现。修复时一次扫出 11 条同类遗漏。
 *
 * 通用扫描覆盖字面量调用；共享安装卡片另查动态动作与状态表中的字面量 key，
 * 防止把不存在的 key 当成合法测试预期。模板拼接的 key 仍需领域测试覆盖。
 *
 * ## 表在哪里
 *
 * 直接消费 `src/locale/catalog.ts` 的运行时登记表；子域聚合、对象展开
 * 与 flat 表遵守应用本身的顺序，不按文件名或源码正则推断域。
 */
const SRC = new URL('../src/', import.meta.url)

/** 无需翻译的 key：占位符与产品名。 */
const IGNORED = new Set([
  // 输入框 placeholder 里的 URL 示例：URL 本身不本地化
  'https://example.com/mod.zip',
  // CodeWeavers 的商标
  'CrossOver',
])

const declared = new Set(readLocaleTables().registered.keys())

test('语言表至少包含项目现有文案量', () => {
  // 防止登记入口失效导致 declared 为空、测试假通过
  assert.ok(declared.size > 700, `语言表只解析出 ${declared.size} 条，扫描可能失效`)
})

test('t() 的字面量 key 全部已登记', () => {
  const missing = []
  for (const file of sourceFiles(fileURLToPath(SRC), ['.vue', '.ts'])) {
    // 语言表自己的文件不参与扫描
    if (/[\\/]locale[\\/]/.test(file)) continue

    const text = readFileSync(file, 'utf8').replace(/\r\n/g, '\n')
    for (const [index, line] of text.split('\n').entries()) {
      // 跳过注释行：注释里常引用示例 tag（`t('mods.xxx')`）当说明，不是真调用
      const trimmed = line.trimStart()
      if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) continue

      const patterns = [
        /\bt\(\s*'((?:[^'\\]|\\.)+)'/g,
        /\bt\(\s*"((?:[^"\\]|\\.)+)"/g,
      ]
      for (const pattern of patterns) {
        for (const match of line.matchAll(pattern)) {
          const key = match[1].replace(/\\'/g, "'").replace(/\\"/g, '"')
          if (IGNORED.has(key) || declared.has(key)) continue
          missing.push(`${key}  (${file.replace(/.*[\\/]src[\\/]/, 'src/')}:${index + 1})`)
        }
      }
    }
  }
  assert.deepEqual(missing, [], `以下文案未登记到语言表：\n  ${missing.join('\n  ')}`)
})

test('地图与 MOD 安装卡片的动态动作、状态文案在五语下都存在', () => {
  const files = [
    'src/mapCardStatus.ts', 'src/levelSetStatus.ts', 'src/statusActions.ts', 'src/statusTags.ts',
    'src/views/ModsOnlineView.vue', 'src/components/ReleaseDetailPanel.vue',
  ]
  const keys = new Set()
  for (const file of files) {
    const text = readFileSync(new URL(`../${file}`, import.meta.url), 'utf8')
    for (const match of text.matchAll(/['"]((?:common|overview|modsOnline|modsLocal|tools|dialogs|levelSet|mapVersion)\.[\w.]+)['"]/g)) {
      keys.add(match[1])
    }
  }
  assert.ok(keys.size > 30, '应覆盖两域动作与状态映射，而非仅直接 t() 调用')
  for (const key of keys) {
    assert.ok(declared.has(key), `动作或状态文案未登记：${key}`)
    for (const locale of LOCALE_CODES) {
      assert.ok(i18n.global.te(key, locale), `${locale} 缺少 ${key}`)
      assert.notEqual(i18n.global.t(key, {}, { locale }), key, `${locale} 不应显示裸 ${key}`)
    }
  }
})

test('旧安装动作 key 与 common.action 复用同一份五语文案', () => {
  const { registered } = readLocaleTables()
  for (const [legacy, shared] of [
    ['modsOnline.install', 'common.action.install'], ['overview.reinstall', 'common.action.reinstall'],
    ['dialogs.update', 'common.action.update'], ['modsOnline.refresh', 'common.action.refresh'],
  ]) assert.equal(registered.get(legacy), registered.get(shared), `${legacy} 应复用 ${shared}`)
})
