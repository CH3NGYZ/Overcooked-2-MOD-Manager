use super::game::{GameConfig, GameRuntime};
use super::maps::MapRepositoryConfig;
use super::mods::ModNote;
use super::network::{GitHubProxyConfig, NetworkConfig};
use super::{
    CommunityDiscovery, CommunityMapRepository, ModRepositoryConfig, RepositorySource,
    StagedReleaseCache,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(skip)]
    pub(crate) network_policy: Option<std::sync::Arc<crate::network_policy::NetworkPolicy>>,
    pub schema_version: u32,
    #[serde(default = "crate::config::default_ui_zoom")]
    pub ui_zoom: f64,
    #[serde(default = "default_hide_unidentified_mods")]
    pub hide_unidentified_mods: bool,
    /// 删除 MOD（含覆盖安装前清理旧包）时是否先移入回收站。
    ///
    /// 默认 `true`：先回收并保留恢复记录；失败及恢复边界见 recycle_bin 实现。
    /// 设成 `false` 后回收成功再清理，删除不可恢复；设置界面提供风险提示与二次确认。
    #[serde(default = "default_true")]
    pub recycle_deleted_mods: bool,
    /// 读取 MOD 元数据（DLL）时最多同时处理几个文件。
    ///
    /// `0` = 全部 CPU 核心（默认）。用户可存任意正整数，真正执行前按当前机器
    /// 的核心数夹紧（[`crate::concurrency::resolve_concurrency`]），所以配置
    /// 换机器仍然安全。
    #[serde(default)]
    pub mod_metadata_concurrency: u32,
    /// 解析地图 `info` 时最多同时处理几份。
    ///
    /// `0` = 全部 CPU 核心（默认，[`crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY`]）。
    #[serde(default = "default_map_info_concurrency")]
    pub map_info_concurrency: u32,
    /// LZMA 转 LZ4 并行上限；0 表示全部 CPU 核心。
    #[serde(default)]
    pub map_compression_concurrency: u32,
    /// 暖暖缩略图场景并行准备上限；0 表示全部 CPU 核心。
    #[serde(default)]
    pub diychef_preview_concurrency: u32,
    /// 已经看过的教程步骤 id（前端步骤表的 `id`）。
    ///
    /// 教程步骤是只增不改的清单：更新后新加的步骤 id 不在这个集合里，
    /// 启动时只播这些新步骤，老步骤不再重复。id 由前端维护、后端只做
    /// 并集与保存，所以这里不校验「是否存在这个步骤」—— 校验只会让
    /// 删改过步骤表的版本把历史悄悄抹掉。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tutorial_watched_steps: Vec<String>,
    pub game: GameConfig,
    pub network: NetworkConfig,
    pub launch: LaunchConfig,
    pub auth: AuthConfig,
    pub repositories: Vec<ModRepositoryConfig>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
    /// 地图仓库（DIYLevel 关卡集的远程来源）。
    ///
    /// 与 `repositories` 平行而非合并：两者的绑定身份、安装单元都不同
    /// （见 `MapRepositoryConfig` 的说明）。源树复用 `repository_sources`
    /// 的地址解析与相对路径规则。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_repositories: Vec<MapRepositoryConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_discovery: Option<CommunityDiscovery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitee_community_discovery: Option<CommunityDiscovery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitcode_community_discovery: Option<CommunityDiscovery>,
    /// 社区地图发现：带 `map` topic 的仓库（两个平台各一份，发现时整体替换）。
    ///
    /// 只存坐标而不存条目：地图条目依赖各 Release note 里的 `oc2-map` 块，
    /// 每次地图源刷新都要重新读一遍，缓存下来反而会有「作者删了版本、本地
    /// 还留着」的陈旧数据。所以这里没有缓存与开关，也不需要迁移旧配置。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub community_map_repositories: Vec<CommunityMapRepository>,
    // Identity only: no source documents, repository settings or Release cache.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_repository_ids: BTreeMap<String, String>,
    /// 整批刷新清掉仓库前，暂存的 Release 缓存（`repository_id` → 缓存）。
    ///
    /// 刷新流程是「先清空整棵源的仓库 → 再拉源文档 → 按源文档重建仓库」。
    /// 重建出来的仓库是全新的，`cached_releases` 为空，于是 5 分钟缓存
    /// **永远命中不了** —— 每次刷新都把整棵仓库树的 API 重打一遍（实测
    /// 20+ 个仓库、几十个请求，还固定撞速率限制）。
    ///
    /// 清理时挪入，源应用时按原 ID 逐项取回；未认领项仍随配置保存，
    /// 没有批次末统一清表。新鲜度仍由各仓库的 `releases_cached_at` 判定。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub staged_release_cache: BTreeMap<String, StagedReleaseCache>,
    #[serde(skip, default = "crate::github_proxies::bundled_defaults")]
    pub github_proxies: GitHubProxyConfig,
    #[serde(default)]
    pub mod_notes: Vec<ModNote>,
    /// Session selection; every new process initializes it from the computer region.
    #[serde(skip)]
    pub data_provider: super::DataProvider,
}

impl AppConfig {
    /// 把「已经看过」的教程步骤 id 并进配置。
    ///
    /// 并集而不是覆盖：重播与跳步可能在不同次运行里交错回写，谁的 id 都
    /// 不该丢。顺序保留首次写入的先后，空白与重复值直接跳过。
    pub(crate) fn record_tutorial_steps(&mut self, ids: &[String]) {
        for id in ids {
            let id = id.trim();
            if id.is_empty() || self.tutorial_watched_steps.iter().any(|seen| seen == id) {
                continue;
            }
            self.tutorial_watched_steps.push(id.to_owned());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub ask_user_to_start_from_steam: bool,
    pub auto_restore_winhttp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub gitcode_publish: super::publish::PublishCredentialConfig,
    pub github_token: String,
    pub gitee_token: String,
    #[serde(default)]
    pub gitcode_token: String,
    pub official_api_only: bool,
    /// 地图发布页的 GitHub App access token；Windows 使用 DPAPI，非 Windows 仅 base64 编码。
    ///
    /// 单独存一份而不是复用 `github_token`：App 令牌只覆盖安装时勾选的仓库，
    /// 直接覆盖用户手填的 PAT 会把读 API 的权限悄悄降下来；两份并存时读路径
    /// 继续用用户的 PAT，发布路径只用这一份。
    #[serde(default)]
    pub github_publish_token: String,
    /// 配对的 refresh token（保护方式同上）；App 关闭令牌过期时为空。
    ///
    /// GitHub 每次刷新都会**同时轮换**两者：旧 refresh token 立即作废，所以新的一对
    /// 必须一起落盘，不能只换 access token。
    #[serde(default)]
    pub github_publish_refresh_token: String,
    /// access token 的过期时刻（Unix 秒）；`None` = 令牌不过期或旧配置没记。
    #[serde(default)]
    pub github_publish_token_expires_at: Option<i64>,
    /// refresh token 的过期时刻（Unix 秒）；过了就只能重新授权。
    #[serde(default)]
    pub github_publish_refresh_expires_at: Option<i64>,
    /// 授权到的 GitHub 登录名，只用于界面显示「以谁的身份发布」。
    #[serde(default)]
    pub github_publish_login: String,
    /// 上次安装 App 时回调带回来的 installation id；0 = 还不知道。
    ///
    /// 存它只是为了绕开一件事：`/user/installations` 是最终一致的，刚在 GitHub
    /// 上勾完仓库时它能空几十秒（实测退出重登后有 25 秒空窗），而按 id 直查
    /// `/user/installations/{id}/repositories` 立刻就是用户刚勾的那一份。
    /// 退出登录不清它：用户在 GitHub 上改仓库范围时这个 id 不会变。
    #[serde(default)]
    pub github_publish_installation_id: u64,
}

fn default_hide_unidentified_mods() -> bool {
    true
}

/// 解析地图 `info` 的默认并发上限。
///
/// 配置默认值与运行时解析共用 `metadata_read` 常量；当前 `0` 表示全部 CPU 核心。
fn default_map_info_concurrency() -> u32 {
    crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConfig {
    pub schema_version: u32,
    pub platform: String,
    pub region: String,
    pub ui_zoom: f64,
    pub hide_unidentified_mods: bool,
    /// 删除 MOD 时先移入回收站；false = 直接删除（不可恢复）。
    #[serde(default = "default_true")]
    pub recycle_deleted_mods: bool,
    /// MOD 元数据并行读取上限；`0` = 全部 CPU 核心。
    #[serde(default)]
    pub mod_metadata_concurrency: u32,
    /// 地图 `info` 并行解析上限；`0` = 全部 CPU 核心。
    #[serde(default = "default_map_info_concurrency")]
    pub map_info_concurrency: u32,
    /// LZMA 转 LZ4 并行上限；0 表示全部 CPU 核心。
    #[serde(default)]
    pub map_compression_concurrency: u32,
    #[serde(default)]
    pub diychef_preview_concurrency: u32,
    /// 已经看过的教程步骤 id；启动教程时只播不在里面的步骤。
    #[serde(default)]
    pub tutorial_watched_steps: Vec<String>,
    pub game: GameConfig,
    pub network: NetworkConfig,
    pub launch: LaunchConfig,
    pub official_api_only: bool,
    pub github_token_set: bool,
    pub gitee_token_set: bool,
    pub gitcode_token_set: bool,
    #[serde(default)]
    pub github_publish_token_set: bool,
    #[serde(default)]
    pub github_publish_login: String,
    pub repositories: Vec<ModRepositoryConfig>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
    #[serde(default)]
    pub community_discovery: Option<CommunityDiscovery>,
    #[serde(default)]
    pub gitee_community_discovery: Option<CommunityDiscovery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitcode_community_discovery: Option<CommunityDiscovery>,
    #[serde(default)]
    pub github_proxies: GitHubProxyConfig,
    pub data_provider: super::DataProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub ui_zoom: Option<f64>,
    pub hide_unidentified_mods: Option<bool>,
    /// 删除 MOD 时先移入回收站（默认 true）。关闭即直接删除，不可恢复。
    pub recycle_deleted_mods: Option<bool>,
    pub game_path: Option<String>,
    pub game_runtime: Option<GameRuntime>,
    pub official_api_only: Option<bool>,
    pub timeout_seconds: Option<u64>,
    pub download_threads: Option<u32>,
    /// MOD 元数据并行读取上限；`0` = 全部 CPU 核心。超出核心数由后端夹紧。
    pub mod_metadata_concurrency: Option<u32>,
    /// 地图 `info` 并行解析上限；`0` = 全部 CPU 核心。
    pub map_info_concurrency: Option<u32>,
    pub map_compression_concurrency: Option<u32>,
    pub diychef_preview_concurrency: Option<u32>,
    /// 这次「已经看过」的教程步骤 id；后端把它并进配置里的集合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutorial_watched_steps: Option<Vec<String>>,
    pub data_provider: Option<super::DataProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPatch {
    pub github_token: String,
    pub gitee_token: String,
    #[serde(default)]
    pub gitcode_token: String,
    /// 显式清除已保存的 Token（留空字符串表示「不修改」）。
    #[serde(default)]
    pub clear: bool,
}

pub(super) fn default_true() -> bool {
    true
}

#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod tutorial_progress_tests;
