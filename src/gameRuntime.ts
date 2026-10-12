import type { AppPlatform, GameRuntime, Provider } from './types'

export function gameExecutableName(platform?: AppPlatform, runtime?: GameRuntime): string {
  if (runtime === 'native' && platform === 'linux') return 'Overcooked2.x86_64'
  if (runtime === 'native' && platform === 'macos') return 'Overcooked! 2.app'
  return 'Overcooked2.exe'
}

export function bepinexDownloadUrls(platform?: AppPlatform, runtime?: GameRuntime): Record<Provider, string> {
  const packageName = runtime === 'native' && platform === 'linux' ? 'BepInEx_linux_x64_5.4.23.5.zip'
    : runtime === 'native' && platform === 'macos' ? 'BepInEx_macos_universal_5.4.23.5.zip'
    : 'BepInEx_win_x86_5.4.23.5.zip'
  return {
    gitee: `https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/${packageName}`,
    github: `https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/${packageName}`,
  }
}

export function nativeSteamLaunchOptions(platform: AppPlatform, gamePath: string): string {
  if (!gamePath) return ''
  if (platform === 'linux') return './run_bepinex.sh %command%'
  if (platform !== 'macos') return ''
  const script = `${gamePath.replace(/\/$/, '')}/run_bepinex.sh`
  return `"${script.replace(/[\\"$`]/g, '\\$&')}" %command%`
}

export function corefontsCommand(prefix?: string): string {
  if (!prefix) return ''
  return `WINEPREFIX='${prefix.replace(/'/g, "'\\''")}' winetricks corefonts`
}
