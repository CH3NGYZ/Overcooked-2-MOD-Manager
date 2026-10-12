export type Provider = 'github' | 'gitee'
export type AppPlatform = 'windows' | 'linux' | 'macos'
export type GameRuntime = 'wine' | 'native'
export type Channel = 'stable' | 'beta' | 'all'
export type ProxyMode = 'direct' | 'auto_direct' | 'auto_proxy' | 'fixed'
export type RepositoryDownloadMode = 'release' | 'repository_file'
export type AssetRuleType = 'exact' | 'glob' | 'regex'
export interface GitHubProxy { id: string; name: string; template: string; regions: string[]; description: string; enabled: boolean; builtin: boolean; remote: boolean }
export interface GitHubProxyConfig {
  release_proxies: GitHubProxy[]
  raw_proxies: GitHubProxy[]
  github_api_proxies: GitHubProxy[]
  gitee_api_proxies: GitHubProxy[]
}
export type TutorialLocale = 'zh-CN' | 'zh-TW' | 'en' | 'ko'
export type TutorialScenario = 'live' | 'overview' | 'framework-missing' | 'framework-channel' | 'framework-installing' | 'framework-ready' | 'framework-backup' | 'mod-management' | 'repository-loading' | 'repository-ready' | 'source-manager' | 'repository-editor' | 'repository-install' | 'release-select' | 'download-progress' | 'sponsor' | 'completion'

export interface AssetRule { type: AssetRuleType; value: string }
export interface ReleaseAsset { name: string; size: number; download_count?: number | null; download_url: string; digest?: string }
export interface ReleaseInfo {
  release_id?: number
  tag_name: string
  version?: string
  name: string
  name_i18n?: Record<string, string>
  body: string
  published_at?: string
  prerelease: boolean
  draft: boolean
  assets: ReleaseAsset[]
}
export interface Repository {
  id: string
  name: string
  name_i18n?: Record<string, string>
  group?: string
  group_i18n?: Record<string, string>
  provider: Provider
  username: string
  repository: string
  download_mode: RepositoryDownloadMode
  branch: string
  file_path: string
  region?: string
  channel: Channel
  asset_rules: AssetRule[]
  install_target: string
  note: string
  note_i18n?: Record<string, string>
  source_id?: string
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  version_regex: string
  max_release_count: number
  download_threads: number
  github_token_set?: boolean
  gitee_token_set?: boolean
  enabled: boolean
  builtin: boolean
  pinned_version?: string
  ignored_version?: string
  cached_releases: ReleaseInfo[]
  last_error?: string
}
// Read-only bundled configuration used to seed isolated tutorial scenes.
export type TutorialRepositoryConfig = Omit<Repository, 'id' | 'builtin' | 'cached_releases'>
export interface PublicConfig {
  schema_version: number
  platform: AppPlatform
  region: string
  ui_zoom: number
  onboarding_seen_steps: string[]
  hide_unidentified_mods: boolean
  tutorial_steps: TutorialStepConfig[]
  game: { path: string; auto_detect: boolean; runtime: GameRuntime }
  network: { timeout_seconds: number; download_threads: number; max_retry_count: number; check_updates_on_start: boolean }
  launch: { ask_user_to_start_from_steam: boolean; auto_restore_winhttp: boolean }
  official_api_only: boolean
  github_token_set: boolean
  gitee_token_set: boolean
  repositories: Repository[]
  repository_sources: RepositorySource[]
  github_proxies: GitHubProxyConfig
}
export interface RepositorySource { id: string; nickname: string; nickname_i18n?: Record<string, string>; url: string; region?: string; builtin: boolean; parent_id?: string }
export interface BepInExInfo { installed: boolean; complete: boolean; has_config: boolean; version?: string; version_source?: string; backups: string[] }
export type WinhttpState = 'modenabled' | 'moddisabled' | 'missing' | 'conflict'
export interface GameInfo { path?: string; steam_library?: string; steam_root?: string; wine_prefix?: string; game_exists: boolean; bepinex: BepInExInfo; kitchen_running: boolean; winhttp_state: WinhttpState }
export interface InstalledMod { repository_id?: string; repository_ids?: string[]; package_id?: string; package_name?: string; package_mod_count?: number; name: string; title?: string; description?: string; product_name?: string; original_filename?: string; guid?: string; has_config?: boolean; plugin_name?: string; company?: string; copyright?: string; dll_file: string; enabled: boolean; version?: string; version_source: string; update_available: boolean; duplicate_guid: boolean; duplicate_version?: boolean; load_warning?: string; note?: string; status: string }
export interface ModConfigDocument { guid: string; file_name: string; content: string; revision: string }
export type ConfigEditorTarget = { kind: 'mod'; mod: InstalledMod } | { kind: 'bepinex' }
export interface UpdateStatus { repository_id: string; local_version?: string; remote_version?: string; state: string }
export interface Dashboard { config: PublicConfig; game: GameInfo; mods: InstalledMod[]; updates: UpdateStatus[] }
export interface OperationResult { ok: boolean; message: string }
export interface BepInExInstallResult extends OperationResult { wine_override_applied: boolean; wine_override_error?: string }
export interface ModRecycleEntry {
  id: string
  name: string
  deleted_at: string
  file_count: number
  files: string[]
  game_path: string
  package_id?: string
  release_tag?: string
}
export interface UninstallLocalModResult extends OperationResult { requires_confirmation: boolean; requires_package_confirmation?: boolean; package_id?: string; package_mod_names?: string[] }
export interface RustLog { path: string; content: string }
export interface DownloadProgress {
  label: string
  downloaded: number
  total?: number
  speed_bytes: number
  finished: boolean
  phase?: 'connecting' | 'switching' | 'downloading' | 'finished'
  route?: 'direct' | 'proxy'
  source?: string
  attempt?: number
  attempt_total?: number
  parallel_sources?: number
}
export interface ImportPreview { valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] }
export interface RepositorySourceImportResult {
  preview: ImportPreview | null
  access_error: string | null
  builtin_fallback_available: boolean
}
export interface AppUpdateInfo { version: string; current_version: string; body?: string; date?: string; download_size?: number; manifest_endpoints?: string[]; installer_url?: string; installer_proxy_url?: string; installer_download_urls?: string[]; installer_digest?: string; installer_name: string; installer_signature_url?: string; installer_type?: 'exe' | 'msi' | 'appimage' | 'deb' | 'rpm' | 'app'; provider?: Provider; source_channel?: string }
export interface AppUpdateChannelStatus { id: string; provider: Provider; mode: string; status: string; error?: string }
export interface AppUpdateChannelEvent { region: string; channel: AppUpdateChannelStatus }
export interface AppUpdatePhaseEvent { region: string; phase: 'api' | 'download' | 'install' | 'skipped'; message?: string; update?: AppUpdateInfo }
export interface AppUpdateCheckResult { update?: AppUpdateInfo; channels: AppUpdateChannelStatus[]; all_failed: boolean; region: string }
export interface TutorialStepConfig {
  id: string
  selector: string
  view?: string
  scenario?: TutorialScenario
  title: string
  description: string
  title_i18n?: Partial<Record<TutorialLocale, string>>
  description_i18n?: Partial<Record<TutorialLocale, string>>
}
export interface SettingsPatch { ui_zoom?: number; onboarding_seen_steps?: string[]; hide_unidentified_mods?: boolean; tutorial_steps?: TutorialStepConfig[]; game_path?: string; auto_detect?: boolean; game_runtime?: GameRuntime; check_updates_on_start?: boolean; official_api_only?: boolean; timeout_seconds?: number; download_threads?: number }
export interface TokenPatch { github_token: string; gitee_token: string }
export interface RepositoryPatch {
  id?: string
  name: string
  group: string
  provider: Provider
  username: string
  repository: string
  download_mode: RepositoryDownloadMode
  branch: string
  file_path: string
  region?: string
  channel: Channel
  asset_rules: AssetRule[]
  install_target: string
  note: string
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  version_regex: string
  max_release_count: number
  download_threads: number
  github_token?: string
  gitee_token?: string
  official_api_only?: boolean
  enabled: boolean
}
export interface GitHubProxyPatch {
  release_proxies: GitHubProxy[]
  raw_proxies: GitHubProxy[]
  github_api_proxies: GitHubProxy[]
  gitee_api_proxies: GitHubProxy[]
}
