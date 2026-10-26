import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  AppUpdateChannelEvent, AppUpdateCheckResult, AppUpdatePhaseEvent, BepInExInstallResult,
  Dashboard, DownloadProgress, GameInfo, ModRecycleEntry, OperationResult, Provider,
  PublicConfig, ReleaseInfo, RepositoryReadme, RustLog, SettingsPatch, UninstallLocalModResult,
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
  uninstallLocalMod: (dllFile: string, allowUnmanaged = false) => invoke<UninstallLocalModResult>('uninstall_local_mod', { dllFile, allowUnmanaged }),
  uninstallModDll: (dllFile: string, packageId: string) => invoke<OperationResult>('uninstall_mod_dll', { dllFile, packageId }),
  getModRecycleBin: () => invoke<ModRecycleEntry[]>('get_mod_recycle_bin'),
  restoreRecycledMod: (id: string) => invoke<OperationResult>('restore_recycled_mod', { id }),
  deleteRecycledMod: (id: string) => invoke<OperationResult>('delete_recycled_mod', { id }),
  detectGame: () => invoke<GameInfo>('detect_game'),
  setGamePath: (path: string) => invoke<GameInfo>('set_game_path', { path }),
  resetGamePath: () => invoke<GameInfo>('reset_game_path'),
  saveSettings: (patch: SettingsPatch) => invoke<PublicConfig>('save_settings', { patch }),
  refreshRepository: (repositoryId: string) => invoke<ReleaseInfo[]>('refresh_repository', { repositoryId }),
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
  installRepository: (repositoryId: string, tag?: string) => invoke<OperationResult>('install_repository', { repositoryId, tag }),
  launchNormal: () => invoke<OperationResult>('launch_normal'),
  launchNoMod: () => invoke<OperationResult>('launch_no_mod'),
  switchWinhttp: (enabled: boolean) => invoke<OperationResult>('switch_winhttp', { enabled }),
  toggleLocalMod: (dllFile: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod', { dllFile, enabled }),
  toggleLocalModPackage: (packageId: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod_package', { packageId, enabled }),
  openDevtools: () => invoke<OperationResult>('open_devtools'),
  resetUi: () => invoke<OperationResult>('reset_ui'),
  openGameDirectory: () => invoke<OperationResult>('open_game_directory'),
  openModDirectory: () => invoke<OperationResult>('open_mod_directory'),
  openModFileLocation: (dllFile: string) => invoke<OperationResult>('open_mod_file_location', { dllFile }),
  openBackupDirectory: (backupPath?: string) => invoke<OperationResult>('open_backup_directory', { backupPath: backupPath ?? null }),
  openUpdateRepository: () => invoke<OperationResult>('open_update_repository'),
  exitApp: () => invoke<void>('exit_app'),
  cancelDownload: () => invoke<OperationResult>('cancel_download'),
  checkAppUpdate: (locale: 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja') => invoke<AppUpdateCheckResult>('check_app_update', { locale }),
  installAppUpdate: () => invoke<OperationResult>('install_app_update'),
  chooseGameDirectory: () => open({ multiple: false, directory: true }),
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
