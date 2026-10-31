import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 三态主题（深色 / 浅色 / 跟随系统）的接线必须完整。
 *
 * `theme.ts` 与 `systemTheme.ts` 都依赖浏览器 API（localStorage、matchMedia、
 * Tauri 宿主事件），无法在 Node 里直接执行，所以这里对真实源码做结构性断言
 * ——与 `theme-default-accent.test.mjs` 同一套路。运行时效果（三种图标、切换
 * 顺序、自动跟随系统切换）由 CDP 实机验收覆盖。
 */
const read = (name) =>
  readFileSync(new URL(`../src/${name}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')

/** 去掉注释后的源码：反例断言不能被解释性注释误伤。 */
const strip = (code) =>
  code.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1')

const themeCode = strip(read('theme.ts'))
const systemCode = strip(read('systemTheme.ts'))

test('ThemeMode 含 auto，且区分「用户选择」与「实际生效」', () => {
  assert.match(themeCode, /export type ThemeMode = 'dark' \| 'light' \| 'auto'/)
  assert.match(themeCode, /export type EffectiveThemeMode = 'dark' \| 'light'/)
  assert.match(themeCode, /export const resolvedMode = computed<EffectiveThemeMode>/)
  assert.match(themeCode, /themeMode\.value === 'auto' \? \(systemDark\.value \? 'dark' : 'light'\)/)
})

test('界面配色只读 resolvedMode，不再直接拿 themeMode 判断明暗', () => {
  assert.match(themeCode, /isDark = computed\(\(\) => resolvedMode\.value === 'dark'\)/)
  assert.match(themeCode, /naiveTheme = computed<GlobalTheme \| null>\(\(\) => \(resolvedMode\.value === 'dark'/)
  assert.match(themeCode, /SURFACES\[resolvedMode\.value\]/)
  assert.equal(/colorScheme = themeMode\.value/.test(themeCode), false)
  assert.equal(/classList\.toggle\('dark', themeMode\.value/.test(themeCode), false)
})

test('三态循环顺序固定：深色 → 浅色 → 跟随系统', () => {
  assert.match(themeCode, /const order: ThemeMode\[\] = \['dark', 'light', 'auto'\]/)
  assert.match(themeCode, /export function cycleThemeMode\(\)/)
})

test('系统外观同时接 prefers-color-scheme 与宿主 theme-changed', () => {
  assert.match(systemCode, /prefers-color-scheme: dark/)
  assert.match(systemCode, /matchMedia/)
  assert.match(systemCode, /onThemeChanged/)
  // 不能自己 setTheme：一旦覆盖窗口主题，WebView2 的 prefers-color-scheme
  // 就不再是系统原值，「跟随系统」会退化成跟随上次手选的颜色。
  assert.equal(/setTheme\(/.test(systemCode), false)
})

test('auto 是合法持久化值（旧版本只存过 dark/light）', () => {
  assert.match(themeCode, /const MODE_VALUES: ThemeMode\[\] = \['dark', 'light', 'auto'\]/)
  assert.match(themeCode, /MODE_VALUES\.includes\(localStorage\.getItem\(MODE_KEY\) as ThemeMode\)/)
})

test('从未选择过时默认跟随系统；存过值以存值为准', () => {
  assert.match(themeCode, /const DEFAULT_MODE: ThemeMode = 'auto'/)
  // 三元的两条分支：合法存值原样用，没有/非法才回落到默认
  assert.match(
    themeCode,
    /MODE_VALUES\.includes\(localStorage\.getItem\(MODE_KEY\) as ThemeMode\)\s*\? \(localStorage\.getItem\(MODE_KEY\) as ThemeMode\)\s*: DEFAULT_MODE,/,
  )
})
