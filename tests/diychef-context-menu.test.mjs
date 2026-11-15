import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const tile = readFileSync(new URL('../src/components/diychef/ChefTile.vue', import.meta.url), 'utf8')
const grid = readFileSync(new URL('../src/components/diychef/ChefGrid.vue', import.meta.url), 'utf8')
const view = readFileSync(new URL('../src/views/DiyChefView.vue', import.meta.url), 'utf8')
const tauri = readFileSync(new URL('../src/services/tauri.ts', import.meta.url), 'utf8')
const locale = readFileSync(new URL('../src/locale/diychef/editor.ts', import.meta.url), 'utf8')

test('暖暖厨师卡片右键事件沿 ChefTile/ChefGrid 传到页面', () => {
  assert.match(tile, /contextmenu\.prevent\.stop/)
  assert.match(tile, /emit\('contextmenu', \$event, item\.chef\.id\)/)
  assert.match(grid, /contextmenu: \[event: MouseEvent, chefId: string\]/)
  assert.match(grid, /@contextmenu="\(event, id\) => emit\('contextmenu', event, id\)"/)
  assert.match(view, /@contextmenu="onChefContextMenu"/)
})

test('厨师卡片可通过 ContextMenu 或 Shift+F10 键盘打开同一菜单', () => {
  assert.match(tile, /event\.key !== 'ContextMenu' && !\(event\.key === 'F10' && event\.shiftKey\)/)
  assert.match(tile, /emit\('contextmenuKey', event, props\.item\.chef\.id\)/)
  assert.match(tile, /aria-haspopup="dialog"/)
  assert.match(grid, /@contextmenu-key="\(event, id\) => emit\('contextmenuKey', event, id\)"/)
  assert.match(view, /@contextmenu-key="onChefContextMenuKey"/)
  assert.match(view, /getBoundingClientRect\(\)/)
})

test('页面使用单一手动定位菜单并接通启停、换帽、删除确认', () => {
  assert.match(view, /trigger="manual"/)
  assert.match(view, /:x="contextMenuX"/)
  assert.match(view, /:y="contextMenuY"/)
  assert.match(view, /editor\.toggle\(chefId\)/)
  assert.match(view, /openHat\(chefId\)/)
  assert.match(view, /dialog\.warning\(/)
  assert.match(view, /editor\.reportOperationFailure\(reason\)/)
  assert.match(view, /api\.diyChefDeleteResource\(sessionId, chefId\)/)
  assert.match(view, /chef\.source_kind !== 'custom'/)
})

test('删除 IPC 使用不透明会话与厨师 ID，而不是前端路径', () => {
  assert.match(tauri, /diyChefDeleteResource: \(sessionId: string, chefId: string\)/)
  assert.match(tauri, /invoke<ChefEditorSnapshot>\('delete_diychef_resource', \{ sessionId, chefId \}\)/)
})

test('删除确认说明资源目录的回收与恢复方式，且五语齐全', () => {
  const entry = locale.split('\n').find((line) => line.includes('chefContextDeleteHint:'))
  assert.ok(entry)
  for (const language of ["'zh-CN'", "'zh-TW'", 'en', 'ko', 'ja']) {
    assert.match(entry, new RegExp(`${language}:`))
  }
  assert.match(entry, /资源文件会移入回收站/)
  assert.match(entry, /resource files move to the recycle bin/)
  assert.doesNotMatch(entry, /cannot be undone|无法撤销|無法復原|되돌릴 수 없습니다|元に戻せません/)
})
