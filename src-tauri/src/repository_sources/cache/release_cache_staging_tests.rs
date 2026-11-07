use super::*;

fn repo(id: &str, source_id: &str) -> crate::models::RepositoryConfig {
    let mut repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": id, "name": "Mod", "group": "G",
        "provider": "github", "username": "u", "repository": "r",
        "download_mode": "release", "channel": "all",
        "asset_rules": [{ "type": "exact", "value": "a.zip" }],
        "install_target": "BepInEx/plugins",
        "enabled": true, "builtin": false,
        "max_release_count": 10, "download_threads": 32
    }))
    .unwrap();
    repo.source_id = Some(source_id.to_string());
    repo
}

#[test]
fn clearing_a_source_stages_the_release_cache_instead_of_dropping_it() {
    let mut config = crate::config::default_config();
    config.repository_sources = vec![
        serde_json::from_value(serde_json::json!({
            "id": "src", "nickname": "S", "nickname_i18n": { "zh-CN": "S" },
            "url": "https://example.com/x.toml", "builtin": false
        }))
        .unwrap(),
    ];

    let mut entry = repo("repo-1", "src");
    entry.cached_releases = vec![
        serde_json::from_value(serde_json::json!({
            "tag_name": "v1.0.0", "version": "1.0.0", "name": "v1", "body": "",
            "prerelease": false, "draft": false, "assets": []
        }))
        .unwrap(),
    ];
    entry.releases_cached_at = Some(1_700_000_000_000);
    config.repositories = vec![entry];

    clear_source_cache(&mut config, &["src".to_string()]);

    // 仓库确实被清掉了（源文档要重新决定它的存在与配置）
    assert!(config.repositories.is_empty(), "仓库应被清空");
    // 但缓存被暂存下来，等重建时取回
    let staged = config
        .staged_release_cache
        .get("repo-1")
        .expect("缓存必须被暂存，否则重建后永远命中不了");
    assert_eq!(staged.releases.len(), 1);
    assert_eq!(staged.cached_at, Some(1_700_000_000_000));
}

#[test]
fn empty_caches_are_not_staged() {
    let mut config = crate::config::default_config();
    config.repository_sources = vec![
        serde_json::from_value(serde_json::json!({
            "id": "src", "nickname": "S", "nickname_i18n": { "zh-CN": "S" },
            "url": "https://example.com/x.toml", "builtin": false
        }))
        .unwrap(),
    ];
    // 从没成功刷新过：空列表 + 没有时间戳
    config.repositories = vec![repo("repo-1", "src")];

    clear_source_cache(&mut config, &["src".to_string()]);

    assert!(
        config.staged_release_cache.is_empty(),
        "空缓存不该搬进暂存表，否则判断变复杂而且没有意义"
    );
}
