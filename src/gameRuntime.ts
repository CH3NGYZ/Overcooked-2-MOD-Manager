import type { AppPlatform, GameRuntime, Provider } from './types'

/**
 * 跨平台运行时辅助（纯函数，可直接在 Node 里测）。
 *
 * 全部围绕一个事实：同一款游戏在两个平台上「可执行文件」「BepInEx 包」
 * 「Steam 启动项」都不同，而 macOS 还分 CrossOver（Wine 容器里的 Windows 版）
 * 与原生版两套。这些判断只依赖配置里的 platform / runtime，不碰 IPC。
 *
 * 只覆盖 Windows 与 macOS —— 本项目不再发布 Linux 版本。
 */

/** 游戏可执行文件名（用于判断目录是否选对）。 */
export function gameExecutableName(platform?: AppPlatform, runtime?: GameRuntime): string {
  if (runtime === 'native' && platform === 'macos') return 'Overcooked2.app'
  return 'Overcooked2.exe'
}

/** 各渠道的 BepInEx 安装包地址（后端 `bepinex::Package::url` 的前端镜像）。 */
export function bepinexDownloadUrls(platform?: AppPlatform, runtime?: GameRuntime): Record<Extract<Provider, 'github' | 'gitee'>, string> {
  const packageName = runtime === 'native' && platform === 'macos'
    ? 'BepInEx_macos_universal_5.4.23.5.zip'
    : 'BepInEx_win_x86_5.4.23.5.zip'
  return {
    gitee: `https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/${packageName}`,
    github: `https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/${packageName}`,
  }
}

/**
 * 原生模式需要写进 Steam「启动选项」的命令。
 *
 * 只有 macOS 有原生模式：脚本在游戏目录里，路径含空格时必须整体加引号，
 * 且引号内的 `\` `"` `$` `` ` `` 要转义，否则 Steam 的启动项解析会截断路径。
 * （Windows 走 doorstop 的 winhttp.dll，不需要启动选项。）
 */
export function nativeSteamLaunchOptions(platform: AppPlatform, gamePath: string): string {
  if (!gamePath) return ''
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

/**
 * 展示给用户的路径：去掉 Windows `canonicalize` 带来的 verbatim 前缀。
 *
 * 后端探测游戏目录时用的是规范化路径，Windows 上会带 `\\?\` 前缀
 * （`\\?\E:\SteamLibrary\…`）。接口调用里它合法，但直接显示像是出了 bug、
 * 放进编辑框也没法用。UNC 的 `\\?\UNC\server\share` 还原成
 * `\\server\share`，其余原样返回（macOS 的路径不带这个前缀）。
 */
export function displayPath(path?: string): string {
  const value = (path ?? '').trim()
  if (!value) return ''
  const unc = '\\\\?\\UNC\\'
  if (value.startsWith(unc)) return `\\\\${value.slice(unc.length)}`
  const verbatim = '\\\\?\\'
  return value.startsWith(verbatim) ? value.slice(verbatim.length) : value
}
