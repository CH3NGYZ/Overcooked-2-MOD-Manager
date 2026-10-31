import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 展示路径去掉 Windows 规范化前缀（真实源码直接执行：纯函数）。
 *
 * 后端探测游戏目录用的是 `fs::canonicalize`，Windows 上会带 verbatim 前缀；
 * 这份路径既显示在设置页的「游戏目录」说明里，也放进路径抽屉的当前值，
 * 带前缀时用户看到的是 `\\?\E:\…`，像出了 bug。
 */
const { displayPath } = await import('../src/gameRuntime.ts')

// '\\\\?\\' 就是四个字符 \ \ ? \，'\\\\?\\UNC\\' 就是 \\?\UNC\
const VERBATIM = '\\\\?\\'
const UNC_PREFIX = '\\\\?\\UNC\\'
const BACKSLASH = '\\'

test('去掉 Windows verbatim 前缀，保留盘符路径', () => {
  const raw = VERBATIM + 'E:' + BACKSLASH + 'SteamLibrary' + BACKSLASH + 'game'
  const expected = 'E:' + BACKSLASH + 'SteamLibrary' + BACKSLASH + 'game'
  assert.equal(displayPath(raw), expected)
})

test('UNC 路径还原成普通网络路径', () => {
  const raw = UNC_PREFIX + 'nas' + BACKSLASH + 'share' + BACKSLASH + 'game'
  const expected = BACKSLASH + BACKSLASH + 'nas' + BACKSLASH + 'share' + BACKSLASH + 'game'
  assert.equal(displayPath(raw), expected)
})

test('macOS 路径与空值原样返回', () => {
  const macPath = '/Users/me/Library/Application Support/Steam/steamapps/common/Overcooked! 2'
  assert.equal(displayPath(macPath), macPath)
  assert.equal(displayPath(''), '')
  assert.equal(displayPath(undefined), '')
  assert.equal(displayPath('   '), '')
})
