use super::*;

#[test]
fn the_authorize_url_requests_offline_access() {
    let (_, url, _) = authorization_target(1, "nonce", false);
    assert!(url.contains("scope=offline_access"));
}

#[test]
fn first_sign_in_opens_the_install_page_and_later_ones_do_not() {
    let (mode, url, verifier) = authorization_target(0, "nonce-1", false);
    assert_eq!(mode, PublishAuthMode::Install);
    assert!(url.starts_with(PUBLISH_INSTALL_URL));
    assert!(url.ends_with("?state=nonce-1"));
    // 安装页那条路没有 PKCE 挑战，硬带 verifier 会被 GitHub 判成不匹配。
    assert!(verifier.is_none());

    let (mode, url, verifier) = authorization_target(18334019, "nonce-2", false);
    assert_eq!(mode, PublishAuthMode::Authorize);
    assert!(url.starts_with(AUTHORIZE_ENDPOINT));
    assert!(url.contains("client_id=Iv23liMDOtLBrrkv781U"));
    assert!(url.contains("state=nonce-2"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(verifier.is_some());

    // 装过 App 却没记下 installation id：安装页不会再回调本机，
    // 用户点「改用 GitHub 登录」时必须走 OAuth 那条。
    let (mode, url, verifier) = authorization_target(0, "nonce-3", true);
    assert_eq!(mode, PublishAuthMode::Authorize);
    assert!(url.starts_with(AUTHORIZE_ENDPOINT));
    assert!(verifier.is_some());
}
