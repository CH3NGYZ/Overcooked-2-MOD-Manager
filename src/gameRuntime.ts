import type { AppPlatform, GameRuntime, Provider } from './types'

/**
 * 跨平台运行时辅助（纯函数，可直接在 Node 里测）。
 *
 * 全部围绕一个事实：同一款游戏在三个平台上「可执行文件」「BepInEx 包」
 * 「Steam 启动项」都不同，而 macOS 还分 CrossOver（Wine 容器里的 Windows 版）
 * 与原生版两套。这些判断只依赖配置里的 platform / runtime，不碰 IPC。
 */

/** 游戏可执行文件名（用于判断目录是否选对）。 */
export function gameExecutableName(platform?: AppPlatform, runtime?: GameRuntime): string {
  if (runtime === 'native' && platform === 'linux') return 'Overcooked2.x86_64'
  if (runtime === 'native' && platform === 'macos') return 'Overcooked2.app'
  return 'Overcooked2.exe'
}

/** 各渠道的 BepInEx 安装包地址（后端 `bepinex::Package::url` 的前端镜像）。 */
export function bepinexDownloadUrls(platform?: AppPlatform, runtime?: GameRuntime): Record<Provider, string> {
  const packageName = runtime === 'native' && platform === 'linux' ? 'BepInEx_linux_x64_5.4.23.5.zip'
    : runtime === 'native' && platform === 'macos' ? 'BepInEx_macos_universal_5.4.23.5.zip'
    : 'BepInEx_win_x86_5.4.23.5.zip'
  return {
    gitee: `https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/${packageName}`,
    github: `https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/${packageName}`,
  }
}

/**
 * 原生模式需要写进 Steam「启动选项」的命令。
 *
 * - Linux：直接跑 `run_bepinex.sh`；
 * - macOS：脚本在游戏目录里，路径含空格时必须整体加引号，且引号内的
 *   `\` `"` `$` `` ` `` 要转义，否则 Steam 的启动项解析会截断路径。
 */
export function nativeSteamLaunchOptions(platform: AppPlatform, gamePath: string): string {
  if (!gamePath) return ''
  if (platform === 'linux') return './run_bepinex.sh %command%'
  if (platform !== 'macos') return ''
  const script = `${gamePath.replace(/\/$/, '')}/run_bepinex.sh`
  return `"${script.replace(/[\\"$`]/g, '\\$&')}" %command%`
}

/** CrossOver 容器里补字体的命令（缺 corefonts 时 BepInEx 控制台会乱码）。 */
export function corefontsCommand(prefix?: string): string {
  if (!prefix) return ''
  // 单引号包裹 + 把内部单引号转成 '\''，是 shell 里最稳的转义方式
  return `WINEPREFIX='${prefix.replace(/'/g, "'\\''")}' winetricks corefonts`
}
