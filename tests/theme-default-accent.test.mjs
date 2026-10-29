import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 默认主题色必须是琥珀色。
 *
 * 用户明确要求默认用琥珀色而不是绿色/青色。`theme.ts` 依赖 localStorage
 * 与 Vue，无法在 Node 里直接执行，所以这里对真实源码做结构性断言
 * （与 tests/install-language.test.mjs 同风格）。运行时效果由 CDP 覆盖。
 *
 * 读取的文件在 Windows 上是 CRLF，先归一化便于正则匹配。
 */
const theme = readFileSync(
  new URL('../src/theme.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/**
 * 去掉注释后的源码。
 *
 * 断言里要检查「不再出现 `'teal'` / `ACCENTS[2]`」这类反向条件，而解释性
 * 注释恰好会提到它们 —— 直接在原文上断言会误伤。注释本身不参与运行，
 * 剥掉之后再判断才反映真实代码。
 */
const code = theme
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/(^|[^:])\/\/.*$/gm, '$1')

test('默认主色是 amber', () => {
  const match = code.match(/const DEFAULT_ACCENT:\s*AccentKey\s*=\s*'([a-z]+)'/)
  assert.ok(match, '应当有 DEFAULT_ACCENT 常量')
  assert.equal(match[1], 'amber', '默认主色应为琥珀色')
})

test('未保存过选择时使用 DEFAULT_ACCENT，而不是硬编码颜色', () => {
  /*
   * 只检查 `accentKey` 初始化那一段。
   *
   * 不能对全文断言「不出现 'teal'」—— `AccentKey` 联合类型里本来就有
   * `'teal'`（它是合法可选项，只是不再是默认值）。
   */
  const init = code.slice(
    code.indexOf('export const accentKey'),
    code.indexOf('watch(themeMode'),
  )
  assert.ok(init.includes(': DEFAULT_ACCENT,'), 'accentKey 的兜底必须是 DEFAULT_ACCENT')
  assert.ok(!init.includes(": 'teal',"), '不应再硬编码 teal 作为兜底')
})

test('accent 计算找不到键时回落到默认色，而非固定下标', () => {
  assert.ok(
    /entry\.key === DEFAULT_ACCENT/.test(code),
    'accent 的兜底应使用 DEFAULT_ACCENT',
  )
  // 旧实现是 ACCENTS[2]，那是 rose，与默认色不一致
  assert.ok(
    !/ACCENTS\[2\]/.test(code),
    '不应再用固定下标作兜底',
  )
})
