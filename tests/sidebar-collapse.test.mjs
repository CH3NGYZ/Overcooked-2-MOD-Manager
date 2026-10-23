import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const sidebar = readFileSync(new URL('../src/components/AppSidebar.vue', import.meta.url), 'utf8')
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const baseCss = readFileSync(new URL('../src/styles/base.css', import.meta.url), 'utf8')
const i18n = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8')

test('the sidebar toggle sits in the brand row and flips its chevron', () => {
  assert.match(sidebar, /class="brand"[\s\S]*?class="sidebar-toggle"[\s\S]*?class="sidebar-section-label"/)
  assert.match(sidebar, /<ChevronRight v-if="collapsed" :size="15" \/>/)
  assert.match(sidebar, /<ChevronLeft v-else :size="15" \/>/)
  assert.match(sidebar, /:aria-expanded="!collapsed"/)
  assert.match(sidebar, /t\(collapsed \? '展开侧边栏' : '收起侧边栏'\)/)
  assert.match(sidebar, /:class="\{ 'sidebar-collapsed': collapsed \}"/)
  assert.match(sidebar, /@click="emit\('toggle'\)"/)
  assert.match(app, /:collapsed="sidebarCollapsed"/)
  assert.match(app, /@toggle="sidebarCollapsed = !sidebarCollapsed"/)
  assert.match(i18n, /'收起侧边栏':/)
  assert.match(i18n, /'展开侧边栏':/)
})

test('the collapsed rail keeps the workspace heading, icons and status dot only', () => {
  assert.match(baseCss, /--sidebar-collapsed-width: 84px/)
  assert.match(baseCss, /\.sidebar-collapsed\s*\{[^}]*width:\s*var\(--sidebar-collapsed-width\)/)
  assert.match(baseCss, /\.sidebar-collapsed \.brand-text\s*\{\s*display:\s*none/)
  assert.match(baseCss, /\.sidebar-collapsed \.nav-item > span,/)
  assert.match(baseCss, /\.sidebar-collapsed \.nav-chevron\s*\{\s*display:\s*none/)
  assert.match(baseCss, /\.sidebar-collapsed \.side-health-text,/)
  assert.match(baseCss, /\.sidebar-collapsed \.version-tag\s*\{\s*display:\s*none/)
  // The workspace heading stays visible in both states.
  assert.doesNotMatch(baseCss, /\.sidebar-collapsed \.sidebar-section-label\s*\{\s*display:\s*none/)
  assert.match(sidebar, /<span class="side-health-text">/)
  assert.match(sidebar, /class="pulse"/)
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
