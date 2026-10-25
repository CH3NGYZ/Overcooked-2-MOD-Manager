import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const sidebar = readFileSync(new URL('../src/components/AppSidebar.vue', import.meta.url), 'utf8')
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const baseCss = readFileSync(new URL('../src/styles/base.css', import.meta.url), 'utf8')
const i18n = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8')

test('the brand row toggles the rail without a separate chevron button', () => {
  assert.match(sidebar, /<button type="button" class="brand"/)
  assert.match(sidebar, /:aria-expanded="!collapsed"/)
  assert.match(sidebar, /t\(collapsed \? '展开侧边栏' : '收起侧边栏'\)/)
  assert.match(sidebar, /:class="\{ 'sidebar-collapsed': collapsed \}"/)
  assert.match(sidebar, /@click="emit\('toggle'\)"/)
  assert.doesNotMatch(sidebar, /sidebar-toggle/, 'The old toggle button must be gone')
  assert.match(app, /:collapsed="sidebarCollapsed"/)
  assert.match(app, /@toggle="sidebarCollapsed = !sidebarCollapsed"/)
  assert.match(i18n, /'收起侧边栏':/)
  assert.match(i18n, /'展开侧边栏':/)
})

test('icons never move: text retracts through max-width instead of display', () => {
  assert.match(baseCss, /--sidebar-collapsed-width: 73px/)
  assert.match(baseCss, /\.sidebar-collapsed\s*\{[^}]*width:\s*var\(--sidebar-collapsed-width\)/)
  assert.doesNotMatch(baseCss, /\.sidebar-collapsed \.nav-item\s*\{/, 'Collapsed items must keep their icon padding')
  assert.match(baseCss, /\.sidebar-collapsed \.nav-item > span\s*\{[^}]*max-width:\s*0/)
  assert.match(baseCss, /\.sidebar-collapsed \.brand-text\s*\{[^}]*max-width:\s*0/)
  assert.match(baseCss, /\.brand-text\s*\{[^}]*transition:[^}]*max-width/)
  assert.match(baseCss, /\.nav-item > span\s*\{[^}]*transition:[^}]*max-width/)
})

test('nav icons, the brand mark and the game icon are uniformly 22px', () => {
  assert.match(sidebar, /:size="22" stroke-width="1\.8"/)
  assert.match(baseCss, /\.brand-mark\s*\{[^}]*width:\s*22px/)
  assert.match(baseCss, /\.game-icon\s*\{[^}]*width:\s*22px/)
})

test('the collapsed state survives a restart', () => {
  assert.match(app, /const SIDEBAR_COLLAPSE_KEY = 'oc2-sidebar-collapsed'/)
  assert.match(app, /localStorage\.getItem\(SIDEBAR_COLLAPSE_KEY\)/)
  assert.match(app, /localStorage\.setItem\(SIDEBAR_COLLAPSE_KEY/)
})

test('the backdrop image follows the collapsed rail instead of the fixed width', () => {
  assert.match(baseCss, /\.app-shell\s*\{[^}]*--sidebar-active-width:\s*var\(--sidebar-width\)/)
  assert.match(baseCss, /\.app-shell\.sidebar-rail\s*\{[^}]*--sidebar-active-width:\s*var\(--sidebar-collapsed-width\)/)
  assert.match(baseCss, /\.app-shell::after\s*\{[^}]*left:\s*var\(--sidebar-active-width\)/)
  assert.match(baseCss, /\.app-shell::after\s*\{[^}]*width:\s*calc\(100% - var\(--sidebar-active-width\)\)/)
  assert.match(app, /'sidebar-rail': sidebarCollapsed/)
})

test('the game launch entry is one-way and reflects the running state', () => {
  assert.match(sidebar, /class="nav-item game-launch"/)
  assert.match(sidebar, /:class="\{ off: !kitchenRunning \}"/)
  assert.match(sidebar, /if \(kitchenRunning\.value\) return/)
  assert.match(sidebar, /props\.launchNormal/)
  assert.match(app, /:launch-normal="launchNormal"/)
  assert.match(sidebar, /kitchenRunning \? t\('游戏运行中'\) : t\('启动游戏'\)/)
  assert.match(i18n, /'启动游戏':/)
  assert.match(i18n, /'游戏运行中':/)
  assert.match(i18n, /'通过 Steam 启动胡闹厨房2（运行中点击无操作）':/)
})
