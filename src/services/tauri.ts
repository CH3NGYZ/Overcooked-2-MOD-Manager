import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  AppUpdateChannelEvent, AppUpdateCheckResult, AppUpdatePhaseEvent, BepInExInstallResult,
  Dashboard, DownloadProgress, GameInfo, ModConfigDocument, ModPreview, ModRecycleEntry, ModSourceKind,
  OperationResult, PermissionStatus, Provider,
  PublicConfig, ReleaseInfo, RepositoryInstallOutcome, RepositoryReadme, RepositorySourceImportResult,
  RustLog, SettingsPatch,
  TokenPatch, UninstallLocalModResult,
} from '../types'

let gameStateRevision = 0
let latestGameState: GameInfo | undefined

async function readDashboard(): Promise<Dashboard> {
  const revision = gameStateRevision
  const dashboard = await invoke<Dashboard>('get_dashboard')
  // MOD 扫描可能慢于游戏状态事件：保留较新的事件快照，避免旧状态覆盖。
  if (revision !== gameStateRevision && latestGameState) dashboard.game = latestGameState
  return dashboard
}

/**
 * v2 前端调用面：只封装新版 UI 实际使用的命令。
 * 教程、社区搜索、代理管理、导入导出、配置编辑器等 v1 功能已裁剪；
 * 后端命令仍全部注册，供后续版本按需恢复。
 */
export const api = {
  dashboard: readDashboard,
  config: () => invoke<PublicConfig>('get_config'),
  readRustLog: () => invoke<RustLog>('read_rust_log'),
  readFrontendLog: () => invoke<RustLog>('read_frontend_log'),
  readCrashLog: () => invoke<RustLog>('read_crash_log'),
  writeFrontendLog: (level: string, message: string) => invoke<void>('write_frontend_log', { level, message }),
  saveModNote: (dllFile: string, note: string) => invoke<OperationResult>('save_mod_note', { dllFile, note }),
  /** MOD 在 BepInEx/config 下的配置文件（含 revision 防覆盖）。 */
  readModConfig: (dllFile: string) => invoke<ModConfigDocument>('read_mod_config', { dllFile }),
  saveModConfig: (dllFile: string, revision: string, content: string) =>
    invoke<ModConfigDocument>('save_mod_config', { dllFile, revision, content }),
  /** BepInEx 框架自身的配置文件（`BepInEx/config/BepInEx.cfg`）。 */
  readBepInExConfig: () => invoke<ModConfigDocument>('read_bepinex_config'),
  saveBepInExConfig: (revision: string, content: string) =>
    invoke<ModConfigDocument>('save_bepinex_config', { revision, content }),
  uninstallLocalMod: (dllFile: string, allowUnmanaged = false) => invoke<UninstallLocalModResult>('uninstall_local_mod', { dllFile, allowUnmanaged }),
  uninstallModDll: (dllFile: string, packageId: string) => invoke<OperationResult>('uninstall_mod_dll', { dllFile, packageId }),
  getModRecycleBin: () => invoke<ModRecycleEntry[]>('get_mod_recycle_bin'),
  restoreRecycledMod: (id: string) => invoke<OperationResult>('restore_recycled_mod', { id }),
  deleteRecycledMod: (id: string) => invoke<OperationResult>('delete_recycled_mod', { id }),
  detectGame: () => invoke<GameInfo>('detect_game'),
  setGamePath: (path: string) => invoke<GameInfo>('set_game_path', { path }),
  resetGamePath: () => invoke<GameInfo>('reset_game_path'),
  saveSettings: (patch: SettingsPatch) => invoke<PublicConfig>('save_settings', { patch }),
  /** 保存 / 清除 API 个人 Token（非空字符串覆盖，空字符串保持不变）。 */
  saveTokens: (patch: TokenPatch) => invoke<PublicConfig>('save_tokens', { patch }),
  refreshRepository: (repositoryId: string) => invoke<ReleaseInfo[]>('refresh_repository', { repositoryId }),
  /** 从源文档地址（raw / 远程 TOML）重新拉取并应用该仓库源。 */
  importRepositorySource: (sourceId: string, sourceUrl: string, builtinFallback = false) =>
    invoke<RepositorySourceImportResult>('import_repository_source', { sourceId, sourceUrl, builtinFallback }),
  /**
   * 刷新前清空这些源（含后代）下的仓库与子源缓存，返回清理后的配置。
   *
   * 为什么要先清：不清的话，前端会继续显示上一次的仓库列表，
   * 直到新文档回来才替换 —— 用户看到「旧列表先出现、再跳变」。
   * 清空后列表立即变空（配合页面的加载态），权威数据只来自本次 raw 拉取。
   */
  clearRepositorySourceCache: (sourceIds: string[]) =>
    invoke<PublicConfig>('clear_repository_source_cache', { sourceIds }),
  releases: (repositoryId: string) => invoke<ReleaseInfo[]>('get_repository_releases', { repositoryId }),
  installBepInEx: (reinstall: boolean, provider: Provider) => invoke<BepInExInstallResult>('install_bepinex', { reinstall, provider }),
  repairNativeBepInExOverlay: () => invoke<OperationResult>('repair_native_bepinex_overlay'),
  uninstallBepInEx: () => invoke<OperationResult>('uninstall_bepinex'),
  backupBepInEx: () => invoke<OperationResult>('backup_bepinex'),
  restoreBepInEx: (backupPath: string) => invoke<OperationResult>('restore_bepinex', { backupPath }),
  deleteBepInExBackup: (backupPath: string) => invoke<OperationResult>('delete_bepinex_backup', { backupPath }),
  /** 仓库 README（详情面板按需加载；requestId 用于取消）。 */
  repositoryReadme: (repositoryId: string, requestId: string) =>
    invoke<RepositoryReadme>('get_repository_readme', { repositoryId, requestId }),
  cancelRepositoryReadme: (requestId: string) => invoke<void>('cancel_repository_readme', { requestId }),
  /** 用系统默认浏览器打开外部链接（Release 页 / 源码仓库）。 */
  openReleasePage: (url: string) => invoke<OperationResult>('open_release_page', { url }),
  /**
   * 安装仓库资产。
   *
   * `previewOnly` 为真时只下载并解析、不写入游戏目录，返回结果里带
   * `preview` —— 仓库卡片的「安装 / 重装 / 更新」走这条路径先进预览弹窗，
   * 用户确认后再调 `confirmModInstall`。
   */
  installRepository: (repositoryId: string, tag?: string, previewOnly = false) =>
    invoke<RepositoryInstallOutcome>('install_repository', { repositoryId, tag, previewOnly }),
  launchNormal: () => invoke<OperationResult>('launch_normal'),
  /** 停止厨房：Steam 没有停止协议，由管理器结束游戏进程。 */
  stopGame: () => invoke<OperationResult>('stop_game'),
  launchNoMod: () => invoke<OperationResult>('launch_no_mod'),
  switchWinhttp: (enabled: boolean) => invoke<OperationResult>('switch_winhttp', { enabled }),
  toggleLocalMod: (dllFile: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod', { dllFile, enabled }),
  toggleLocalModPackage: (packageId: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod_package', { packageId, enabled }),
  openDevtools: () => invoke<OperationResult>('open_devtools'),
  resetUi: () => invoke<OperationResult>('reset_ui'),
  /** 把生成好的多彩名字推送给正在运行的多彩名字 MOD（后端逐个端口尝试）。 */
  pushColorfulName: (name: string) => invoke<OperationResult>('push_colorful_name', { name }),
  /** 复制文本到系统剪贴板。 */
  copyText: (text: string) => invoke<OperationResult>('copy_text', { text }),
  openGameDirectory: () => invoke<OperationResult>('open_game_directory'),
  openModDirectory: () => invoke<OperationResult>('open_mod_directory'),
  openModFileLocation: (dllFile: string) => invoke<OperationResult>('open_mod_file_location', { dllFile }),
  openModLogsDirectory: (dllFile: string) => invoke<OperationResult>('open_mod_logs_directory', { dllFile }),
  openBackupDirectory: (backupPath?: string) => invoke<OperationResult>('open_backup_directory', { backupPath: backupPath ?? null }),
  openUpdateRepository: () => invoke<OperationResult>('open_update_repository'),
  exitApp: () => invoke<void>('exit_app'),
  /** 当前进程权限与游戏/数据目录可写性自检。 */
  permissionStatus: () => invoke<PermissionStatus>('permission_status'),
  /** 以管理员身份重启（仅文件访问被拒时由用户主动触发）。 */
  relaunchAsAdmin: () => invoke<OperationResult>('relaunch_as_admin'),
  cancelDownload: () => invoke<OperationResult>('cancel_download'),
  /**
   * 安装程序在安装时写入的界面语言（`install-language.txt`）。
   *
   * 仅用于「首次启动且用户还没手动选过语言」的场景；后端只返回受支持的
   * 五种语言，其它内容一律为 null，调用方不必再校验。
   */
  installLanguage: () => invoke<string | null>('get_install_language'),
  /**
   * 搜索一个平台的社区仓库并合并进配置，返回合并后的完整配置。
   *
   * 搜索结果由后端 `merge_results` 落盘：与官方源同坐标的社区副本会被标成
   * `enabled=false` 整条隐藏，所以前端拿到配置后直接写回 Dashboard 即可。
   * 同一平台并发搜索时后端返回「社区仓库正在搜索」。
   */
  refreshCommunityRepositories: (provider: Provider) =>
    invoke<PublicConfig>('refresh_community_repositories', { provider }),
  checkAppUpdate: (locale: 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja') => invoke<AppUpdateCheckResult>('check_app_update', { locale }),
  installAppUpdate: () => invoke<OperationResult>('install_app_update'),
  chooseGameDirectory: () => open({ multiple: false, directory: true }),
  /** 选择要安装的本地 MOD 文件（DLL 或 ZIP）。 */
  chooseModFile: () => open({
    multiple: false,
    directory: false,
    filters: [{ name: 'MOD', extensions: ['dll', 'zip'] }],
  }),
  /** 安装本地 MOD 文件（DLL 或 ZIP）。 */
  installLocalMod: (sourcePath: string) => invoke<OperationResult>('install_local_mod', { sourcePath }),
  /** 从 zip / dll 直链下载并安装（后端已有直连下载 + 布局判定 + 清单）。 */
  installOnlineMod: (url: string) => invoke<OperationResult>('install_online_mod', { url }),
  /** 解析安装源（本地文件 / 在线直链），返回预览而不安装。 */
  analyzeModSource: (kind: ModSourceKind, value: string) =>
    invoke<ModPreview>('analyze_mod_source', { source: { kind, value } }),
  /** 用户在预览里确认后真正安装。 */
  confirmModInstall: (previewId: string) => invoke<OperationResult>('confirm_mod_install', { previewId }),
  /** 取消预览，删除暂存。 */
  cancelModPreview: (previewId: string) => invoke<void>('cancel_mod_preview', { previewId }),
}

export function watchDownloadProgress(callback: (progress: DownloadProgress) => void): Promise<UnlistenFn> {
  return listen<DownloadProgress>('download-progress', (event) => callback(event.payload))
}

export function watchGameState(callback: (game: GameInfo) => void): Promise<UnlistenFn> {
  return listen<GameInfo>('game-state-changed', (event) => {
    latestGameState = event.payload
    gameStateRevision += 1
    callback(event.payload)
  })
}

export function watchAppUpdatePhase(callback: (progress: AppUpdatePhaseEvent) => void): Promise<UnlistenFn> {
  return listen<AppUpdatePhaseEvent>('app-update-phase', (event) => callback(event.payload))
}

export function watchAppUpdateChannel(callback: (progress: AppUpdateChannelEvent) => void): Promise<UnlistenFn> {
  return listen<AppUpdateChannelEvent>('app-update-channel', (event) => callback(event.payload))
}
