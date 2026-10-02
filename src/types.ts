export type Provider = 'github' | 'gitee'
export type Channel = 'stable' | 'beta' | 'all'
export type ProxyMode = 'direct' | 'auto' | 'fixed'
export type AssetRuleType = 'exact' | 'glob' | 'regex'

export interface AssetRule { type: AssetRuleType; value: string }
export interface ReleaseAsset { name: string; size: number; download_url: string; digest?: string }
export interface ReleaseInfo {
  tag_name: string
  version?: string
  name: string
  body: string
  published_at?: string
  prerelease: boolean
  draft: boolean
  assets: ReleaseAsset[]
}
export interface Repository {
  id: string
  name: string
  provider: Provider
  url: string
  channel: Channel
  asset_rules: AssetRule[]
  install_target: string
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  download_threads: number
  api_proxy_mode: ProxyMode
  api_proxy_url: string
  release_proxy_mode: ProxyMode
  release_proxy_url: string
  github_token_set?: boolean
  gitee_token_set?: boolean
  enabled: boolean
  builtin: boolean
  pinned_version?: string
  ignored_version?: string
  cached_releases: ReleaseInfo[]
  last_error?: string
}
export interface PublicConfig {
  schema_version: number
  game: { path: string; auto_detect: boolean }
  network: { timeout_seconds: number; download_threads: number; max_retry_count: number; check_updates_on_start: boolean; max_release_count: number }
  launch: { ask_user_to_start_from_steam: boolean; auto_restore_winhttp: boolean }
  official_api_only: boolean
  github_token_set: boolean
  gitee_token_set: boolean
  repositories: Repository[]
}
export interface BepInExInfo { installed: boolean; complete: boolean; version?: string; version_source?: string; backups: string[] }
export type WinhttpState = 'modenabled' | 'moddisabled' | 'missing' | 'conflict'
export interface GameInfo { path?: string; steam_library?: string; game_exists: boolean; bepinex: BepInExInfo; kitchen_running: boolean; winhttp_state: WinhttpState }
export interface InstalledMod { repository_id?: string; name: string; product_name?: string; original_filename?: string; guid?: string; plugin_name?: string; dll_file: string; version?: string; version_source: string; update_available: boolean; duplicate_guid: boolean; load_warning?: string; note?: string; status: string }
export interface UpdateStatus { repository_id: string; local_version?: string; remote_version?: string; state: string }
export interface Dashboard { config: PublicConfig; game: GameInfo; mods: InstalledMod[]; updates: UpdateStatus[] }
export interface OperationResult { ok: boolean; message: string }
export interface RustLog { path: string; content: string }
export interface DownloadProgress { label: string; downloaded: number; total?: number; speed_bytes: number; finished: boolean }
export interface ImportPreview { valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] }
export interface AppUpdateInfo { version: string; current_version: string; body?: string; date?: string; download_size?: number; manifest_endpoints?: string[] }
export interface SettingsPatch { game_path?: string; auto_detect?: boolean; check_updates_on_start?: boolean; official_api_only?: boolean; timeout_seconds?: number; download_threads?: number; max_release_count?: number }
export interface TokenPatch { github_token: string; gitee_token: string }
export interface RepositoryPatch {
  id?: string
  name: string
  provider: Provider
  url: string
  channel: Channel
  asset_rules: AssetRule[]
  install_target: string
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  download_threads: number
  api_proxy_mode: ProxyMode
  api_proxy_url: string
  release_proxy_mode: ProxyMode
  release_proxy_url: string
  github_token?: string
  gitee_token?: string
  official_api_only?: boolean
  enabled: boolean
}
