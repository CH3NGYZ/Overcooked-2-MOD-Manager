import assert from 'node:assert/strict'
import test from 'node:test'

/** 跨平台运行时辅助：真实源码直接执行（纯函数，无 IPC 依赖）。 */
const {
  gameExecutableName, bepinexDownloadUrls, nativeSteamLaunchOptions, corefontsCommand,
} = await import('../src/gameRuntime.ts')

test('Windows 与 CrossOver 都用 Windows 可执行文件名', () => {
  assert.equal(gameExecutableName('windows', 'native'), 'Overcooked2.exe')
  // macOS + wine = CrossOver 里的 Windows 版
  assert.equal(gameExecutableName('macos', 'wine'), 'Overcooked2.exe')
})

test('只有 macOS 原生有自己的可执行文件名', () => {
  assert.equal(gameExecutableName('macos', 'native'), 'Overcooked2.app')
  assert.equal(gameExecutableName('windows', 'native'), 'Overcooked2.exe')
})

test('BepInEx 包按平台与运行时区分', () => {
  const win = bepinexDownloadUrls('windows', 'native')
  assert.ok(win.github.includes('BepInEx_win_x86'))
  assert.ok(win.gitee.includes('BepInEx_win_x86'))

  const macNative = bepinexDownloadUrls('macos', 'native')
  assert.ok(macNative.github.includes('BepInEx_macos_universal'))

  const macWine = bepinexDownloadUrls('macos', 'wine')
  assert.ok(macWine.github.includes('BepInEx_win_x86'), 'CrossOver 装 Windows 版包')
})

test('两个渠道的包名一致，只有仓库主机不同', () => {
  const urls = bepinexDownloadUrls('macos', 'native')
  const name = 'BepInEx_macos_universal_5.4.23.5.zip'
  assert.ok(urls.github.endsWith(name))
  assert.ok(urls.gitee.endsWith(name))
  assert.ok(urls.github.includes('github.com/BepInEx/BepInEx/releases'))
  assert.ok(urls.gitee.includes('gitee.com/ch3ngyz/'))
})

test('Windows 没有原生启动项（走 doorstop 的 winhttp.dll）', () => {
  assert.equal(nativeSteamLaunchOptions('windows', 'C:/Games/Overcooked 2'), '')
})

test('原生 macOS 的启动项是带引号的绝对脚本路径', () => {
  const options = nativeSteamLaunchOptions('macos', '/Applications/Overcooked 2')
  assert.equal(options, '"/Applications/Overcooked 2/run_bepinex.sh" %command%')
})

test('macOS 启动项转义引号内的特殊字符', () => {
  // 路径里出现 $ ` " \ 时必须转义，否则 Steam 解析启动项会截断
  const options = nativeSteamLaunchOptions('macos', '/tmp/a$b`c"d')
  assert.ok(options.includes('\\$'), '$ 应被转义')
  assert.ok(options.includes('\\`'), '反引号应被转义')
  assert.ok(options.includes('\\"'), '双引号应被转义')
  assert.ok(options.endsWith(' %command%'))
})

test('末尾斜杠不会产生双斜杠', () => {
  assert.equal(
    nativeSteamLaunchOptions('macos', '/Applications/Overcooked2/'),
    '"/Applications/Overcooked2/run_bepinex.sh" %command%',
  )
})

test('Windows 没有原生启动项', () => {
  assert.equal(nativeSteamLaunchOptions('windows', 'C:/Games/OC2'), '')
})

test('游戏路径为空时返回空启动项', () => {
  assert.equal(nativeSteamLaunchOptions('macos', ''), '')
})

test('corefonts 命令带上 WINEPREFIX', () => {
  assert.equal(
    corefontsCommand('/Users/me/Library/CrossOver/Bottles/Steam'),
    "WINEPREFIX='/Users/me/Library/CrossOver/Bottles/Steam' winetricks corefonts",
  )
})

test('corefonts 命令转义路径里的单引号', () => {
  const cmd = corefontsCommand("/tmp/it's here")
  // 形如 WINEPREFIX='/tmp/it'\''s here' —— shell 里这样才是一个完整参数
  assert.ok(cmd.includes("'\\''"), '单引号必须转义')
  assert.ok(cmd.startsWith('WINEPREFIX='))
  assert.ok(cmd.endsWith('winetricks corefonts'))
})

test('没有 wine prefix 时不产生 corefonts 命令', () => {
  assert.equal(corefontsCommand(undefined), '')
  assert.equal(corefontsCommand(''), '')
})
