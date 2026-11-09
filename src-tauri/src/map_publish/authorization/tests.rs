use super::*;

#[test]
fn the_authorize_url_requests_offline_access() {
    let (_, url, _) = authorization_target(PublishProvider::Github, 1, "nonce", false);
    assert!(url.contains("scope=offline_access"));
}

#[test]
fn first_sign_in_opens_the_install_page_and_later_ones_do_not() {
    let (mode, url, verifier) = authorization_target(PublishProvider::Github, 0, "nonce-1", false);
    assert_eq!(mode, PublishAuthMode::Install);
    assert!(url.starts_with(PUBLISH_INSTALL_URL));
    assert!(url.ends_with("?state=nonce-1"));
    // 安装页那条路没有 PKCE 挑战，硬带 verifier 会被 GitHub 判成不匹配。
    assert!(verifier.is_none());

    let (mode, url, verifier) =
        authorization_target(PublishProvider::Github, 18334019, "nonce-2", false);
    assert_eq!(mode, PublishAuthMode::Authorize);
    assert!(url.starts_with(AUTHORIZE_ENDPOINT));
    let parsed = url::Url::parse(&url).unwrap();
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(key, _)| key == "client_id")
            .unwrap()
            .1,
        GITHUB_APP_CLIENT_ID
    );
    assert!(url.contains("state=nonce-2"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(verifier.is_some());

    // 装过 App 却没记下 installation id：安装页不会再回调本机，
    // 用户点「改用 GitHub 登录」时必须走 OAuth 那条。
    let (mode, url, verifier) = authorization_target(PublishProvider::Github, 0, "nonce-3", true);
    assert_eq!(mode, PublishAuthMode::Authorize);
    assert!(url.starts_with(AUTHORIZE_ENDPOINT));
    assert!(verifier.is_some());
}

#[test]
fn gitcode_always_uses_oauth_with_state_and_without_installation_or_unverified_pkce() {
    for known in [0, 123] {
        let (mode, url, verifier) =
            authorization_target(PublishProvider::Gitcode, known, "a&b", false);
        assert_eq!(mode, PublishAuthMode::Authorize);
        assert!(url.starts_with("https://gitcode.com/oauth/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=a%26b"));
        assert!(!url.contains("code_challenge"));
        assert!(verifier.is_none());
    }
}

#[test]
fn gitcode_optional_scope_preserves_state_callback_and_explicit_permissions() {
    for scope in ["", "  ", "all_user all_repository", "  requested&scope  "] {
        let target = gitcode_authorize_url(
            "fixture-id",
            "http://127.0.0.1:52214/callback",
            scope,
            "a&b",
        );
        let url = url::Url::parse(&target).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], "fixture-id");
        assert_eq!(query["redirect_uri"], "http://127.0.0.1:52214/callback");
        assert_eq!(query["state"], "a&b");
        assert_eq!(query["response_type"], "code");
        if scope.trim().is_empty() {
            assert!(!query.contains_key("scope"));
            assert_eq!(query.len(), 4);
        } else {
            assert_eq!(query["scope"], scope.trim());
            assert_eq!(query.len(), 5);
        }
    }
}
