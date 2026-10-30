/**
 * 后端 tag 覆盖测试。
 *
 * 目标：`scripts/backend-tag-map.mjs` 里登记的**每一个** tag，都必须在
 * `src/locale/` 的语言表里出现，且五种语言齐全、占位符与后端参数名一致。
 *
 * ## 为什么值得一个测试
 *
 * 迁移是脚本批量做的，人和脚本都会漏：漏一个 tag 时后端照常返回，前端
 * `t()` 找不到 key 就把 tag 原样吐给用户（`mods.higherVersionPresent`），
 * 而且只在特定失败分支才出现 —— 手工点不到。这类缺陷只能靠静态核对发现。
 *
 * ## 参数名为什么也要核对
 *
 * 后端 `with_params` 传的参数名（`TEMPLATE_PARAMS`）必须与文案里的
 * `{name}` 完全一致。名字对不上时 vue-i18n 不报错，只是把 `{name}` 原样
 * 留在句子里 —— 比缺翻译更难发现。
 *
 * ## 怎么知道一张表属于哪个域
 *
 * 表里的 key 是**短名**（`higherVersionPresent`），域由 `index.ts` 的
 * `TABLES` 声明。所以这里也读 `index.ts`，从 `{ domain: 'x', messages: y }`
 * 建立 `y → x` 的映射，再据此还原完整 tag。
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { ALL_ENTRIES, TEMPLATE_PARAMS } from '../scripts/backend-tag-map.mjs'

const LOCALE_DIR = 'src/locale'
const LOCALE_CODES = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']
/** 只声明类型/状态的文件，不含文案。 */
const NON_TABLE_FILES = new Set(['index.ts', 'backend.ts', 'state.ts', 'types.ts'])

/**
 * 从 `index.ts` 读出「导出名 → 域」。
 *
 * 两种声明：
 * - `{ domain: 'mods', messages: mods }` —— 短 key，域来自这里；
 * - `{ flat: true, messages: error }` —— key 自带完整 tag，不加前缀。
 *
 * `flat` 记为 `null`。
 */
function readDomainMap() {
  const text = readFileSync(join(LOCALE_DIR, 'index.ts'), 'utf8')
  const map = new Map()
  const re = /\{\s*(?:domain:\s*'([A-Za-z.]+)'|flat:\s*true),\s*messages:\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}/g
  let match
  while ((match = re.exec(text)) !== null) {
    const [, domain, name] = match
    map.set(name, domain ?? null)
  }
  return map
}

/** 逐个文件读出 `导出名 → { key → 五语 }`。 */
function readExportedTables() {
  const tables = new Map()
  for (const file of readdirSync(LOCALE_DIR).filter((name) => name.endsWith('.ts'))) {
    if (NON_TABLE_FILES.has(file)) continue
    const lines = readFileSync(join(LOCALE_DIR, file), 'utf8').split('\n')
    let currentName = null
    let index = 0
    while (index < lines.length) {
      if (currentName === null) {
        const decl = /^export const ([A-Za-z_][A-Za-z0-9_]*):\s*DomainMessages\s*=\s*\{\s*$/.exec(lines[index])
        if (decl) {
          currentName = decl[1]
          tables.set(currentName, new Map())
        }
        index += 1
        continue
      }
      if (/^\}\s*$/.test(lines[index])) {
        currentName = null
        index += 1
        continue
      }
      // key 有两种写法：带引号 `'a.b': {` 和裸标识符 `abc: {`，都要接受。
      const head = /^\s*(?:'([A-Za-z0-9_.]+)'|([A-Za-z_][A-Za-z0-9_]*)):\s*\{\s*$/.exec(lines[index])
      if (!head) { index += 1; continue }
      const key = head[1] ?? head[2]
      const body = []
      let depth = 0
      let cursor = index
      for (; cursor < lines.length; cursor += 1) {
        for (const ch of lines[cursor]) {
          if (ch === '{') depth += 1
          else if (ch === '}') depth -= 1
        }
        body.push(lines[cursor])
        if (depth <= 0) break
      }
      tables.get(currentName).set(key, parseEntry(body.join('\n')))
      index = cursor + 1
    }
  }
  return tables
}

/** 从一条条目里抓出五种语言的文案。 */
function parseEntry(body) {
  const entry = {}
  for (const code of LOCALE_CODES) {
    const pattern = code.includes('-')
      ? new RegExp(`'${code}':\\s*(['"\`])((?:\\\\.|(?!\\1).)*)\\1`)
      : new RegExp(`\\b${code}:\\s*(['"\`])((?:\\\\.|(?!\\1).)*)\\1`)
    const value = pattern.exec(body)
    if (value) entry[code] = value[2]
  }
  return entry
}

const DOMAIN_OF = readDomainMap()
const EXPORTED = readExportedTables()

/** 拼出「完整 tag → 五语」。 */
const REGISTERED = new Map()
for (const [name, table] of EXPORTED) {
  // 注意用 has() 而不是真假判断：flat 表的域是 `null`，会被 `!domain` 误判成未声明。
  if (!DOMAIN_OF.has(name)) continue
  const domain = DOMAIN_OF.get(name)
  const prefix = domain === null ? '' : `${domain}.`
  for (const [key, entry] of table) REGISTERED.set(`${prefix}${key}`, entry)
}

test('能读出域声明与表', () => {
  assert.ok(DOMAIN_OF.size >= 20, `只读到 ${DOMAIN_OF.size} 条域声明`)
  assert.ok(REGISTERED.size > 400, `只拼出 ${REGISTERED.size} 个 tag`)
})

test('后端 tag 全部在语言表登记', () => {
  const missing = []
  for (const tag of new Set(Object.values(ALL_ENTRIES))) {
    if (!REGISTERED.has(tag)) missing.push(tag)
  }
  assert.deepEqual(missing, [], `以下后端 tag 没有语言表条目：\n${missing.join('\n')}`)
})

test('每条文案五种语言齐全且非空', () => {
  const incomplete = []
  for (const [tag, entry] of REGISTERED) {
    const missingCodes = LOCALE_CODES.filter((code) => !entry[code] || entry[code].trim() === '')
    if (missingCodes.length) incomplete.push(`${tag}: 缺 ${missingCodes.join(', ')}`)
  }
  assert.deepEqual(incomplete, [], `以下条目缺少语言：\n${incomplete.join('\n')}`)
})

test('模板 tag 的占位符与后端参数名一致', () => {
  const mismatched = []
  for (const [tag, paramNames] of Object.entries(TEMPLATE_PARAMS)) {
    const entry = REGISTERED.get(tag)
    if (!entry) {
      mismatched.push(`${tag}: 语言表里没有这条`)
      continue
    }
    const placeholders = new Set(
      [...(entry['zh-CN'].matchAll(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g))].map((m) => m[1]),
    )
    for (const name of paramNames) {
      if (!placeholders.has(name)) mismatched.push(`${tag}: 文案里缺少 {${name}}`)
    }
  }
  assert.deepEqual(mismatched, [], `占位符不一致：\n${mismatched.join('\n')}`)
})

test('语言表里没有遗留的中文 key', () => {
  const chineseKey = [...REGISTERED.keys()].filter((tag) => /[\u4e00-\u9fff]/.test(tag))
  assert.deepEqual(chineseKey, [], `以下 key 仍是中文原文：\n${chineseKey.join('\n')}`)
})

test('index.ts 里每张表都声明了域', () => {
  const undeclared = [...EXPORTED.keys()].filter((name) => !DOMAIN_OF.has(name))
  assert.deepEqual(undeclared, [], `以下表没有在 TABLES 里声明域：\n${undeclared.join('\n')}`)
})
