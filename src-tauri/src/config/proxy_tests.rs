use super::ConfigService;
use crate::models::GitHubProxyConfig;
use std::fs;
use uuid::Uuid;

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
