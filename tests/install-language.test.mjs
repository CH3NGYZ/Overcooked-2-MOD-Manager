import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 首启语言优先级：安装程序写入的语言只作为「用户还没选过」时的默认值。
 *
 * `i18n.ts` 依赖 localStorage 与 vue 运行时，无法在 Node 里直接执行，
 * 所以这里对真实源码做结构性断言 —— 与 registry.test.mjs 的做法一致。
 * 行为本身另由 CDP 实测覆盖（见 PLAN.md 第 1 步的验收记录）。
 */
const i18n = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8')
const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8')

test('locale 初始化优先 localStorage', () => {
  // 首行就必须是「localStorage 有合法值则用它」，否则会覆盖用户的选择
  assert.match(
    i18n,
    /export const locale = ref<Locale>\(\s*LOCALES\.includes\(localStorage\.getItem\(LOCALE_KEY\) as Locale\)/,
  )
})

test('applyInstallLanguage 在已有用户选择时直接返回', () => {
  const body = i18n.slice(i18n.indexOf('export function applyInstallLanguage'))
  assert.match(body, /if \(hasStoredLocale\(\)\) return false/, '必须先检查是否已有用户选择')
  // 检查要排在赋值之前，否则会覆盖用户选择
  const guardAt = body.indexOf('hasStoredLocale()')
  const assignAt = body.indexOf('locale.value =')
  assert.ok(guardAt >= 0 && assignAt > guardAt, '守卫必须在赋值之前')
})

test('未知语言不会被应用', () => {
  const body = i18n.slice(i18n.indexOf('export function applyInstallLanguage'))
  assert.match(body, /LOCALES\.includes\(value as Locale\)/, '必须校验取值在支持列表内')
  // 非法值走 else 分支：落盘默认语言，但不采纳该值
  assert.match(body, /localStorage\.setItem\(LOCALE_KEY, locale\.value\)/, '首启应把默认语言落盘一次')
  // 命中才赋值
  assert.match(body, /if \(applied\)[\s\S]*?locale\.value = value as Locale/)
})

test('首次启动后不再被安装语言覆盖', () => {
  const body = i18n.slice(i18n.indexOf('export function applyInstallLanguage'))
  // 两条路径都会写 localStorage，所以第二次启动 hasStoredLocale() 为真
  assert.match(body, /if \(hasStoredLocale\(\)\) return false/)
})

test('main 在 mount 之前应用安装语言', () => {
  const applyAt = main.indexOf('applyInstallLanguage')
  const mountAt = main.indexOf('.mount(')
  assert.ok(applyAt >= 0, 'main.ts 应调用 applyInstallLanguage')
  assert.ok(mountAt > applyAt, '必须在挂载前完成，否则会先渲染默认语言再切换')
})

test('安装语言读取失败不影响启动', () => {
  // 浏览器预览、无文件、IPC 拒绝都会抛错，必须被吞掉
  assert.match(main, /try\s*\{[\s\S]*?api\.installLanguage\(\)[\s\S]*?\}\s*catch\s*\{/)
})
