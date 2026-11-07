use crate::models::*;

pub(crate) fn repository(id: &str, mode: RepositoryDownloadMode) -> RepositoryConfig {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": id, "provider": "github",
        "username": "example", "repository": "mod", "download_mode": mode,
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "cached_releases": [{
            "tag_name": "v99.0.0", "version": "99.0.0", "name": "newer release",
            "body": "", "published_at": null, "prerelease": false, "draft": false,
            "assets": []
        }]
    }))
    .unwrap()
}

pub(crate) fn config(repositories: Vec<RepositoryConfig>) -> AppConfig {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "ui_zoom": 1.5,
        "game": {"path": ""},
        "network": {"timeout_seconds": 5, "download_threads": 1,
            "max_retry_count": 3},
        "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
        "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
        "repositories": repositories
    }))
    .unwrap()
}
