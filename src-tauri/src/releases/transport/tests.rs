use super::super::test_support::{ApiTestServer, api_test_client};
use super::*;

#[tokio::test]
async fn release_api_uses_the_client_configured_timeout() {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/releases", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "request did not reach fixture");
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        };
        // Windows accepted sockets inherit the listener's nonblocking mode.
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut buffer = [0_u8; 1024];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0 && request.len() + count <= 8192);
            request.extend_from_slice(&buffer[..count]);
        }
        // A configured ten-second API budget must survive the former five-second limit.
        std::thread::sleep(Duration::from_secs(6));
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]")
            .unwrap();
    });
    let started = Instant::now();
    let result = fetch_release_api(
        &crate::releases::build_client(Duration::from_secs(10)).unwrap(),
        &url,
        &Provider::Gitee,
        None,
    )
    .await;
    server.join().unwrap();
    assert!(result.unwrap().is_empty());
    assert!(started.elapsed() >= Duration::from_secs(6));
}

#[tokio::test]
async fn release_api_short_custom_client_timeout_is_not_overridden() {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/releases", listener.local_addr().unwrap());
    let (request_received_tx, request_received_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let accept_deadline = Instant::now() + Duration::from_secs(3);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < accept_deadline);
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0 && request.len() + count <= 8192);
            request.extend_from_slice(&buffer[..count]);
        }
        request_received_tx.send(Instant::now()).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]");
    });
    let result = fetch_release_api(
        &crate::releases::build_client(Duration::from_millis(150)).unwrap(),
        &url,
        &Provider::Gitee,
        None,
    )
    .await;
    // Exclude the shared provider scheduler queue from the HTTP timeout budget.
    let completed = Instant::now();
    let request_received = request_received_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let elapsed = completed.duration_since(request_received);
    server.join().unwrap();
    assert!(
        result.is_err(),
        "a transport override must not extend the caller's timeout"
    );
    assert!(elapsed < Duration::from_millis(450), "elapsed={elapsed:?}");
}

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
fn repository_api_plan_uses_the_requested_region_for_each_call() {
    let mut repo = crate::releases::test_support::api_test_repository();
    repo.provider = Provider::Github;
    repo.kind = crate::models::RepositoryKind::Map;
    repo.network_plan = toml::from_str(
        "mainland = 'qingyuqingxue-map-proxy-only'\ninternational = 'github-public'",
    )
    .unwrap();
    let official = "https://api.github.com/repos/user/repo/releases?per_page=50&page=2";
    for region in ["cn", "global", "cn"] {
        let targets = repository_api_targets(
            &repo,
            region,
            official,
            Some("never-forward-this"),
            true,
            &[],
        )
        .unwrap();
        assert_eq!(
            targets.iter().any(|(url, _)| url == official),
            region == "global"
        );
        for (url, token) in &targets {
            assert_eq!(*token, (url == official).then_some("never-forward-this"));
        }
    }
}

#[test]
fn repository_api_plan_can_exclude_official_without_forwarding_credentials() {
    let mut repo = crate::releases::test_support::api_test_repository();
    repo.provider = Provider::Github;
    repo.kind = crate::models::RepositoryKind::Map;
    repo.network_plan = toml::from_str(
        "mainland = 'qingyuqingxue-map-proxy-only'\ninternational = 'qingyuqingxue-map-proxy-only'",
    )
    .unwrap();
    let official = "https://api.github.com/repos/user/repo/releases?per_page=50&page=2";
    let targets = repository_api_targets(
        &repo,
        "global",
        official,
        Some("never-forward-this"),
        true,
        &[],
    )
    .unwrap();
    assert_eq!(targets.len(), 1);
    assert!(targets[0].0.starts_with("https://oc2ghapi.ch3ng.top/"));
    assert!(targets[0].0.contains("page=2"));
    assert!(targets[0].1.is_none());
    assert!(!targets[0].0.contains("never-forward-this"));
}
