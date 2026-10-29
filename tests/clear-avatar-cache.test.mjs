import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 清除头像缓存必须同时清两层。
 *
 * `src/avatars.ts` 依赖 Vue 的 `ref` 与 Tauri IPC，无法在 Node 里直接执行，
 * 所以对真实源码做结构性断言（与其它只能源码断言的模块同一做法）。
 * 行为层的「命令被调用 + 有数量提示」由 tools/verify-clear-avatars.mjs 覆盖。
 *
 * 为什么必须清 `requested`：它记录「本次会话已经问过后端」的 key。
 * 只清 `cache` 的话，界面会立刻回落到平台图标，而且**再也不会重新获取** ——
 * 预取会认为这些 owner 已经请求过，用户看到的就是「点了之后头像永远消失」。
 */
const source = readFileSync(
  new URL('../src/avatars.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/** 取出 clearAvatarCache 的函数体。 */
function body() {
  const start = source.indexOf('export function clearAvatarCache')
  assert.ok(start >= 0, '应当导出 clearAvatarCache')
  const rest = source.slice(start)
  const end = rest.indexOf('\n}')
  assert.ok(end > 0, '函数体没有正常结束')
  return rest.slice(0, end)
}

test('clearAvatarCache 清空内存缓存', () => {
  assert.match(body(), /cache\.value\s*=/, '应当重置 cache')
})

test('clearAvatarCache 清空「已请求」标记', () => {
  assert.match(
    body(),
    /requested\.clear\(\)/,
    '必须清 requested，否则本次会话不会再重新获取头像',
  )
})

test('工具页的清除入口同时调用后端与前端清零', () => {
  const tools = readFileSync(
    new URL('../src/views/ToolsView.vue', import.meta.url),
    'utf8',
  ).replace(/\r\n/g, '\n')
  const start = tools.indexOf('async function clearAvatars')
  assert.ok(start >= 0, '工具页应当有 clearAvatars')
  const block = tools.slice(start, tools.indexOf('\n}', start))
  assert.match(block, /api\.clearRepositoryAvatars\(\)/, '必须清后端磁盘缓存')
  assert.match(block, /clearAvatarCache\(\)/, '必须清前端内存缓存')
})
