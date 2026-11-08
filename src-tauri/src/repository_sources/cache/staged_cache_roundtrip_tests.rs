use super::*;

fn sample_repo(id: &str, source_id: &str) -> crate::models::ModRepositoryConfig {
    let mut repo: crate::models::ModRepositoryConfig = serde_json::from_value(serde_json::json!({
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

fn sample_source(id: &str) -> crate::models::RepositorySource {
    serde_json::from_value(serde_json::json!({
        "id": id, "nickname": "S", "nickname_i18n": { "zh-CN": "S" },
        "url": "https://example.com/x.toml", "builtin": false
    }))
    .unwrap()
}

#[test]
fn cache_survives_clear_and_is_restorable_by_id() {
    let now = 1_700_000_000_000i64;
    let mut config = crate::config::default_config();
    config.repository_sources = vec![sample_source("src")];

    let mut entry = sample_repo("repo-1", "src");
    entry.cached_releases = vec![
        serde_json::from_value(serde_json::json!({
            "tag_name": "v1.0.0", "version": "1.0.0", "name": "v1", "body": "",
            "prerelease": false, "draft": false, "assets": []
        }))
        .unwrap(),
    ];
    entry.releases_cached_at = Some(now);
    config.repositories = vec![entry];

    // 第一步：整批刷新前的清理
    clear_source_cache(&mut config, &["src".to_string()]);
    assert!(config.repositories.is_empty(), "仓库应被清空");
    let staged = config
        .staged_release_cache
        .get("repo-1")
        .expect("缓存必须暂存下来");
    assert_eq!(staged.cached_at, Some(now));

    // 第二步：按源文档重建（模拟 apply_document 的接回）
    let mut rebuilt = sample_repo("new-uuid", "src");
    rebuilt.id = "repo-1".into();
    if let Some(pulled) = config.staged_release_cache.remove("repo-1") {
        rebuilt.cached_releases = pulled.releases;
        rebuilt.releases_cached_at = pulled.cached_at;
        rebuilt.last_error = pulled.last_error;
    }
    config.repositories = vec![rebuilt];

    // 重建后的仓库必须带着原缓存，且时间戳仍能让 TTL 判定通过
    assert_eq!(config.repositories[0].cached_releases.len(), 1);
    assert_eq!(config.repositories[0].releases_cached_at, Some(now));
    assert!(config.staged_release_cache.is_empty(), "认领后暂存表应为空");
}

/// 多个源共存时，暂存表不能被「第一个源重建完」就清空。
///
/// 这正是用户踩到的那个 bug：`apply_document` 逐源调用，早先每次都
/// 无条件 `clear()`，于是只有第一个源能拿回缓存。
#[test]
fn staging_table_keeps_entries_for_sources_not_yet_rebuilt() {
    let mut config = crate::config::default_config();
    config.repository_sources = vec![sample_source("src-a"), sample_source("src-b")];

    let mut a = sample_repo("repo-a", "src-a");
    a.cached_releases = vec![
        serde_json::from_value(serde_json::json!({
            "tag_name": "v1.0.0", "version": "1.0.0", "name": "v1", "body": "",
            "prerelease": false, "draft": false, "assets": []
        }))
        .unwrap(),
    ];
    a.releases_cached_at = Some(1_700_000_000_000);
    let mut b = a.clone();
    b.id = "repo-b".into();
    b.source_id = Some("src-b".into());
    config.repositories = vec![a, b];

    clear_source_cache(&mut config, &["src-a".to_string(), "src-b".to_string()]);
    assert_eq!(config.staged_release_cache.len(), 2);

    // 只认领 A（模拟 src-a 的 apply_document 跑完）
    config.staged_release_cache.remove("repo-a");
    assert!(
        config.staged_release_cache.contains_key("repo-b"),
        "B 的缓存必须留到它自己的 apply_document 跑完"
    );
}
