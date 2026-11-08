use crate::config::metadata::localized_mod_repository_name;
use crate::models::*;
use std::collections::BTreeMap;

fn repo_with(names: &[(&str, &str)]) -> ModRepositoryConfig {
    let mut name_i18n = BTreeMap::new();
    for (key, value) in names {
        name_i18n.insert((*key).to_string(), (*value).to_string());
    }
    let mut repo: ModRepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "r1", "name": "主机实用 MOD（街机 MOD）稳定版",
        "group": "", "provider": "github", "username": "u", "repository": "r",
        "download_mode": "release", "branch": "main", "file_path": "",
        "region": "", "channel": "all", "install_target": "",
        "enabled": true, "builtin": false
    }))
    .unwrap();
    repo.name_i18n = name_i18n;
    repo
}

#[test]
fn picks_the_name_matching_the_interface_locale() {
    let repo = repo_with(&[
        ("zh-CN", "主机实用 MOD"),
        ("en", "HostUtilities"),
        ("ja", "ホストツール"),
    ]);
    assert_eq!(
        localized_mod_repository_name(&repo, Some("en")),
        "HostUtilities"
    );
    assert_eq!(
        localized_mod_repository_name(&repo, Some("ja")),
        "ホストツール"
    );
    assert_eq!(
        localized_mod_repository_name(&repo, Some("zh-CN")),
        "主机实用 MOD"
    );
}

/// 语言标记缺失时退回中文原名，不能返回空串。
#[test]
fn falls_back_to_the_base_name_when_locale_is_missing_or_unknown() {
    let repo = repo_with(&[("en", "HostUtilities")]);
    assert_eq!(localized_mod_repository_name(&repo, Some("ko")), repo.name);
    assert_eq!(localized_mod_repository_name(&repo, None), repo.name);
    assert_eq!(localized_mod_repository_name(&repo, Some("")), repo.name);
}

/// 源文档可能只写主语言标记（`zh`），界面传来的是 `zh-TW`：应当能对上。
#[test]
fn matches_on_the_primary_language_when_the_exact_tag_is_absent() {
    let repo = repo_with(&[("zh", "主机实用 MOD"), ("en-US", "HostUtilities")]);
    assert_eq!(
        localized_mod_repository_name(&repo, Some("zh-TW")),
        "主机实用 MOD"
    );
    assert_eq!(
        localized_mod_repository_name(&repo, Some("en-GB")),
        "HostUtilities"
    );
}

/// 空翻译值等同缺失，不要显示成空白。
#[test]
fn ignores_blank_translations() {
    let repo = repo_with(&[("en", "   ")]);
    assert_eq!(localized_mod_repository_name(&repo, Some("en")), repo.name);
}
