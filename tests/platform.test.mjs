import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 窗控按钮摆哪边的系统判定（真实源码直接执行：纯函数、不碰 IPC）。
 *
 * 这条判定只影响标题栏布局，业务平台仍读 `config.platform`；但它决定
 * macOS 上三个圆点在左还是右侧，判错就是每次开窗都在错的位置。
 */
const { isMacWindowChrome } = await import('../src/platform.ts')

const MAC_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)'
const MAC_ARM_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 11_2_3) AppleWebKit/605.1.15 (KHTML, like Gecko)'
const WIN_UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0'
const LINUX_UA = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)'

test('macOS 的 WKWebView UA 判定为 macOS（Intel 与 Apple Silicon 同一文案）', () => {
  assert.equal(isMacWindowChrome(MAC_UA), true)
  assert.equal(isMacWindowChrome(MAC_ARM_UA), true)
})

test('Windows / Linux / 空 UA 都不会被当成 macOS', () => {
  assert.equal(isMacWindowChrome(WIN_UA), false)
  assert.equal(isMacWindowChrome(LINUX_UA), false)
  assert.equal(isMacWindowChrome(''), false)
})

test('iOS 的 "like Mac OS X" 不触发 macOS 布局', () => {
  assert.equal(
    isMacWindowChrome('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15'),
    false,
  )
})
