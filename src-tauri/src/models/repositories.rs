use super::config::default_true;
use super::relations::{ReleaseRelationOverride, RepositoryRelation, deserialize_relations};
use super::releases::ReleaseInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitee,
    Gitcode,
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

/// 社区发现得到的「地图源仓库」坐标。
///
/// 与 `CommunityDiscovery` 分开存：MOD 仓库发现的结果要落成完整的
/// `RepositoryConfig` 进 `repositories`（有开关、缓存、安装绑定），
/// 地图侧的仓库只是一句「去这个仓库读 Release note」—— 地图条目要等
/// 地图源树加载时按 `release-tags` 规则现展开，不能提前固化成本地缓存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityMapRepository {
    pub provider: Provider,
    pub username: String,
    pub repository: String,
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

/// 整批刷新期间暂存的 Release 缓存。
///
/// 见 `AppConfig::staged_release_cache` 的说明：清理仓库前把缓存挪到这里，
/// 按源文档重建仓库时再取回，避免 5 分钟缓存被「重建」这件事本身作废。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StagedReleaseCache {
    #[serde(default)]
    pub releases: Vec<ReleaseInfo>,
    #[serde(default)]
    pub cached_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
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
    /// 这个 MOD 支持的桌面平台；`None` / 缺失表示全平台可用。
    ///
    /// 与 `region` 同一语义层：不适用就等于**不存在**（不展示、不刷新、
    /// 不参与绑定与更新统计），而不是「存在但不显示」——否则前端会替一个
    /// 用户看不见的仓库发请求，失败时弹出一条无从处理的错误。
    ///
    /// 为什么需要它：有些 MOD 只对特定平台有意义（例如 macOS 手柄修复），
    /// 在别的平台上装上也毫无作用，却会占满列表并参与更新检测。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
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
    /// 这个 MOD 与其它插件的关系（依赖 / 排斥 / 兼容，按 GUID 声明）。
    ///
    /// 缺失依赖或出现冲突时，MOD 安装页与 MOD 管理页都会在卡片上提示；
    /// 能匹配到线上仓库且版本满足时给出快捷安装入口。
    ///
    /// `requires` 是早期只有依赖时的字段名，保留为别名：已经照那个名字
    /// 写下的源文档不需要改写。
    #[serde(
        default,
        alias = "requires",
        deserialize_with = "deserialize_relations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<RepositoryRelation>,
    /// 按 Release 版本区间覆盖 `relations`（见 `ReleaseRelationOverride`）。
    #[serde(
        default,
        alias = "release_requires",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub release_relations: Vec<ReleaseRelationOverride>,
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
    /// 是否在这个仓库源下展示并刷新这个仓库。
    ///
    /// `false` = 完全不参与：不在 MOD 安装页显示卡片，也不发起 Release 刷新，
    /// 不参与 MOD 绑定与更新统计。除了用户主动关闭，被官方仓库源覆盖的社区
    /// 副本也会被置成 `false`。
    ///
    /// 地区不适用（例如中国大陆下的 international 仓库）是另一条独立判定，
    /// 但语义相同：整个仓库视作不存在。统一判定见
    /// `config::repository_is_available`。
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
    /// `cached_releases` 的获取时间（Unix 毫秒）。
    ///
    /// 用来判断缓存是否还在有效期内，避免每次打开页面或批量刷新都把整棵
    /// 仓库树的 Release API 重打一遍。`None` 表示这份缓存来自旧版本配置或
    /// 从未成功获取过 —— 两种情况都当作「已过期」，必须重新请求。
    #[serde(default)]
    pub releases_cached_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

pub(super) fn default_max_release_count() -> u32 {
    10
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
    #[serde(default)]
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
    /// 支持的桌面平台（`windows` / `macos`）；`None` 表示全平台可用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
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
    /// 与其它插件的关系（依赖 / 排斥 / 兼容）；与 `RepositoryConfig` 同名字段对应。
    ///
    /// `requires` 是早期只有依赖时的字段名，保留为别名：已经照那个名字
    /// 写下的源文档不需要改写。
    #[serde(
        default,
        alias = "requires",
        deserialize_with = "deserialize_relations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<RepositoryRelation>,
    /// 按 Release 版本覆盖关系表的规则（与 `RepositoryConfig` 同名字段对应）。
    #[serde(
        default,
        alias = "release_requires",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub release_relations: Vec<ReleaseRelationOverride>,
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

/// 头像请求：一次批量读取时要哪个平台、哪个 owner 的头像。
///
/// 头像由 owner（用户/组织）决定，不是仓库级信息 —— GitHub/Gitee 都没有
/// 独立的仓库头像。同一 owner 下的多个仓库共用一张图，前端按
/// `{provider}:{owner}` 去重后再传上来。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarRequest {
    pub provider: Provider,
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPatch {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub name_i18n: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: Option<BTreeMap<String, String>>,
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
    /// 支持的桌面平台（`windows` / `macos`）；`None` 表示全平台可用。
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub channel: Channel,
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: Option<BTreeMap<String, String>>,
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    /// 与其它插件的关系；`None` 表示本次编辑不修改关系表。
    #[serde(default)]
    pub relations: Option<Vec<RepositoryRelation>>,
    /// 按 Release 覆盖关系表的规则；`None` 表示本次编辑不修改。
    #[serde(default)]
    pub release_relations: Option<Vec<ReleaseRelationOverride>>,
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
    /// 是否展示并刷新这个仓库（语义同 RepositoryConfig::enabled）。
    pub enabled: bool,
}

pub(super) fn default_download_threads() -> u32 {
    0
}
