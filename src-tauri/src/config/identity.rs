//! 仓库身份、源缓存键与导出模型投影。

use super::availability::normalize_repository_platform;
use super::availability::normalize_repository_region;
use super::availability::normalize_repository_region_for_storage;
use crate::models::*;

pub(crate) fn repository_address_key(
    provider: &Provider,
    username: &str,
    repository: &str,
    region: Option<&str>,
) -> (&'static str, String, String, String) {
    (
        match provider {
            Provider::Github => "github",
            Provider::Gitee => "gitee",
            Provider::Gitcode => "gitcode",
        },
        username.trim().to_ascii_lowercase(),
        repository.trim().to_ascii_lowercase(),
        normalize_repository_region(region),
    )
}

pub(crate) fn repo_key(repo: &ModRepositoryConfig) -> String {
    let export = ModRepositoryExport {
        network_plan: repo.network_plan.clone(),
        kind: repo.kind,
        name: String::new(),
        name_i18n: repo.name_i18n.clone(),
        group: repo.group.clone(),
        group_i18n: repo.group_i18n.clone(),
        provider: repo.provider.clone(),
        username: repo.username.clone(),
        repository: repo.repository.clone(),
        download_mode: repo.download_mode.clone(),
        branch: repo.branch.clone(),
        file_path: repo.file_path.clone(),
        region: repo.region.clone(),
        platform: repo.platform.clone(),
        channel: repo.channel.clone(),
        asset_rules: repo.asset_rules.clone(),
        install_target: repo.install_target.clone(),
        note: repo.note.clone(),
        note_i18n: repo.note_i18n.clone(),
        plugin_guid: repo.plugin_guid.clone(),
        local_product_name: repo.local_product_name.clone(),
        local_original_filename: repo.local_original_filename.clone(),
        relations: repo.relations.clone(),
        release_relations: repo.release_relations.clone(),
        version_regex: repo.version_regex.clone(),
        max_release_count: repo.max_release_count,
        download_threads: repo.download_threads,
        enabled: repo.enabled,
        source_id: repo.source_id.clone(),
    };
    export_key(&export)
}

pub(crate) fn source_repository_key(source_id: &str, identity: &str) -> String {
    format!("{}:{source_id}{identity}", source_id.len())
}

pub(crate) fn export_key(repo: &ModRepositoryExport) -> String {
    // Fields hidden by the current download mode are not repository identity.
    // Omitting them from an export must not change its ID on re-import.
    let (branch, file_path, asset_rules) = match repo.download_mode {
        ModDownloadMode::Release => ("", "", repo.asset_rules.as_slice()),
        ModDownloadMode::RepositoryFile => (repo.branch.as_str(), repo.file_path.as_str(), &[][..]),
    };
    format!(
        "{:?}|{}|{}|{:?}|{:?}|{}|{}|{:?}|{}|{}|{}",
        repo.provider,
        repo.username.trim().to_ascii_lowercase(),
        repo.repository.trim().to_ascii_lowercase(),
        repo.download_mode,
        // Stable and Beta entries of one repository are separate install
        // targets, so the channel belongs to the repository identity.
        repo.channel,
        branch,
        file_path,
        asset_rules,
        repo.install_target,
        repo.enabled,
        normalize_repository_region(repo.region.as_deref())
    )
}

/// 从 MOD 导出模型构造 MOD 配置，统一兼容字段和默认值。
/// 地图与通用 Release 读取直接构造 RepositoryRequest，不经过 MOD 配置。
pub(crate) fn mod_repository_from_export(
    repo: &ModRepositoryExport,
    id: String,
    builtin: bool,
) -> ModRepositoryConfig {
    ModRepositoryConfig {
        network_plan: repo.network_plan.clone(),
        kind: repo.kind,
        id,
        name: repo.name.clone(),
        name_i18n: repo.name_i18n.clone(),
        group: repo.group.clone(),
        group_i18n: repo.group_i18n.clone(),
        provider: repo.provider.clone(),
        username: repo.username.trim().to_string(),
        repository: repo.repository.trim().to_string(),
        download_mode: repo.download_mode.clone(),
        branch: repo.branch.clone(),
        file_path: repo.file_path.clone(),
        region: normalize_repository_region_for_storage(
            repo.region.as_deref(),
            &crate::locale_tag::with_params(
                "config.repositoryLabeled",
                &[("name", &repo.name.to_string())],
            ),
        ),
        platform: normalize_repository_platform(repo.platform.as_deref()),
        channel: repo.channel.clone(),
        asset_rules: repo.asset_rules.clone(),
        install_target: repo.install_target.clone(),
        note: repo.note.clone(),
        note_i18n: repo.note_i18n.clone(),
        // An exported search result can be imported as a custom repository,
        // but cannot impersonate the generated community catalogue.
        source_id: repo
            .source_id
            .clone()
            .filter(|id| !crate::community::is_source(id)),
        plugin_guid: repo.plugin_guid.clone(),
        local_product_name: repo.local_product_name.clone(),
        local_original_filename: repo.local_original_filename.clone(),
        relations: repo.relations.clone(),
        release_relations: repo.release_relations.clone(),
        version_regex: repo.version_regex.clone(),
        max_release_count: repo.max_release_count.clamp(1, 100),
        download_threads: repo.download_threads,
        github_token: String::new(),
        gitee_token: String::new(),
        official_api_only: true,
        enabled: repo.enabled,
        builtin,
        pinned_version: None,
        ignored_version: None,
        etag: None,
        cached_releases: Vec::new(),
        releases_cached_at: None,
        last_error: None,
    }
}
