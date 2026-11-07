use super::super::test_support::{ApiTestServer, api_test_client};
use super::*;

#[tokio::test]
async fn failed_api_requests_do_not_expose_query_credentials() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let client = Client::builder().no_proxy().build().unwrap();
    let url = format!("http://{address}/api?access_token=placeholder-credential");
    let error = fetch_release_api(&client, &url, &Provider::Gitee, None)
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("access_token"));
    assert!(!error.contains("placeholder-credential"));
    assert!(!error.contains(&address.to_string()));
}

#[tokio::test]
async fn release_api_non_403_failures_try_three_times() {
    for status in [
        "404 Not Found",
        "429 Too Many Requests",
        "500 Internal Server Error",
    ] {
        let server = ApiTestServer::new(vec![(status, ""); 3]);
        let result =
            fetch_release_api_with_retries(&api_test_client(), &server.url, &Provider::Gitee, None)
                .await;
        assert!(result.is_err());
        assert_eq!(server.requests().len(), 3, "{status}");
    }
}

#[tokio::test]
async fn release_api_403_is_never_retried_even_after_an_ordinary_failure() {
    let server = ApiTestServer::new(vec![
        ("500 Internal Server Error", ""),
        ("403 Forbidden", ""),
    ]);
    let error =
        fetch_release_api_with_retries(&api_test_client(), &server.url, &Provider::Gitee, None)
            .await
            .unwrap_err();
    assert!(error.is_forbidden());
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn api_request_targets_order_direct_token_then_proxy() {
    let official = "https://api.github.com/repos/a/b/releases";
    let proxies = vec![GitHubProxy {
        id: "proxy".into(),
        name: "Proxy".into(),
        template: "https://proxy.example/{url}".into(),
        regions: Vec::new(),
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];

    // 配了 Token：直接用官方 + Token，不再先打匿名接口
    let targets = api_request_targets(official, Some("secret"), true, &proxies);
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0], (official.to_string(), Some("secret")));
    assert_eq!(
        targets[1],
        (
            "https://proxy.example/https://api.github.com/repos/a/b/releases".to_string(),
            None,
        )
    );
    // 代理永远不带凭据
    assert_eq!(targets[1].1, None);

    // 没有 Token：官方匿名 + 代理
    let targets = api_request_targets(official, None, false, &proxies);
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0], (official.to_string(), None));
    assert_eq!(targets[1].1, None);
}

#[test]
fn gitee_token_uses_access_token_query_on_official_api_only() {
    // Gitee 的 access_token 走 query 参数（官方约定），且只附加在官方地址上
    let official = "https://gitee.com/api/v5/repos/a/b/releases?per_page=10";
    assert_eq!(
        gitee_api_url_with_token(official, "secret token").unwrap(),
        "https://gitee.com/api/v5/repos/a/b/releases?per_page=10&access_token=secret+token"
    );
    // 代理地址不识别为官方接口，因此不会附加凭据
    assert!(!is_official_gitee_api_url(
        "https://oc2gtapi.ch3ng.top/api/v5/repos/a/b/releases"
    ));
}
