use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameInfo {
    pub path: Option<String>,
    pub steam_library: Option<String>,
    #[serde(default)]
    pub steam_root: Option<String>,
    #[serde(default)]
    pub wine_prefix: Option<String>,
    pub game_exists: bool,
    pub bepinex: BepInExInfo,
    pub kitchen_running: bool,
    pub winhttp_state: WinhttpState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BepInExInfo {
    pub installed: bool,
    pub complete: bool,
    #[serde(default)]
    pub has_config: bool,
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
    #[serde(default)]
    pub repository_binding: Option<RepositoryBinding>,
    #[serde(default)]
    pub package_id: Option<String>,
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub package_mod_count: usize,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub product_name: Option<String>,
    pub original_filename: Option<String>,
    pub guid: Option<String>,
    #[serde(default)]
    pub has_config: bool,
    pub plugin_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub dll_file: String,
    pub enabled: bool,
    pub version: Option<String>,
    pub version_source: String,
    pub update_available: bool,
    pub duplicate_guid: bool,
    #[serde(default)]
    pub duplicate_version: bool,
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
    #[serde(default)]
    pub download_count: Option<u64>,
    pub download_url: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitee,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
    #[default]
    All,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryDownloadMode {
    #[default]
    Release,
    RepositoryFile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryBinding {
    pub provider: Provider,
    pub owner: String,
    pub repository: String,
    pub tag: String,
    #[serde(default)]
    pub channel: Channel,
}

impl RepositoryBinding {
    pub fn from_release(repo: &RepositoryConfig, tag: &str) -> Option<Self> {
        if repo.download_mode != RepositoryDownloadMode::Release || tag.trim().is_empty() {
            return None;
        }
        let binding = Self {
            provider: repo.provider.clone(),
            owner: repo.username.clone(),
            repository: repo.repository.clone(),
            tag: tag.into(),
            channel: repo.channel,
        };
        binding.is_valid().then_some(binding)
    }

    pub fn is_valid(&self) -> bool {
        !self.tag.trim().is_empty()
            && !self.tag.chars().any(char::is_control)
            && crate::releases::repository_coordinates(&self.owner, &self.repository).is_ok()
    }

    pub fn matches_identity(&self, repo: &RepositoryConfig) -> bool {
        self.provider == repo.provider
            && self.owner.eq_ignore_ascii_case(&repo.username)
            && self.repository.eq_ignore_ascii_case(&repo.repository)
    }

    pub fn matches_repository(&self, repo: &RepositoryConfig) -> bool {
        self.matches_identity(repo)
            && repo.download_mode == RepositoryDownloadMode::Release
            && (repo.channel == self.channel || repo.channel == Channel::All)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommunityDiscovery {
    pub repository_ids: Vec<String>,
    pub total_count: u64,
    pub incomplete_results: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    Direct,
    AutoDirect,
    AutoProxy,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubProxy {
    pub id: String,
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    /// A managed proxy discovered in a userscript, rather than the TOML defaults.
    #[serde(default)]
    pub remote: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubProxyConfig {
    #[serde(default)]
    pub release_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubProxyManifest {
    #[serde(default = "default_proxy_manifest_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub release_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
}

fn default_proxy_manifest_version() -> u32 {
    1
}

impl Default for ProxyMode {
    fn default() -> Self {
        Self::AutoDirect
    }
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
    pub username: String,
    pub repository: String,
    #[serde(default)]
    pub download_mode: RepositoryDownloadMode,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub channel: Channel,
    #[serde(default)]
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
    /// Optional regular expression applied to the installed MOD version when
    /// binding a local plugin to this repository.
    #[serde(default)]
    pub version_regex: String,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_discovery: Option<CommunityDiscovery>,
    // Identity only: no source documents, repository settings or Release cache.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_repository_ids: BTreeMap<String, String>,
    #[serde(default = "crate::github_proxies::bundled_defaults")]
    pub github_proxies: GitHubProxyConfig,
    #[serde(default)]
    pub mod_notes: Vec<ModNote>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GameRuntime {
    #[default]
    Wine,
    Native,
}

fn default_game_runtime() -> GameRuntime {
    if cfg!(windows) {
        GameRuntime::Native
    } else {
        GameRuntime::Wine
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameConfig {
    pub path: String,
    pub auto_detect: bool,
    #[serde(default = "default_game_runtime")]
    pub runtime: GameRuntime,
}

impl GameConfig {
    pub(crate) fn switch_runtime(&mut self, runtime: GameRuntime) {
        if self.runtime != runtime {
            self.runtime = runtime;
            self.path.clear();
            self.auto_detect = true;
        }
    }
}

#[cfg(test)]
mod game_runtime_tests {
    use super::*;

    #[test]
    fn fresh_game_settings_default_to_wine_only_on_unix() {
        let settings: GameConfig =
            serde_json::from_str(r#"{"path":"","auto_detect":true}"#).unwrap();
        assert_eq!(
            settings.runtime,
            if cfg!(windows) {
                GameRuntime::Native
            } else {
                GameRuntime::Wine
            }
        );
    }

    #[test]
    fn switching_runtime_clears_only_the_selected_path_and_reenables_detection() {
        let mut settings = GameConfig {
            path: "old Windows game directory".into(),
            auto_detect: false,
            runtime: GameRuntime::Wine,
        };
        settings.switch_runtime(GameRuntime::Wine);
        assert_eq!(settings.path, "old Windows game directory");
        assert!(!settings.auto_detect);
        settings.switch_runtime(GameRuntime::Native);
        assert!(settings.path.is_empty());
        assert!(settings.auto_detect);
        let persisted = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<GameConfig>(&persisted).unwrap(),
            settings
        );
        settings.path = "native game directory".into();
        settings.switch_runtime(GameRuntime::Wine);
        assert!(settings.path.is_empty());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub timeout_seconds: u64,
    pub download_threads: u32,
    pub max_retry_count: u32,
    pub check_updates_on_start: bool,
}
fn default_max_release_count() -> u32 {
    10
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub title_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub description_i18n: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialDocument {
    #[serde(default = "default_tutorial_version")]
    pub version: u32,
    #[serde(default)]
    pub steps: Vec<TutorialStepConfig>,
}

fn default_tutorial_version() -> u32 {
    1
}

fn default_hide_unidentified_mods() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConfig {
    pub schema_version: u32,
    pub platform: String,
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
    #[serde(default)]
    pub community_discovery: Option<CommunityDiscovery>,
    #[serde(default)]
    pub github_proxies: GitHubProxyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySource {
    pub id: String,
    pub nickname: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nickname_i18n: BTreeMap<String, String>,
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
    pub provider: Provider,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub username: String,
    pub repository: String,
    pub enabled: bool,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub download_mode: RepositoryDownloadMode,
    #[serde(default)]
    pub channel: Channel,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    #[serde(default)]
    pub version_regex: String,
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
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
pub struct RepositorySourceImportResult {
    pub preview: Option<ImportPreview>,
    pub access_error: Option<String>,
    pub builtin_fallback_available: bool,
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

#[derive(Debug, Clone, Serialize)]
pub struct BepInExInstallResult {
    #[serde(flatten)]
    pub operation: OperationResult,
    pub wine_override_applied: bool,
    pub wine_override_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRecycleEntry {
    pub id: String,
    pub name: String,
    pub deleted_at: String,
    pub file_count: usize,
    pub files: Vec<String>,
    pub game_path: String,
    pub package_id: Option<String>,
    pub release_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallLocalModResult {
    pub ok: bool,
    pub message: String,
    pub requires_confirmation: bool,
    #[serde(default)]
    pub requires_package_confirmation: bool,
    #[serde(default)]
    pub package_id: Option<String>,
    #[serde(default)]
    pub package_mod_names: Vec<String>,
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
    pub installer_digest: Option<String>,
    #[serde(default)]
    pub installer_name: String,
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
pub struct AppUpdatePhaseEvent {
    pub region: String,
    pub phase: String,
    pub message: Option<String>,
    pub update: Option<AppUpdateInfo>,
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
    pub game_runtime: Option<GameRuntime>,
    pub check_updates_on_start: Option<bool>,
    pub official_api_only: Option<bool>,
    pub timeout_seconds: Option<u64>,
    pub download_threads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubProxyPatch {
    pub release_proxies: Vec<GitHubProxy>,
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
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
    pub username: String,
    pub repository: String,
    #[serde(default)]
    pub download_mode: RepositoryDownloadMode,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
    pub version_regex: String,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub github_token: String,
    #[serde(default)]
    pub gitee_token: String,
    #[serde(default = "default_true")]
    pub official_api_only: bool,
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
fn default_download_threads() -> u32 {
    0
}
