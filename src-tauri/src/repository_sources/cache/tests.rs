use super::*;

#[test]
fn source_cache_clearing_removes_complete_trees_and_preserves_settings() {
    let mut config: AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "ui_zoom": 1.5,
        "game": {"path": "custom-game"},
        "network": {"timeout_seconds": 5, "download_threads": 1,
            "max_retry_count": 3},
        "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
        "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
        "repository_sources": [
            {"id": "leaf", "parent_id": "child", "nickname": "Leaf", "url": "leaf.toml"},
            {"id": "second", "nickname": "Second", "url": "second.toml"},
            {"id": "child", "parent_id": "first", "nickname": "Child", "url": "child.toml"},
            {"id": "first", "nickname": "First", "url": "first.toml", "builtin": true},
            {"id": "other", "nickname": "Other", "url": "other.toml"}
        ],
        "repositories": (["first", "second", "child", "leaf", "other", "manual"]
            .iter().map(|id| serde_json::json!({
                "id": id, "name": id, "provider": "github", "username": "example",
                "repository": "mod", "install_target": "BepInEx/plugins",
                "enabled": false, "builtin": false,
                "source_id": if *id == "manual" { None } else { Some(*id) },
                "etag": "old-etag", "last_error": "old-error",
                "cached_releases": [{"tag_name": "old", "name": "Old", "body": "",
                    "prerelease": false, "draft": false, "assets": []}]
            })).collect::<Vec<_>>())
    }))
    .unwrap();
    let original = serde_json::to_value(&config).unwrap();
    let source_ids = vec!["first".into(), "second".into()];
    assert_eq!(clear_source_cache(&mut config, &source_ids), (4, 2));
    assert_eq!(
        config
            .repositories
            .iter()
            .map(|repo| repo.id.as_str())
            .collect::<Vec<_>>(),
        ["other", "manual"]
    );
    assert_eq!(
        config
            .repository_sources
            .iter()
            .map(|source| source.id.as_str())
            .collect::<Vec<_>>(),
        ["second", "first", "other"]
    );
    let cleared = serde_json::to_value(&config).unwrap();
    for field in ["game", "network", "launch", "auth", "ui_zoom"] {
        assert_eq!(cleared[field], original[field]);
    }
    assert_eq!(cleared["repositories"][0], original["repositories"][4]);
    assert_eq!(cleared["repositories"][1], original["repositories"][5]);
    assert_eq!(clear_source_cache(&mut config, &source_ids), (0, 0));
    assert_eq!(clear_source_cache(&mut config, &[]), (0, 0));
}
