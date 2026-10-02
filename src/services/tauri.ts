import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import type { AppUpdateInfo, Dashboard, DownloadProgress, GameInfo, ImportPreview, OperationResult, PublicConfig, ReleaseInfo, RepositoryPatch, SettingsPatch, TokenPatch } from '../types'

export const api = {
  dashboard: () => invoke<Dashboard>('get_dashboard'),
  config: () => invoke<PublicConfig>('get_config'),
  saveModNote: (dllFile: string, note: string) => invoke<OperationResult>('save_mod_note', { dllFile, note }),
  installLocalMod: (sourcePath: string) => invoke<OperationResult>('install_local_mod', { sourcePath }),
  uninstallLocalMod: (dllFile: string) => invoke<OperationResult>('uninstall_local_mod', { dllFile }),
  detectGame: () => invoke<GameInfo>('detect_game'),
  setGamePath: (path: string) => invoke<GameInfo>('set_game_path', { path }),
  saveSettings: (patch: SettingsPatch) => invoke<PublicConfig>('save_settings', { patch }),
  saveTokens: (patch: TokenPatch) => invoke<PublicConfig>('save_tokens', { patch }),
  refreshRepository: (repositoryId: string) => invoke<ReleaseInfo[]>('refresh_repository', { repositoryId }),
  releases: (repositoryId: string) => invoke<ReleaseInfo[]>('get_repository_releases', { repositoryId }),
  installBepInEx: (reinstall: boolean) => invoke<OperationResult>('install_bepinex', { reinstall }),
  uninstallBepInEx: () => invoke<OperationResult>('uninstall_bepinex'),
  backupBepInEx: () => invoke<OperationResult>('backup_bepinex'),
  restoreBepInEx: (backupPath: string) => invoke<OperationResult>('restore_bepinex', { backupPath }),
  deleteBepInExBackup: (backupPath: string) => invoke<OperationResult>('delete_bepinex_backup', { backupPath }),
  installRepository: (repositoryId: string, tag?: string, assetName?: string) => invoke<OperationResult>('install_repository', { repositoryId, tag, assetName }),
  uninstallRepository: (repositoryId: string) => invoke<OperationResult>('uninstall_repository', { repositoryId }),
  exportRepositories: (ids: string[] | null, includeBuiltin: boolean) => invoke<string>('export_repositories', { ids, includeBuiltin }),
  exportFile: (path: string, ids: string[] | null, includeBuiltin: boolean) => invoke<OperationResult>('export_repositories_to_file', { path, ids, includeBuiltin }),
  copyRepositories: (ids: string[] | null, includeBuiltin: boolean) => invoke<OperationResult>('copy_repositories', { ids, includeBuiltin }),
  readClipboard: () => invoke<string>('read_clipboard'),
  readTextFile: (path: string) => invoke<string>('read_text_file', { path }),
  previewImport: (text: string) => invoke<ImportPreview>('preview_repository_import', { text }),
  importRepositories: (text: string, conflict: string) => invoke<ImportPreview>('import_repositories', { text, conflict }),
  saveRepository: (patch: RepositoryPatch) => invoke<PublicConfig>('save_repository', { patch }),
  deleteRepository: (repositoryId: string) => invoke<OperationResult>('delete_repository', { repositoryId }),
  launchNormal: () => invoke<OperationResult>('launch_normal'),
  launchNoMod: () => invoke<OperationResult>('launch_no_mod'),
  restoreMod: () => invoke<OperationResult>('restore_mod'),
  switchWinhttp: (enabled: boolean) => invoke<OperationResult>('switch_winhttp', { enabled }),
  openDevtools: () => invoke<OperationResult>('open_devtools'),
  resetUi: () => invoke<OperationResult>('reset_ui'),
  openGameDirectory: () => invoke<OperationResult>('open_game_directory'),
  openBackupDirectory: () => invoke<OperationResult>('open_backup_directory'),
  openReleasePage: (url: string) => invoke<OperationResult>('open_release_page', { url }),
  exitApp: () => invoke<void>('exit_app'),
  cancelDownload: () => invoke<OperationResult>('cancel_download'),
  checkAppUpdate: () => invoke<AppUpdateInfo | null>('check_app_update'),
  installAppUpdate: () => invoke<OperationResult>('install_app_update'),
  chooseImportFile: () => open({ multiple: false, directory: false, filters: [{ name: 'OC2 仓库 TOML', extensions: ['oc2repo.toml', 'toml'] }] }),
  chooseGameDirectory: () => open({ multiple: false, directory: true }),
  chooseModFile: () => open({ multiple: false, directory: false, filters: [{ name: 'MOD 文件', extensions: ['dll', 'zip'] }] }),
  chooseExportFile: () => save({ defaultPath: 'oc2-repositories.oc2repo.toml', filters: [{ name: 'OC2 仓库 TOML', extensions: ['oc2repo.toml', 'toml'] }] }),
}

export function watchDownloadProgress(callback: (progress: DownloadProgress) => void): Promise<UnlistenFn> {
  return listen<DownloadProgress>('download-progress', (event) => callback(event.payload))
}
