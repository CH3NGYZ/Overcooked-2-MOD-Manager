use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub path: Option<String>,
    pub steam_library: Option<String>,
    pub game_exists: bool,
    pub bepinex: BepInExInfo,
    pub kitchen_running: bool,
    pub winhttp_state: WinhttpState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BepInExInfo {
    pub installed: bool,
    pub complete: bool,
    pub version: Option<String>,
    pub version_source: Option<String>,
    pub backups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WinhttpState {
    ModEnabled,
    ModDisabled,
    Missing,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub repository_id: Option<String>,
    #[serde(default)]
    pub repository_ids: Vec<String>,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub product_name: Option<String>,
    pub original_filename: Option<String>,
    pub guid: Option<String>,
    pub plugin_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub dll_file: String,
    pub enabled: bool,
    pub version: Option<String>,
    pub version_source: String,
    pub update_available: bool,
    pub duplicate_guid: bool,
    pub load_warning: Option<String>,
    pub note: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModNote {
    pub key: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    #[serde(default)]
    pub release_id: Option<u64>,
    pub tag_name: String,
    pub version: Option<String>,
    pub name: String,
    pub body: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub size: u64,
    pub download_url: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitee,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    Direct,
    Auto,
    Fixed,
}

impl Default for ProxyMode {
    fn default() -> Self { Self::Auto }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssetRuleType {
    Exact,
    Glob,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRule {
    #[serde(rename = "type")]
    pub rule_type: AssetRuleType,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    pub provider: Provider,
    pub url: String,
    pub channel: Channel,
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub api_proxy_mode: ProxyMode,
    #[serde(default)]
    pub api_proxy_url: String,
    #[serde(default)]
    pub release_proxy_mode: ProxyMode,
    #[serde(default)]
    pub release_proxy_url: String,
    #[serde(default)]
    pub github_token: String,
    #[serde(default)]
    pub gitee_token: String,
    #[serde(default = "default_true")]
    pub official_api_only: bool,
    pub enabled: bool,
    pub builtin: bool,
    #[serde(default)]
    pub pinned_version: Option<String>,
    #[serde(default)]
    pub ignored_version: Option<String>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub cached_releases: Vec<ReleaseInfo>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub ui_zoom: f64,
    pub onboarding_seen_steps: Vec<String>,
    #[serde(default = "default_hide_unidentified_mods")]
    pub hide_unidentified_mods: bool,
    #[serde(default)]
    pub tutorial_steps: Vec<TutorialStepConfig>,
    pub game: GameConfig,
    pub network: NetworkConfig,
    pub launch: LaunchConfig,
    pub auth: AuthConfig,
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
    #[serde(default)]
    pub mod_notes: Vec<ModNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub path: String,
    pub auto_detect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub timeout_seconds: u64,
    pub download_threads: u32,
    pub max_retry_count: u32,
    pub check_updates_on_start: bool,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
}
fn default_max_release_count() -> u32 { 10 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub ask_user_to_start_from_steam: bool,
    pub auto_restore_winhttp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub github_token: String,
    pub gitee_token: String,
    pub official_api_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialStepConfig {
    pub id: String,
    pub selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    pub title: String,
    pub description: String,
}

fn default_hide_unidentified_mods() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConfig {
    pub schema_version: u32,
    pub region: String,
    pub ui_zoom: f64,
    pub onboarding_seen_steps: Vec<String>,
    pub hide_unidentified_mods: bool,
    #[serde(default)]
    pub tutorial_steps: Vec<TutorialStepConfig>,
    pub game: GameConfig,
    pub network: NetworkConfig,
    pub launch: LaunchConfig,
    pub official_api_only: bool,
    pub github_token_set: bool,
    pub gitee_token_set: bool,
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySource {
    pub id: String,
    pub nickname: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryExport {
    pub name: String,
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    pub provider: Provider,
    pub url: String,
    pub channel: Channel,
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub api_proxy_mode: ProxyMode,
    #[serde(default)]
    pub api_proxy_url: String,
    #[serde(default)]
    pub release_proxy_mode: ProxyMode,
    #[serde(default)]
    pub release_proxy_url: String,
    pub enabled: bool,
    #[serde(default)]
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryBundle {
    pub format: String,
    pub schema_version: u32,
    pub exported_at: String,
    pub repositories: Vec<RepositoryExport>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub valid: bool,
    pub errors: Vec<String>,
    pub additions: Vec<String>,
    pub duplicates: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub repository_id: String,
    pub local_version: Option<String>,
    pub remote_version: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallLocalModResult {
    pub ok: bool,
    pub message: String,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustLog {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
    pub download_size: Option<u64>,
    #[serde(default)]
    pub manifest_endpoints: Vec<String>,
    #[serde(default)]
    pub installer_url: Option<String>,
    #[serde(default)]
    pub installer_proxy_url: Option<String>,
    #[serde(default)]
    pub installer_download_urls: Vec<String>,
    #[serde(default)]
    pub installer_type: String,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub source_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateChannelStatus {
    pub id: String,
    pub provider: Provider,
    pub mode: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateChannelEvent {
    pub region: String,
    pub channel: AppUpdateChannelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateCheckResult {
    pub update: Option<AppUpdateInfo>,
    pub channels: Vec<AppUpdateChannelStatus>,
    pub all_failed: bool,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub config: PublicConfig,
    pub game: GameInfo,
    pub mods: Vec<InstalledMod>,
    pub updates: Vec<UpdateStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub ui_zoom: Option<f64>,
    pub onboarding_seen_steps: Option<Vec<String>>,
    pub hide_unidentified_mods: Option<bool>,
    pub tutorial_steps: Option<Vec<TutorialStepConfig>>,
    pub game_path: Option<String>,
    pub auto_detect: Option<bool>,
    pub check_updates_on_start: Option<bool>,
    pub official_api_only: Option<bool>,
    pub timeout_seconds: Option<u64>,
    pub download_threads: Option<u32>,
    pub max_release_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPatch {
    pub github_token: String,
    pub gitee_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPatch {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub group: String,
    pub provider: Provider,
    pub url: String,
    pub channel: Channel,
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub api_proxy_mode: ProxyMode,
    #[serde(default)]
    pub api_proxy_url: String,
    #[serde(default)]
    pub release_proxy_mode: ProxyMode,
    #[serde(default)]
    pub release_proxy_url: String,
    #[serde(default)]
    pub github_token: String,
    #[serde(default)]
    pub gitee_token: String,
    #[serde(default = "default_true")]
    pub official_api_only: bool,
    pub enabled: bool,
}

fn default_true() -> bool { true }
fn default_download_threads() -> u32 { 0 }
