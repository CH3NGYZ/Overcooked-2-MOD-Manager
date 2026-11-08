use super::*;

#[test]
fn publishing_credentials_are_protected_and_isolated_by_platform() {
    let service = ConfigService::for_test(std::env::temp_dir().join("oc2-no-write-secrets-test"));
    let mut config = crate::config::default_config();
    let original = config.auth.github_token.clone();
    for (provider, access, refresh, login) in [
        (
            PublishProvider::Github,
            "fixture-gh",
            "fixture-gh-refresh",
            "gh-user",
        ),
        (
            PublishProvider::Gitcode,
            "fixture-gc",
            "fixture-gc-refresh",
            "gc-user",
        ),
    ] {
        let tokens = PublishTokens {
            access_token: access.into(),
            refresh_token: Some(refresh.into()),
            expires_at: Some(1234),
            refresh_expires_at: None,
        };
        service
            .set_publish_tokens(&mut config, &tokens, provider, login)
            .unwrap();
        assert_eq!(
            service.publish_token(&config, provider).as_deref(),
            Some(access)
        );
        assert_eq!(
            service.publish_refresh_token(&config, provider).as_deref(),
            Some(refresh)
        );
    }
    assert_ne!(config.auth.gitcode_publish.access_token, "fixture-gc");
    let public = serde_json::to_string(&service.public_config(&config)).unwrap();
    assert!(!public.contains("fixture-gc"));
    assert!(!public.contains("fixture-gh"));
    service.clear_publish_token(&mut config, PublishProvider::Gitcode);
    assert!(
        service
            .publish_token(&config, PublishProvider::Gitcode)
            .is_none()
    );
    assert_eq!(
        service
            .publish_token(&config, PublishProvider::Github)
            .as_deref(),
        Some("fixture-gh")
    );
    assert_eq!(config.auth.github_token, original);
}
