use super::NetworkPlanSelection;
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
/// `ModRepositoryConfig` 进 `repositories`（有开关、缓存、安装绑定），
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

pub(super) fn default_max_release_count() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySource {
    #[serde(default, skip_serializing_if = "NetworkPlanSelection::is_empty")]
    pub network_plan: NetworkPlanSelection,
    pub id: String,
    pub nickname: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nickname_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
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

pub(super) fn default_download_threads() -> u32 {
    0
}
