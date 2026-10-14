import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const source = readFileSync(new URL('../src/gameRuntime.ts', import.meta.url), 'utf8')
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } })
const runtime = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`)

test('the frontend shows the correct executable and same-name GitHub/Gitee package for each runtime', () => {
  for (const [platform, mode, executable, packageName] of [
    ['windows', 'native', 'Overcooked2.exe', 'BepInEx_win_x86_5.4.23.5.zip'],
    ['linux', 'wine', 'Overcooked2.exe', 'BepInEx_win_x86_5.4.23.5.zip'],
    ['linux', 'native', 'Overcooked2.x86_64', 'BepInEx_linux_x64_5.4.23.5.zip'],
    ['macos', 'wine', 'Overcooked2.exe', 'BepInEx_win_x86_5.4.23.5.zip'],
    ['macos', 'native', 'Overcooked2.app', 'BepInEx_macos_universal_5.4.23.5.zip'],
  ]) {
    assert.equal(runtime.gameExecutableName(platform, mode), executable)
    const urls = runtime.bepinexDownloadUrls(platform, mode)
    assert.equal(urls.github, `https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/${packageName}`)
    assert.equal(urls.gitee, `https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/${packageName}`)
  }
})

test('Steam launch options distinguish relative Linux and safely quoted absolute Mac scripts', () => {
  assert.equal(runtime.nativeSteamLaunchOptions('linux', '/home/player/Overcooked! 2'), './run_bepinex.sh %command%')
  assert.equal(runtime.nativeSteamLaunchOptions('macos', '/Users/player/Overcooked! 2/'), '"/Users/player/Overcooked! 2/run_bepinex.sh" %command%')
  assert.equal(runtime.nativeSteamLaunchOptions('macos', '/Users/"$`\\/Game'), '"/Users/\\"\\$\\`\\\\/Game/run_bepinex.sh" %command%')
  for (const platform of ['windows', 'linux', 'macos']) assert.equal(runtime.nativeSteamLaunchOptions(platform, ''), '')
  assert.equal(runtime.nativeSteamLaunchOptions('windows', 'C:/Game'), '')
})

test('the corefonts command targets the actual prefix without shell interpolation', () => {
  assert.equal(runtime.corefontsCommand('/home/player/Prefix with spaces'), "WINEPREFIX='/home/player/Prefix with spaces' winetricks corefonts")
  assert.equal(runtime.corefontsCommand("/home/player's/Prefix"), "WINEPREFIX='/home/player'\\''s/Prefix' winetricks corefonts")
  assert.equal(runtime.corefontsCommand(undefined), '')
  assert.equal(runtime.corefontsCommand(''), '')
})
