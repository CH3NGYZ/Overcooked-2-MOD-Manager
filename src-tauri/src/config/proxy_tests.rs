use super::ConfigService;
use crate::models::GitHubProxyConfig;
use std::fs;
use uuid::Uuid;

#[tokio::test]
async fn bundled_provider_blocks_remote_refresh_and_replaces_the_entire_network_snapshot() {
    let root = std::env::temp_dir().join(format!("oc2-bundled-provider-{}", Uuid::new_v4()));
    let service = ConfigService::for_test(root.clone());
    let stale = service.load().unwrap();
    let bundled = crate::github_proxies::bundled_defaults();
    service
        .proxies
        .replace_for_test(GitHubProxyConfig::default());
    assert_ne!(service.load().unwrap().github_proxies, bundled);
    service.select_data_provider(crate::models::DataProvider::Builtin);
    service.refresh_proxies().await.unwrap();
    let loaded = service.clone().load().unwrap();
    assert_eq!(loaded.github_proxies, bundled);
    assert_eq!(loaded.repository_sources[0].url, "mods.toml");
    let public = service.public_config(&stale);
    assert_eq!(public.data_provider, crate::models::DataProvider::Builtin);
    assert_eq!(public.github_proxies, bundled);
    assert_eq!(public.repository_sources[0].url, "mods.toml");
    // Saving the pre-switch config cannot restore a previous provider or policy.
    service.save(&stale).unwrap();
    assert_eq!(
        service.load().unwrap().data_provider,
        crate::models::DataProvider::Builtin
    );
    assert!(service.load().unwrap().network_policy.is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn proxy_snapshots_are_shared_but_never_saved_or_restored() {
    let root = std::env::temp_dir().join(format!("oc2-proxies-{}", Uuid::new_v4()));
    let service = ConfigService::for_test(root.clone());
    let config = service.load().unwrap();
    let bundled = crate::github_proxies::bundled_defaults();
    assert_eq!(config.github_proxies, bundled);
    let old = format!(
        "{}\n[github_proxies]\nrelease_proxies = 'obsolete invalid shape'\n",
        fs::read_to_string(service.config_path()).unwrap()
    );
    fs::write(service.config_path(), old).unwrap();
    let config = service.load().unwrap();
    assert_eq!(
        config.github_proxies, bundled,
        "legacy lists must be ignored even if malformed"
    );
    assert!(
        !fs::read_to_string(service.config_path())
            .unwrap()
            .contains("github_proxies")
    );
    assert!(!root.join("config.previous.toml").exists());

    let clone = service.clone();
    service
        .proxies
        .replace_for_test(GitHubProxyConfig::default());
    assert_eq!(
        clone.load().unwrap().github_proxies,
        GitHubProxyConfig::default()
    );
    assert_eq!(
        clone.public_config(&config).github_proxies,
        GitHubProxyConfig::default()
    );
    // Saving a stale config must neither restore proxies nor serialize them.
    clone.save(&config).unwrap();
    assert_eq!(
        service.load().unwrap().github_proxies,
        GitHubProxyConfig::default()
    );
    assert!(!toml::to_string(&config).unwrap().contains("github_proxies"));
    assert!(
        serde_json::to_value(&config)
            .unwrap()
            .get("github_proxies")
            .is_none()
    );
    let mut changed = config.clone();
    changed.ui_zoom = 2.0;
    fs::write(
        service.config_path(),
        format!("{}\n# external edit\n", toml::to_string(&changed).unwrap()),
    )
    .unwrap();
    let loaded = clone.load().unwrap();
    assert_eq!(loaded.ui_zoom, 2.0);
    assert_eq!(loaded.github_proxies, GitHubProxyConfig::default());
    let restarted = ConfigService::for_test(root.clone());
    assert_eq!(restarted.load().unwrap().github_proxies, bundled);
    assert_eq!(restarted.load().unwrap().ui_zoom, 2.0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn config_error_fallback_uses_current_runtime_snapshot() {
    let root = std::env::temp_dir().join(format!("oc2-proxy-fallback-{}", Uuid::new_v4()));
    let service = ConfigService::for_test(root.clone());
    service.load().unwrap();
    service
        .proxies
        .replace_for_test(GitHubProxyConfig::default());
    fs::write(service.config_path(), "invalid [ config").unwrap();
    assert_eq!(
        service.load_or_default().github_proxies,
        GitHubProxyConfig::default()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn provider_is_initialized_once_per_session_and_never_restored_by_disk_reads() {
    use crate::models::DataProvider;
    let root = std::env::temp_dir().join(format!("oc2-session-provider-{}", Uuid::new_v4()));
    for (region, expected) in [
        ("cn", DataProvider::Gitee),
        ("global", DataProvider::Github),
    ] {
        let service = ConfigService::for_test(root.clone());
        service.select_data_provider(DataProvider::for_region(region));
        let before = service.load().unwrap();
        assert_eq!(before.data_provider, expected);
        service.select_data_provider(DataProvider::Builtin);
        let clone = service.clone();
        for _ in 0..4 {
            assert_eq!(clone.load().unwrap().data_provider, DataProvider::Builtin);
        }
        let mut changed = before.clone();
        changed.ui_zoom = 1.75;
        // External edits (even a persisted selector from a different version)
        // and stale writes cannot replace this session's explicit selection.
        let mut stored = toml::Value::try_from(&changed).unwrap();
        stored
            .as_table_mut()
            .unwrap()
            .insert("data_provider".into(), "github".into());
        fs::write(service.config_path(), toml::to_string(&stored).unwrap()).unwrap();
        assert_eq!(clone.load().unwrap().data_provider, DataProvider::Builtin);
        assert_eq!(clone.load().unwrap().ui_zoom, 1.75);
        clone.save(&before).unwrap();
        assert_eq!(clone.load().unwrap().data_provider, DataProvider::Builtin);
        let restarted = ConfigService::for_test(root.clone());
        restarted.select_data_provider(DataProvider::for_region(region));
        assert_eq!(restarted.load().unwrap().data_provider, expected);
        let stored = fs::read_to_string(service.config_path()).unwrap();
        assert!(!stored.contains("data_provider"));
        assert!(!stored.contains("network_policy"));
    }
    fs::remove_dir_all(root).unwrap();
}
