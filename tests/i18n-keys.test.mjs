import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { sourceFiles } from './helpers/source-scan.mjs'
import { readLocaleTables } from './helpers/locale-tables.mjs'
import test from 'node:test'

/**
 * 所有 `t('…')` 的字面量 key 都必须登记在语言表里。
 *
 * 真实事故：安装来源弹窗（现行实现是 `InstallSourceCard`）里写「…下载并**解析**」，
 * 语言表里是「…下载并**安装**」。key 拼错不会报错、不会崩，界面上就是中文原文，
 * 切到英/日/韩也不变 —— 只能靠扫描发现。修复时一次扫出 11 条同类遗漏。
 *
 * 只覆盖字面量调用。动态 key（变量、模板拼接）无法静态判定，那部分仍需
 * 人工检查；本测试的存在是为了让「新增文案忘了登记」变成构建期红灯。
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
