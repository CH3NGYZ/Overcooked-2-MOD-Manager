import { invoke } from '@tauri-apps/api/core'
import type { BepInExInstallResult, ModConfigDocument, RepositoryReadme } from '../types'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import type { AppUpdateChannelEvent, AppUpdateCheckResult, AppUpdatePhaseEvent, Dashboard, DownloadProgress, GameInfo, GitHubProxyPatch, ImportPreview, ModRecycleEntry, OperationResult, Provider, ProxyMode, PublicConfig, ReleaseInfo, RepositoryPatch, RepositorySource, RepositorySourceImportResult, RustLog, SettingsPatch, TokenPatch, TutorialRepositoryConfig, TutorialStepConfig, UninstallLocalModResult } from '../types'

let gameStateRevision = 0
let latestGameState: GameInfo | undefined

async function readDashboard(): Promise<Dashboard> {
  const revision = gameStateRevision
  const dashboard = await invoke<Dashboard>('get_dashboard')
  // MOD scanning can outlast a game-state event. Keep the newer event snapshot
  // instead of overwriting it with the earlier state captured by the command.
  if (revision !== gameStateRevision && latestGameState) dashboard.game = latestGameState
  return dashboard
}

export const api = {
  dashboard: readDashboard,
  config: () => invoke<PublicConfig>('get_config'),
  installLanguage: () => invoke<'zh-CN' | 'zh-TW' | 'en' | 'ko' | null>('get_install_language'),
  readRustLog: () => invoke<RustLog>('read_rust_log'),
  readFrontendLog: () => invoke<RustLog>('read_frontend_log'),
  readCrashLog: () => invoke<RustLog>('read_crash_log'),
  writeFrontendLog: (level: string, message: string) => invoke<void>('write_frontend_log', { level, message }),
  saveModNote: (dllFile: string, note: string) => invoke<OperationResult>('save_mod_note', { dllFile, note }),
  installOnlineMod: (url: string) => invoke<OperationResult>('install_online_mod', { url }),
  installLocalMod: (sourcePath: string) => invoke<OperationResult>('install_local_mod', { sourcePath }),
  uninstallLocalMod: (dllFile: string, allowUnmanaged = false) => invoke<UninstallLocalModResult>('uninstall_local_mod', { dllFile, allowUnmanaged }),
  uninstallModDll: (dllFile: string, packageId: string) => invoke<OperationResult>('uninstall_mod_dll', { dllFile, packageId }),
  getModRecycleBin: () => invoke<ModRecycleEntry[]>('get_mod_recycle_bin'),
  restoreRecycledMod: (id: string) => invoke<OperationResult>('restore_recycled_mod', { id }),
  deleteRecycledMod: (id: string) => invoke<OperationResult>('delete_recycled_mod', { id }),
  detectGame: () => invoke<GameInfo>('detect_game'),
  checkCrossoverRunning: () => invoke<boolean>('check_crossover_running'),
  setGamePath: (path: string) => invoke<GameInfo>('set_game_path', { path }),
  resetGamePath: () => invoke<GameInfo>('reset_game_path'),
  saveSettings: (patch: SettingsPatch) => invoke<PublicConfig>('save_settings', { patch }),
  saveTokens: (patch: TokenPatch) => invoke<PublicConfig>('save_tokens', { patch }),
  saveGithubProxies: (patch: GitHubProxyPatch) => invoke<PublicConfig>('save_github_proxies', { patch }),
  refreshGithubProxies: () => invoke<PublicConfig>('refresh_github_proxies'),
  refreshRepository: (repositoryId: string) => invoke<ReleaseInfo[]>('refresh_repository', { repositoryId }),
  refreshCommunityRepositories: (provider: Provider = 'github') => invoke<PublicConfig>('refresh_community_repositories', { provider }),
  cancelCommunitySearch: (provider: Provider = 'github') => invoke<void>('cancel_community_search', { provider }),
  repositoryReadme: (repositoryId: string, requestId: string) => invoke<RepositoryReadme>('get_repository_readme', { repositoryId, requestId }),
  cancelRepositoryReadme: (requestId: string) => invoke<void>('cancel_repository_readme', { requestId }),
  releases: (repositoryId: string) => invoke<ReleaseInfo[]>('get_repository_releases', { repositoryId }),
  installBepInEx: (reinstall: boolean, provider: Provider) => invoke<BepInExInstallResult>('install_bepinex', { reinstall, provider }),
  repairNativeBepInExOverlay: () => invoke<OperationResult>('repair_native_bepinex_overlay'),
  uninstallBepInEx: () => invoke<OperationResult>('uninstall_bepinex'),
  backupBepInEx: () => invoke<OperationResult>('backup_bepinex'),
  restoreBepInEx: (backupPath: string) => invoke<OperationResult>('restore_bepinex', { backupPath }),
  deleteBepInExBackup: (backupPath: string) => invoke<OperationResult>('delete_bepinex_backup', { backupPath }),
  installRepository: (repositoryId: string, tag?: string, assetName?: string, assetProxyMode?: ProxyMode, assetProxyId?: string) => invoke<OperationResult>('install_repository', { repositoryId, tag, assetName, assetProxyMode, assetProxyId }),
  uninstallRepository: (repositoryId: string) => invoke<OperationResult>('uninstall_repository', { repositoryId }),
  exportRepositories: (ids: string[] | null) => invoke<string>('export_repositories', { ids }),
  exportFile: (path: string, ids: string[] | null) => invoke<OperationResult>('export_repositories_to_file', { path, ids }),
  copyRepositories: (ids: string[] | null) => invoke<OperationResult>('copy_repositories', { ids }),
  readClipboard: () => invoke<string>('read_clipboard'),
  copyText: (text: string) => invoke<OperationResult>('copy_text', { text }),
  getBuiltinTutorial: () => invoke<TutorialStepConfig[]>('get_builtin_tutorial'),
  getBuiltinTutorialRepositories: () => invoke<TutorialRepositoryConfig[]>('get_builtin_tutorial_repositories'),
  parseTutorialConfig: (text: string) => invoke<TutorialStepConfig[]>('parse_tutorial_config', { text }),
  serializeTutorialConfig: (steps: TutorialStepConfig[]) => invoke<string>('serialize_tutorial_config', { steps }),
  readTextFile: (path: string) => invoke<string>('read_text_file', { path }),
  previewImport: (text: string) => invoke<ImportPreview>('preview_repository_import', { text }),
  importRepositories: (text: string, conflict: string) => invoke<ImportPreview>('import_repositories', { text, conflict }),
  clearRepositorySourceCache: (sourceIds: string[]) => invoke<PublicConfig>('clear_repository_source_cache', { sourceIds }),
  importRepositorySource: (sourceId: string, sourceUrl: string, builtinFallback = false) => invoke<RepositorySourceImportResult>('import_repository_source', { sourceId, sourceUrl, builtinFallback }),
  saveRepositorySource: (id: string | null, nickname: string, url: string, nicknameI18n?: Record<string, string>) => invoke<PublicConfig>('save_repository_source', { id, nickname, url, nicknameI18n }),
  saveRepositorySources: (sources: RepositorySource[]) => invoke<PublicConfig>('save_repository_sources', { sources }),
  deleteRepositorySource: (id: string) => invoke<PublicConfig>('delete_repository_source', { id }),
  reorderRepositorySources: (ids: string[]) => invoke<PublicConfig>('reorder_repository_sources', { ids }),
  saveRepository: (patch: RepositoryPatch) => invoke<PublicConfig>('save_repository', { patch }),
  deleteRepository: (repositoryId: string) => invoke<OperationResult>('delete_repository', { repositoryId }),
  launchNormal: () => invoke<OperationResult>('launch_normal'),
  launchNoMod: () => invoke<OperationResult>('launch_no_mod'),
  restoreMod: () => invoke<OperationResult>('restore_mod'),
  switchWinhttp: (enabled: boolean) => invoke<OperationResult>('switch_winhttp', { enabled }),
  toggleLocalMod: (dllFile: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod', { dllFile, enabled }),
  toggleLocalModPackage: (packageId: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod_package', { packageId, enabled }),
  openDevtools: () => invoke<OperationResult>('open_devtools'),
  resetUi: () => invoke<OperationResult>('reset_ui'),
  openGameDirectory: () => invoke<OperationResult>('open_game_directory'),
  openModDirectory: () => invoke<OperationResult>('open_mod_directory'),
  openModFileLocation: (dllFile: string) => invoke<OperationResult>('open_mod_file_location', { dllFile }),
  readModConfig: (dllFile: string) => invoke<ModConfigDocument>('read_mod_config', { dllFile }),
  saveModConfig: (dllFile: string, revision: string, content: string) => invoke<ModConfigDocument>('save_mod_config', { dllFile, revision, content }),
  readBepInExConfig: () => invoke<ModConfigDocument>('read_bepinex_config'),
  saveBepInExConfig: (revision: string, content: string) => invoke<ModConfigDocument>('save_bepinex_config', { revision, content }),
  openBackupDirectory: (backupPath?: string) => invoke<OperationResult>('open_backup_directory', { backupPath: backupPath ?? null }),
  openReleasePage: (url: string) => invoke<OperationResult>('open_release_page', { url }),
  openUpdateRepository: () => invoke<OperationResult>('open_update_repository'),
  exitApp: () => invoke<void>('exit_app'),
  cancelDownload: () => invoke<OperationResult>('cancel_download'),
  checkAppUpdate: (locale: 'zh-CN' | 'zh-TW' | 'en' | 'ko') => invoke<AppUpdateCheckResult>('check_app_update', { locale }),
  installAppUpdate: () => invoke<OperationResult>('install_app_update'),
  chooseImportFile: () => open({ multiple: false, directory: false, filters: [{ name: 'OC2 仓库 TOML', extensions: ['oc2repo.toml', 'toml'] }] }),
  chooseGameDirectory: () => open({ multiple: false, directory: true }),
  chooseModFile: () => open({ multiple: false, directory: false, filters: [{ name: 'MOD 文件', extensions: ['dll', 'zip'] }] }),
  chooseExportFile: () => save({ defaultPath: 'oc2-repositories.oc2repo.toml', filters: [{ name: 'OC2 仓库 TOML', extensions: ['oc2repo.toml', 'toml'] }] }),
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

export function watchAppUpdateChannel(callback: (progress: AppUpdateChannelEvent) => void): Promise<UnlistenFn> {
  return listen<AppUpdateChannelEvent>('app-update-channel', (event) => callback(event.payload))
}

export function watchAppUpdatePhase(callback: (progress: AppUpdatePhaseEvent) => void): Promise<UnlistenFn> {
  return listen<AppUpdatePhaseEvent>('app-update-phase', (event) => callback(event.payload))
}
