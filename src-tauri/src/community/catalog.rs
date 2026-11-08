use super::github::SEARCH_ENDPOINT;
use crate::models::CommunityDiscovery;
use crate::models::CommunityMapRepository;
use crate::models::ModRepositoryConfig;
use crate::models::Provider;
use crate::models::RepositorySource;
use crate::releases;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashSet;

pub const SOURCE_ID: &str = "github-community";

pub const GITCODE_SOURCE_ID: &str = "gitcode-community";

pub const GITEE_SOURCE_ID: &str = "gitee-community";

/// 命中结果必须同时带这两个 topic 才算社区仓库。
///
/// 两个平台的发现入口共用这一份口径：GitHub 走 Search API 的仓库对象，
/// Gitee 走话题页 HTML，判据必须是同一条，否则同一次刷新两边会给出
/// 互相矛盾的「准入」结果。
pub(crate) const REQUIRED_TOPICS: [&str; 2] = ["overcooked-2", "bepinex"];

pub fn is_repository(repo: &ModRepositoryConfig) -> bool {
    repo.source_id.as_deref().is_some_and(is_source)
}

pub fn is_source(id: &str) -> bool {
    matches!(id, SOURCE_ID | GITEE_SOURCE_ID | GITCODE_SOURCE_ID)
}

pub fn source_id(provider: &Provider) -> &'static str {
    match provider {
        Provider::Github => SOURCE_ID,
        Provider::Gitee => GITEE_SOURCE_ID,
        Provider::Gitcode => GITCODE_SOURCE_ID,
    }
}

/**
 * 两个社区源（GitHub / Gitee）都不再限制地区：用户在哪个地区都可能想装
 * 另一边社区里的 MOD。地区只由仓库/来源自身的 `region` 字段表达，
 * 社区源本身不再额外施加平台限制，所以这里不再有「地区是否可用」的判断。
 *
 * 与官方仓库源的关系不靠地区表达，而是靠身份闪避（见
 * `official_repository_keys`）：只要官方源树里已经收录同一个
 * owner/repository，社区那份就整条隐藏，哪怕官方那份当前地区不可见或被禁用。
 */

pub fn is_source_archive(
    provider: &Provider,
    source_id: Option<&str>,
    tag: &str,
    asset_name: &str,
) -> bool {
    *provider == Provider::Gitee
        && source_id == Some(GITEE_SOURCE_ID)
        && !tag.trim().is_empty()
        && asset_name
            .trim()
            .eq_ignore_ascii_case(&format!("{}.zip", tag.trim()))
}

pub fn source(provider: &Provider) -> RepositorySource {
    let name = match provider {
        Provider::Github => "GitHub",
        Provider::Gitee => "Gitee",
        Provider::Gitcode => "GitCode",
    };
    RepositorySource {
        network_plan: Default::default(),
        id: source_id(provider).into(),
        nickname: crate::locale_tag::with_params(
            "community.labelSimplified",
            &[("name", &name.to_string())],
        ),
        nickname_i18n: [
            (
                "zh-CN".into(),
                crate::locale_tag::with_params(
                    "community.labelSimplified",
                    &[("name", &name.to_string())],
                ),
            ),
            (
                "zh-TW".into(),
                crate::locale_tag::with_params(
                    "community.labelTraditional",
                    &[("name", &name.to_string())],
                ),
            ),
            ("en".into(), format!("{name} community repositories")),
            ("ko".into(), format!("{name} 커뮤니티 저장소")),
            ("ja".into(), format!("{name} コミュニティリポジトリ")),
        ]
        .into(),
        url: match provider {
            Provider::Github => SEARCH_ENDPOINT,
            Provider::Gitee => crate::gitee_community::TOPIC_ENDPOINT,
            Provider::Gitcode => super::gitcode::GITCODE_SEARCH_ENDPOINT,
        }
        .into(),
        /*
         * 两个社区源都不限制地区：社区发现是「额外找找还有哪些 MOD」，
         * 用户在哪个地区都可能想装 Gitee 或 GitHub 上的社区 MOD。
         *
         * 与官方仓库源的关系不靠地区表达，而是靠身份闪避（见
         * `official_repository_keys`）：只要官方源树里已经收录同一个
         * owner/repository，社区那份就整条隐藏，哪怕官方那份当前地区
         * 不可见或被禁用。
         */
        region: None,
        builtin: false,
        parent_id: None,
    }
}

/// 一条搜索命中最终算哪种仓库。
///
/// `Skip` 与 `Invalid` 的差别只在「要不要报告发现不完整」：仓库带了准入
/// topic 却没标 `mod` / `map` 的，是「用 BepInEx 的胡闹厨房仓库」，既不是
/// MOD 也不是地图源，静默丢弃；连坐标都读不出来才是真的格式异常，要计入
/// `incomplete_results` 让用户知道这次发现不完整。
pub(super) enum SearchHit {
    Mod(ModRepositoryConfig),
    Map(CommunityMapRepository),
    Skip,
    Invalid,
}

pub(super) fn classify_search_item(item: &Value) -> SearchHit {
    classify_provider_item(item, &Provider::Github)
}

pub(super) fn classify_provider_item(item: &Value, provider: &Provider) -> SearchHit {
    let Some(topics) = item.get("topics").and_then(Value::as_array).map(|topics| {
        topics
            .iter()
            .filter_map(Value::as_str)
            .collect::<HashSet<_>>()
    }) else {
        return SearchHit::Invalid;
    };
    let has_topic = |topic: &str| topics.iter().any(|value| *value == topic);
    if item.get("private").and_then(Value::as_bool) != Some(false)
        || !REQUIRED_TOPICS.iter().all(|topic| has_topic(topic))
    {
        return SearchHit::Skip;
    }
    let (Some(owner), Some(name)) = (
        item.get("owner")
            .and_then(|owner| owner.get("login"))
            .and_then(Value::as_str),
        item.get("name").and_then(Value::as_str),
    ) else {
        return SearchHit::Invalid;
    };
    let Ok((owner, name)) = releases::repository_coordinates(owner, name) else {
        return SearchHit::Invalid;
    };
    /*
     * `mod` 优先：同时标了两个 topic 的仓库按 MOD 仓库收。否则它的 Release
     * 会被 MOD 页与地图页各刷一遍，同一个仓库在两处显示两份身份。
     */
    if has_topic("mod") {
        let branch = item
            .get("default_branch")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let note = item
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return repository_from_coordinates(provider, &owner, &name, branch, note)
            .map_or(SearchHit::Invalid, SearchHit::Mod);
    }
    if has_topic("map") {
        return SearchHit::Map(CommunityMapRepository {
            provider: provider.clone(),
            username: owner,
            repository: name,
        });
    }
    SearchHit::Skip
}

pub(crate) fn repository_from_coordinates(
    provider: &Provider,
    owner: &str,
    name: &str,
    branch: &str,
    note: &str,
) -> Option<ModRepositoryConfig> {
    let (owner, name) = releases::repository_coordinates(owner, name).ok()?;
    let identity = format!(
        "{}/{}",
        owner.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    );
    let prefix = match provider {
        Provider::Github => "community",
        Provider::Gitee => "gitee-community",
        Provider::Gitcode => "gitcode-community",
    };
    let id = format!("{prefix}-{:x}", Sha256::digest(identity.as_bytes()));
    let display_name = format!("{owner}/{name}");
    serde_json::from_value(serde_json::json!({
        "id": id, "name": display_name, "name_i18n": {"zh-CN": display_name},
        "provider": provider, "username": owner,
        "repository": name, "branch": branch, "note": note,
        "group_i18n": {"zh-CN": ""}, "note_i18n": {"zh-CN": note},
        /*
         * 社区仓库不写地区：两个社区源都适用于所有地区，
         * 仓库自身也就没有地区限制。与官方源的关系靠身份闪避处理，
         * 不靠地区表达。
         */
        "source_id": source_id(provider), "install_target": "BepInEx/plugins", "channel": "all",
        "asset_rules": [{"type": "regex", "value": "^.+\\.(?:[dD][lL][lL]|[zZ][iI][pP])$"}],
        "download_threads": 16, "enabled": true, "builtin": false
    }))
    .ok()
}

pub(crate) struct SearchResults {
    pub(crate) repositories: Vec<ModRepositoryConfig>,
    pub(crate) map_repositories: Vec<CommunityMapRepository>,
    pub(crate) discovery: CommunityDiscovery,
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(super) use test_support::repository_from_search;
#[cfg(test)]
mod tests;
