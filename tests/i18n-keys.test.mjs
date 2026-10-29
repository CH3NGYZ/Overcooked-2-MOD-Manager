import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'

/**
 * 所有 `t('…')` 的字面量 key 都必须登记在语言表里。
 *
 * 真实事故：`ModInstallSourceDialog` 里写「…下载并**解析**」，语言表里是
 * 「…下载并**安装**」。key 拼错不会报错、不会崩，界面上就是中文原文，
 * 切到英/日/韩也不变 —— 只能靠扫描发现。修复时一次扫出 11 条同类遗漏。
 *
 * 只覆盖字面量调用。动态 key（变量、模板拼接）无法静态判定，那部分仍需
 * 人工检查；本测试的存在是为了让「新增文案忘了登记」变成构建期红灯。
 */
const SRC = new URL('../src/', import.meta.url)
const I18N = new URL('../src/i18n.ts', import.meta.url)

/** 无需翻译的 key：占位符与产品名。 */
const IGNORED = new Set([
  // 输入框 placeholder 里的 URL 示例：URL 本身不本地化
  'https://example.com/mod.zip',
  // CodeWeavers 的商标
  'CrossOver',
  // 管理器自身的产品名，各语言保持同一写法
  'OC2 MOD 管理器',
])

function walk(dir, files = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) {
      walk(full, files)
    } else if (/\.(vue|ts)$/.test(name)) {
      files.push(full)
    }
  }
  return files
}

const i18nSource = readFileSync(I18N, 'utf8').replace(/\r\n/g, '\n')
const declared = new Set()
for (const match of i18nSource.matchAll(/^\s*'((?:[^'\\]|\\.)+)'\s*:\s*\[/gm)) {
  declared.add(match[1].replace(/\\'/g, "'"))
}

test('语言表至少包含项目现有文案量', () => {
  // 防止解析正则失效导致 declared 为空、测试假通过
  assert.ok(declared.size > 400, `语言表只解析出 ${declared.size} 条，扫描可能失效`)
})

test('t() 的字面量 key 全部已登记', () => {
  const missing = []
  for (const file of walk(SRC.pathname.replace(/^\/([A-Za-z]:)/, '$1'))) {
    if (file.endsWith('i18n.ts')) continue
    /*
     * 跳过 `src/locale/`：那是正在迁移过来的新系统（语义化 tag + vue-i18n），
     * 它的 key 存在自己的域文件里，由 tests/backend-i18n.test.mjs 覆盖。
     * 旧系统只负责尚未迁移的页面，两者的 key 空间互不相干。
     */
    if (/[\\/]locale[\\/]/.test(file)) continue

    const text = readFileSync(file, 'utf8').replace(/\r\n/g, '\n')
    for (const [index, line] of text.split('\n').entries()) {
      // 跳过注释行：注释里常引用示例 tag（`t('error.xxx')`）当说明，不是真调用
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
  assert.deepEqual(missing, [], `以下文案未登记到 i18n.ts：\n  ${missing.join('\n  ')}`)
})
