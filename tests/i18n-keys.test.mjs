import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
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
 * 文案全部在 `src/locale/*.ts`，每张表声明自己属于哪个域（`index.ts` 的
 * `TABLES`）。所以这里读 `index.ts` 建立「导出名 → 域」，再读各域文件
 * 的 key，拼出完整 tag 集合。
 */
const SRC = new URL('../src/', import.meta.url)
const LOCALE_DIR = new URL('../src/locale/', import.meta.url)
const NON_TABLE = new Set(['index.ts', 'backend.ts', 'state.ts', 'types.ts'])

/** 无需翻译的 key：占位符与产品名。 */
const IGNORED = new Set([
  // 输入框 placeholder 里的 URL 示例：URL 本身不本地化
  'https://example.com/mod.zip',
  // CodeWeavers 的商标
  'CrossOver',
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

const localePath = LOCALE_DIR.pathname.replace(/^\/([A-Za-z]:)/, '$1')

/** 从 `index.ts` 读「导出名 → 域」。`flat` 表记为 null（key 自带完整 tag）。 */
const domainOf = new Map()
for (const m of readFileSync(join(localePath, 'index.ts'), 'utf8').matchAll(
  /\{\s*(?:domain:\s*'([A-Za-z.]+)'|flat:\s*true),\s*messages:\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}/g,
)) {
  domainOf.set(m[2], m[1] ?? null)
}

/** 读出所有已登记的完整 tag。 */
const declared = new Set()
for (const file of readdirSync(localePath)) {
  if (!file.endsWith('.ts') || NON_TABLE.has(file)) continue
  const text = readFileSync(join(localePath, file), 'utf8').replace(/\r\n/g, '\n')

  // 该文件导出了哪些表（一个文件可能导出多张）
  const exports = [...text.matchAll(/export const ([A-Za-z_][A-Za-z0-9_]*):\s*DomainMessages/g)].map((m) => m[1])

  // 按 `export const` 切块，逐块收集 key
  const blocks = text.split(/export const [A-Za-z_][A-Za-z0-9_]*:\s*DomainMessages\s*=\s*\{/)
  for (let i = 0; i < exports.length; i += 1) {
    const block = blocks[i + 1] ?? ''
    const domain = domainOf.has(exports[i]) ? domainOf.get(exports[i]) : null
    const prefix = domain === null ? '' : `${domain}.`
    // key 有带引号（`'a.b': {`）和裸标识符（`abc: {`）两种写法
    for (const m of block.matchAll(/^\s*(?:'([A-Za-z0-9_.]+)'|([A-Za-z_][A-Za-z0-9_]*)):\s*\{/gm)) {
      declared.add(`${prefix}${m[1] ?? m[2]}`)
    }
  }
}

test('语言表至少包含项目现有文案量', () => {
  // 防止解析正则失效导致 declared 为空、测试假通过
  assert.ok(declared.size > 700, `语言表只解析出 ${declared.size} 条，扫描可能失效`)
})

test('t() 的字面量 key 全部已登记', () => {
  const missing = []
  for (const file of walk(SRC.pathname.replace(/^\/([A-Za-z]:)/, '$1'))) {
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
