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
export type TutorialLocale = 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja'
export type TutorialScenario = 'live' | 'overview' | 'framework-missing' | 'framework-channel' | 'framework-installing' | 'framework-ready' | 'framework-backup' | 'mod-management' | 'repository-loading' | 'repository-ready' | 'source-manager' | 'repository-editor' | 'repository-install' | 'release-select' | 'download-progress' | 'sponsor' | 'completion'

export interface AssetRule { type: AssetRuleType; value: string }
export interface RepositoryBinding { provider: Provider; owner: string; repository: string; tag: string; channel: Channel }
export interface CommunityDiscovery { repository_ids: string[]; total_count: number; incomplete_results: boolean; truncated: boolean }
export interface RepositoryReadme { content: string; exists: boolean; url: string }
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
/**
 * MOD 与另一个插件之间的关系种类。
 *
 * 依赖、排斥、兼容三件事本质上是同一个问题 ——「对某个 GUID 的版本区间
 * 有什么要求」，只是满足与违反时的含义不同：
 *
 * - `requires`   目标必须存在且版本落在区间内；否则是「缺依赖」
 * - `conflicts`  目标存在且版本落在区间内 → 冲突，需要卸载
 * - `compatible` 目标存在时版本必须落在区间内；否则不兼容，需要卸载
 *
 * 三者共用 `min_version`/`max_version`：不写就是该端不设限制。
 */
export type RelationKind = 'requires' | 'conflicts' | 'compatible'

/** 一条关系声明：对某个 GUID 的版本区间要求。 */
export interface RepositoryRelation {
  /** 省略时按 `requires` 处理（历史写法只写 guid）。 */
  kind?: RelationKind
  guid: string
  /** 版本区间下限（含）；空字符串表示不限。 */
  min_version?: string
  /** 版本区间上限（含）；空字符串表示不限。 */
  max_version?: string
  /**
   * 只对预发布版本成立（三态）。
   *
   * - 不写：正式版与预发布版都算
   * - `true`：只匹配预发布版（四段写法 `1.9.8.1` 即 `1.9.8-beta.1`）
   * - `false`：只匹配正式版
   *
   * 用于「正式版与测试版互斥」：预发布版按 semver 排在**同号正式版之下**，
   * 版本区间无法把 `1.9.8` 和 `1.9.8.1` 分成两类，只能靠这个标志。
   */
  prerelease?: boolean
  /** 仅对 `requires` 有意义：缺失时不提示、不显示按钮。 */
  optional?: boolean
}

/**
 * 按 Release 版本覆盖关系表。
 *
 * 同一 MOD 的不同大版本可以依赖不同版本的基础库。匹配规则：
 * 所有区间命中的条目里取 `min_version` 最高的那条，其 `relations`
 * 整体替换仓库级 `relations`（不合并）。
 */
export interface ReleaseRelationOverride {
  /** 生效的最低 Release 版本（含）；空表示不限。 */
  min_version?: string
  /** 生效的最高 Release 版本（含）；空表示不限。 */
  max_version?: string
  relations: RepositoryRelation[]
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
  /** 这个 MOD 与其它插件的关系（依赖 / 排斥 / 兼容，按 GUID 声明）。 */
  relations?: RepositoryRelation[]
  /** 按 Release 版本覆盖关系表（见 ReleaseRelationOverride）。 */
  release_relations?: ReleaseRelationOverride[]
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
  /** 删除 MOD 时先移入回收站；false = 直接删除，不可恢复。 */
  recycle_deleted_mods: boolean
  tutorial_steps: TutorialStepConfig[]
  game: { path: string; auto_detect: boolean; runtime: GameRuntime }
  network: { timeout_seconds: number; download_threads: number; max_retry_count: number; check_updates_on_start: boolean }
  launch: { ask_user_to_start_from_steam: boolean; auto_restore_winhttp: boolean }
  official_api_only: boolean
  github_token_set: boolean
  gitee_token_set: boolean
  repositories: Repository[]
  repository_sources: RepositorySource[]
  community_discovery?: CommunityDiscovery | null
  gitee_community_discovery?: CommunityDiscovery | null
  github_proxies: GitHubProxyConfig
}
export interface RepositorySource { id: string; nickname: string; nickname_i18n?: Record<string, string>; url: string; region?: string; builtin: boolean; parent_id?: string }
export interface BepInExInfo { installed: boolean; has_residue: boolean; has_config: boolean; version?: string; version_source?: string; backups: string[] }
export type WinhttpState = 'modenabled' | 'moddisabled' | 'missing' | 'conflict'
export interface GameInfo { path?: string; steam_library?: string; steam_root?: string; wine_prefix?: string; game_exists: boolean; bepinex: BepInExInfo; kitchen_running: boolean; winhttp_state: WinhttpState }
export type InstalledModStatusLabel = 'update_available' | 'latest' | 'not_release_asset' | 'unbound' | 'bound_repository_missing' | 'repository_unavailable' | 'checking' | 'unknown' | 'duplicate' | 'duplicate_lower_version' | 'load_warning' | 'disabled' | 'dependency' | 'missing_dependency' | 'dependency_conflict'
export interface InstalledMod { repository_id?: string; repository_ids?: string[]; repository_binding?: RepositoryBinding | null; package_id?: string; package_name?: string; package_name_i18n?: Record<string, string>; package_mod_count?: number; group_i18n?: Record<string, string>; group?: string; prerelease?: boolean; name: string; title?: string; description?: string; product_name?: string; original_filename?: string; guid?: string; has_config?: boolean; has_logs?: boolean; plugin_name?: string; manual_install?: boolean; is_dependency?: boolean; company?: string; copyright?: string; dll_file: string; enabled: boolean; version?: string; version_source: string; update_available: boolean; labels?: InstalledModStatusLabel[]; duplicate_guid: boolean; duplicate_version?: boolean; load_warning?: string; note?: string; status: string }
export interface ModConfigDocument { guid: string; file_name: string; content: string; revision: string }
/** MOD 安装预览：本地文件 / 在线直链在安装前的解析结果。 */
export type ModPreviewRole = 'mod' | 'dependency'
export interface ModPreviewEntry {
  path: string
  role: ModPreviewRole
  name?: string | null
  guid?: string | null
  version?: string | null
  title?: string | null
  product_name?: string | null
  company?: string | null
  copyright?: string | null
  original_filename?: string | null
}
export interface ModPreview {
  preview_id: string
  /** single = 单 mod（恰好 1 个 DLL）；package = 包含 0 个或多个 DLL。 */
  kind: 'single' | 'package'
  package_name: string
  file_name: string
  total_dlls: number
  mod_count: number
  dependency_count: number
  entries: ModPreviewEntry[]
  /** repository = 从在线仓库资产下载；manual = 本地文件或直链。 */
  origin: 'repository' | 'manual'
  /** 仓库来源本次 Release 的版本标签（如 v1.2.3）；手动来源为空串。 */
  release_tag: string
  /** 仓库来源本次 Release 的提交日志（Markdown）；手动来源为空串。 */
  release_body: string
}
export type ModSourceKind = 'local' | 'url'
export type ConfigEditorTarget = { kind: 'mod'; mod: InstalledMod } | { kind: 'bepinex' }
export interface UpdateStatus { repository_id: string; local_version?: string; remote_version?: string; state: string }
export interface Dashboard { config: PublicConfig; game: GameInfo; mods: InstalledMod[]; updates: UpdateStatus[] }
export interface OperationResult { ok: boolean; message: string }
export interface BepInExInstallResult extends OperationResult { wine_override_applied: boolean; wine_override_error?: string; macos_overlay_repaired: boolean }
export interface ModRecycleEntry {
  id: string
  name: string
  /** 删除时仓库名的多语言快照；前端按当前语言现选，缺省回退 `name`。 */
  name_i18n?: Record<string, string>
  deleted_at: string
  file_count: number
  files: string[]
  game_path: string
  package_id?: string
  release_tag?: string
}
export interface UninstallLocalModResult extends OperationResult { requires_confirmation: boolean; requires_package_confirmation?: boolean; package_id?: string; package_mod_names?: string[] }
export interface RustLog { path: string; content: string }
/** 当前进程权限与可写性：应用不再启动即提权，凭这个判断是否需要提权。 */
export interface PermissionStatus {
  elevated: boolean
  game_writable: boolean
  data_writable: boolean
}
/** 仓库安装结果；预览模式下携带解析结果，供安装弹窗展示。 */
export interface RepositoryInstallOutcome extends OperationResult {
  preview?: ModPreview
}
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
export interface AppUpdateInfo { version: string; current_version: string; body?: string; date?: string; download_size?: number; manifest_url: string; installer_url?: string; installer_signature: string; installer_name: string; installer_type?: 'exe' | 'msi' | 'appimage' | 'deb' | 'rpm' | 'app'; provider?: Provider; source_channel?: string }
export interface AppUpdateChannelStatus { id: string; provider: Provider; mode: string; status: string; error?: string }
export interface AppUpdateChannelEvent { region: string; channel: AppUpdateChannelStatus }
export interface AppUpdatePhaseEvent { region: string; phase: 'manifest' | 'download' | 'install' | 'skipped'; message?: string; update?: AppUpdateInfo }
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
export interface SettingsPatch { ui_zoom?: number; onboarding_seen_steps?: string[]; hide_unidentified_mods?: boolean; recycle_deleted_mods?: boolean; tutorial_steps?: TutorialStepConfig[]; game_path?: string; auto_detect?: boolean; game_runtime?: GameRuntime; check_updates_on_start?: boolean; official_api_only?: boolean; timeout_seconds?: number; download_threads?: number }
export interface TokenPatch { github_token: string; gitee_token: string; clear?: boolean }
export interface RepositoryPatch {
  id?: string
  name: string
  name_i18n?: Record<string, string>
  group: string
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
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  /** 这个 MOD 与其它插件的关系（依赖 / 排斥 / 兼容，按 GUID 声明）。 */
  relations?: RepositoryRelation[]
  /** 按 Release 版本覆盖关系表（见 ReleaseRelationOverride）。 */
  release_relations?: ReleaseRelationOverride[]
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
