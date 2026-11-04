/**
 * 后端 tag 字面量覆盖测试。
 *
 * 目标：`src-tauri/src/*.rs` 里出现的每一个 tag 形态的字符串字面量
 * （`<域>.<原因>`），都必须在 `src/locale/` 的语言表里登记。
 *
 * ## 为什么不能只核对一份手工清单
 *
 * 仓库里原本有 `backend-tag-coverage.test.mjs`，它核对的是
 * `scripts/backend-tag-map.mjs` —— 一份**迁移时**人工整理的表。清单之外新写的
 * tag 永远不会被检查到：`assetBundle.*` 就是这样一路漏到线上的，界面里出现过
 * `assetBundle.corrupt:lzma: lzma data error` 这种原始 tag。
 *
 * 所以这里改成**直接扫源码**：新增 tag 时不需要谁记得更新清单，测试自己会发现。
 *
 * ## 白名单为什么必要
 *
 * Rust 源码里的点号字符串不全是 tag：GUID（`dev.gua.overcooked.diylevel`）、
 * 测试固件（`a.b`、`release.example`）、文件名（`Thumbs.db`）、地区值
 * （`common.other`）、域名（`oc2gh.ch3ng.top`）都会命中「`x.y` 形态」。
 * 这些必须在白名单里**显式**排除 —— 显式的代价是加一行，收益是新 tag 漏登记
 * 一定会让 CI 变红，而不是等用户点到那个失败分支才发现。
 *
 * ## 已知豁免
 *
 * `error.<域>.<原因>` 是早期形态，前端 `translateBackendMessage()` 会去掉
 * `error.` 前缀再查一次表，所以它们不必以完整形式登记。
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const LOCALE_DIR = 'src/locale'
const RUST_DIR = 'src-tauri/src'

/** 读文件并统一换行：部分 locale 文件是 CRLF，行尾 `\r` 会让表头正则失配。 */
function readText(path) {
  return readFileSync(path, 'utf8').replace(/\r\n/g, '\n')
}

/**
 * 从语言表里读出「域 → 短 key 集合」与「flat 表的完整 key 集合」。
 *
 * 表的位置不能按文件名推：一个文件可能包含多个表
 * （`installPipeline.ts` 里就有 `archive` / `download` / `direct`），
 * 所以按 `export const X: DomainMessages = {` 定位，再用 `index.ts` 里的
 * `{ domain, messages: X }` 反查域。
 */
function readLocaleTables() {
  const index = readText(join(LOCALE_DIR, 'index.ts'))
  const domainOf = new Map()
  const re = /\{\s*(?:domain:\s*'([A-Za-z.]+)'|flat:\s*true),\s*messages:\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}/g
  let match
  while ((match = re.exec(index)) !== null) domainOf.set(match[2], match[1] ?? null)

  const byDomain = new Map()
  const flat = new Set()
  for (const file of readdirSync(LOCALE_DIR)) {
    if (!file.endsWith('.ts')) continue
    const lines = readText(join(LOCALE_DIR, file)).split('\n')
    for (let i = 0; i < lines.length; i += 1) {
      const head = lines[i].match(/^export const ([A-Za-z_][A-Za-z0-9_]*): DomainMessages = \{$/)
      if (!head || !domainOf.has(head[1])) continue
      const domain = domainOf.get(head[1])
      const keys = []
      for (let j = i + 1; j < lines.length && lines[j] !== '}'; j += 1) {
        const key = lines[j].match(/^ {2}'?([A-Za-z_][A-Za-z0-9_.]*)'?:\s*\{/)
        if (key) keys.push(key[1])
      }
      if (domain === null) {
        for (const key of keys) flat.add(key)
      } else {
        if (!byDomain.has(domain)) byDomain.set(domain, new Set())
        for (const key of keys) byDomain.get(domain).add(key)
      }
    }
  }
  return { byDomain, flat }
}

/**
 * 明确不是 tag 的字面量。
 *
 * 每一条都写清为什么，避免以后有人为了「让测试变绿」往里塞真正的 tag。
 */
const NON_TAG = new Set([
  // 地图源里表示「全部地区」的取值
  'common.other',
  // DIYLevel 框架的稳定 GUID（与仓库源、安装清单共用同一标识）
  'dev.gua.overcooked.diylevel',
  // 同上 GUID 的短写，只在地图发布模块的单测固件里出现
  'dev.gua.diylevel',
  // 社区搜索/测试固件里的假 GUID 与假文件名
  'a.b', 'another.plugin', 'any.guid', 'base.guid', 'conflict.guid', 'dep.guid',
  'high.guid', 'legacy.guid', 'mid.guid', 'modern.guid', 'narrow.guid',
  'new.guid', 'old.guid', 'ranged.guid', 'unknown.guid', 'wide.guid',
  'com.bepis.bepinex.configurationmanager', 'com.valvesoftware.steam',
  'com.ch3ngyz.plugin.HostUtilities', 'com.ch3ngyz.plugin.DIYLevel',
  'fixture.configurationmanager',
  // 测试用的示例文件名
  'bundled.example', 'disabled.example', 'fast.example', 'first.example',
  'invalid.example', 'raw.example', 'release.example', 'remote.example',
  'second.example', 'selected.example', 'slow.example', 'halfwritten.json.tmp',
  'dll.disabled',
  // 拍平测试里的随包资源文件名（作者真的这么命名，不是 tag）
  'shared.assets',
  // 地图源测试里的示例值
  'oc2.pre', 'oc2.legacy', 'oc2.grouped', 'oc2.oc2diylevelruntimewloader',
  // 社区源的域名
  'oc2gh.ch3ng.top',
  // BepInEx 的 WinHTTP 开关用的是「禁用后缀」文件名，不是 tag
  'winhttp.dll.disabled', 'winhttp.dll.oc2mod.disabled',
  // 只在测试固件里出现的 last_error 取值（真实代码不产出这个 tag）
  'release.rateLimited',
  // 回归断言里的源码片段：确认这个已废弃的 tag 不再出现在代码里
  'config.game.path',
])

/** 早期形态：前端会去掉 `error.` 前缀重查一次，不必以完整形式登记。 */
function isLegacyErrorTag(tag) {
  return tag.startsWith('error.')
}

/**
 * 测试固件的命名空间：`com.example.*`。
 *
 * Rust 单测里造 BepInPlugin 元数据要写 GUID，逐条登记的话每加一个测试就得同步
 * 改白名单，加漏了 CI 变红却跟真正的 tag 无关。命名空间整体豁免，同时把它写在
 * 这里 —— 真实 tag 不可能落在 `com.example` 这个域下，豁免不会被误用。
 */
function isFixtureIdentifier(tag) {
  return tag.startsWith('com.example.')
}

/** 收集 Rust 源码里所有 tag 形态的字面量。 */
function collectRustTags() {
  const found = new Map()
  for (const file of readdirSync(RUST_DIR)) {
    if (!file.endsWith('.rs')) continue
    readText(join(RUST_DIR, file)).split('\n').forEach((line, index) => {
      for (const match of line.matchAll(/"([A-Za-z][A-Za-z0-9]*(?:\.[A-Za-z][A-Za-z0-9]*)+)(\?[^"]*)?"/g)) {
        const tag = match[1]
        // 路径、文件扩展名、文件名里的点都不是 tag
        if (tag.includes('/') || tag.includes('\\')) continue
        if (/\.(toml|json|log|exe|dll|md|zip|png|jpg|lock|txt|nsh|wxs|nsi|so|dylib|cfg|sig|db|app|rs|com)$/i.test(tag)) continue
        // 域名首字母大写（BepInEx.cfg、Overcooked2.app 之类）
        if (!/^[a-z]/.test(tag.split('.')[0])) continue
        if (!found.has(tag)) found.set(tag, [])
        found.get(tag).push(`${file}:${index + 1}`)
      }
    })
  }
  return found
}

test('后端 tag 字面量都登记进了语言表', () => {
  const { byDomain, flat } = readLocaleTables()
  const tags = collectRustTags()
  assert.ok(tags.size > 200, `扫描到的 tag 太少（${tags.size}），扫描器可能失效了`)

  const missing = []
  const unknownDomain = []
  for (const [tag, where] of tags) {
    if (flat.has(tag) || NON_TAG.has(tag) || isLegacyErrorTag(tag) || isFixtureIdentifier(tag)) continue
    const [domain, ...rest] = tag.split('.')
    if (!byDomain.has(domain)) {
      unknownDomain.push(`${tag}（域 ${domain} 没有登记）— ${where.slice(0, 2).join(', ')}`)
      continue
    }
    if (!byDomain.get(domain).has(rest.join('.'))) {
      missing.push(`${tag} — ${where.slice(0, 2).join(', ')}`)
    }
  }

  assert.deepEqual(
    unknownDomain, [],
    `以下 tag 所在的域没有登记语言表，界面上会原样显示 tag：\n  ${unknownDomain.join('\n  ')}`,
  )
  assert.deepEqual(
    missing, [],
    `以下 tag 没有登记语言表，界面上会原样显示 tag：\n  ${missing.join('\n  ')}`,
  )
})

test('tag 字面量的参数名与语言表占位符一致', () => {
  const { byDomain, flat } = readLocaleTables()
  const text = readdirSync(RUST_DIR)
    .filter((file) => file.endsWith('.rs'))
    .map((file) => readText(join(RUST_DIR, file)))
    .join('\n')
  const localeText = readdirSync(LOCALE_DIR)
    .filter((file) => file.endsWith('.ts'))
    .map((file) => readText(join(LOCALE_DIR, file)))
    .join('\n')

  /*
   * 只核对 `with_params("tag", &[("name", …)])` 这种明确点名的调用：
   * 内联 `format!("tag?a={}")` 的写法参数名藏在格式串里，静态提取容易误判，
   * 交给前端 decodeURIComponent 兜底即可，不在这里制造假失败。
   */
  const mismatches = []
  const callRe = /with_params\(\s*"([A-Za-z][A-Za-z0-9]*(?:\.[A-Za-z][A-Za-z0-9]*)+)"\s*,\s*&\[([^\]]*)\]/g
  let call
  while ((call = callRe.exec(text)) !== null) {
    const tag = call[1]
    if (NON_TAG.has(tag) || isLegacyErrorTag(tag)) continue
    const paramNames = [...call[2].matchAll(/\("([A-Za-z_][A-Za-z0-9_]*)"/g)].map((m) => m[1])
    if (!paramNames.length) continue

    const [domain, ...rest] = tag.split('.')
    const key = rest.join('.')
    const entry = byDomain.get(domain)?.has(key)
      ? localeText.match(new RegExp(`'${key.replace(/\./g, '\\.')}':\\s*\\{([\\s\\S]*?)\\n  \\}`))
      : null
    if (!entry) continue // 缺失由上一个测试负责报
    const placeholders = new Set([...entry[1].matchAll(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g)].map((m) => m[1]))
    for (const name of paramNames) {
      // 语言表里参数名对不上时 vue-i18n 不报错，只会把 {name} 原样留在句子里
      if (!placeholders.has(name)) {
        mismatches.push(`${tag} 传了参数 ${name}，但语言表里没有 {${name}}`)
      }
    }
  }

  assert.deepEqual(mismatches, [], `参数名与语言表不一致：\n  ${mismatches.join('\n  ')}`)
  assert.ok(!flat.has('__never__'), 'flat 表结构读取异常')
})
