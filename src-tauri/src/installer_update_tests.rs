use super::*;
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn target() -> crate::app_update::UpdateTarget {
    crate::app_update::UpdateTarget::from_parts("windows", "x86_64", Some("exe")).unwrap()
}

fn fixture(provider: Provider) -> serde_json::Value {
    let base = match provider {
        Provider::Github => "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager",
        Provider::Gitee => "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager",
    };
    let platforms = [
        ("windows-x86_64-nsis", "OC2ModManager_9.0.0_x64-setup.exe"),
        ("windows-x86_64-msi", "OC2ModManager_9.0.0_x64_en-US.msi"),
        (
            "linux-x86_64-appimage",
            "OC2ModManager_9.0.0_amd64.AppImage",
        ),
        ("linux-x86_64-deb", "OC2ModManager_9.0.0_amd64.deb"),
        ("linux-x86_64-rpm", "OC2ModManager-9.0.0-1.x86_64.rpm"),
        ("darwin-aarch64-app", "OC2ModManager_aarch64.app.tar.gz"),
        ("darwin-x86_64-app", "OC2ModManager_x64.app.tar.gz"),
    ]
    .into_iter()
    .map(|(key, name)| {
        (
            key.into(),
            serde_json::json!({
                "url": format!("{base}/releases/download/v9.0.0/{name}"),
                "signature": "public signature fixture",
            }),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    serde_json::json!({
        "version": "9.0.0", "notes": "Release notes",
        "pub_date": "2026-10-13T19:33:14.785Z", "platforms": platforms,
    })
}

fn direct(provider: Provider) -> InstallerChannel {
    InstallerChannel {
        id: if provider == Provider::Gitee {
            "gitee-direct"
        } else {
            "github-direct"
        }
        .into(),
        endpoint: installer_manifest_url(&provider),
        provider,
        proxy: None,
    }
}

#[test]
fn region_order_uses_all_and_only_compiled_release_templates() {
    let mainland = installer_manifest_sources(InstallerRegion::MainlandChina);
    let international = installer_manifest_sources(InstallerRegion::International);
    assert_eq!(mainland[0].id, "gitee-direct");
    assert_eq!(mainland[1].id, "github-direct");
    assert_eq!(international[0].id, "github-direct");
    assert_eq!(mainland.len(), international.len() + 1);
    assert!(
        international
            .iter()
            .all(|route| route.provider == Provider::Github)
    );
    let defaults = crate::github_proxies::bundled_defaults();
    let urls = crate::github_proxies::proxy_urls(
        &defaults.release_proxies,
        &international[0].endpoint,
        false,
        None,
    );
    assert_eq!(
        international[1..]
            .iter()
            .map(|route| &route.endpoint)
            .collect::<Vec<_>>(),
        urls.iter().collect::<Vec<_>>()
    );
    for route in &international[1..] {
        let proxy = route.proxy.as_ref().unwrap();
        assert!(defaults.release_proxies.contains(proxy));
        assert!(route.id.starts_with("github-proxy:"));
        assert!(!route.endpoint.contains("api.github.com"));
    }
    assert_eq!(
        international.len(),
        international
            .iter()
            .map(|route| &route.endpoint)
            .collect::<HashSet<_>>()
            .len()
    );
    // Edits to a saved/default copy cannot affect updater route construction.
    let mut saved = defaults;
    for proxy in &mut saved.release_proxies {
        proxy.template = "https://custom.example/{url}".into();
    }
    assert_eq!(
        installer_manifest_sources(InstallerRegion::International)[1].endpoint,
        international[1].endpoint
    );
}

#[test]
fn each_of_seven_formats_is_selected_from_json_without_api_size_or_digest() {
    for provider in [Provider::Github, Provider::Gitee] {
        let route = direct(provider.clone());
        let document = fixture(provider);
        for (os, arch, kind) in [
            ("windows", "x86_64", "exe"),
            ("windows", "x86_64", "msi"),
            ("linux", "x86_64", "appimage"),
            ("linux", "x86_64", "deb"),
            ("linux", "x86_64", "rpm"),
            ("macos", "aarch64", "app"),
            ("macos", "x86_64", "app"),
        ] {
            let target = crate::app_update::UpdateTarget::from_parts(os, arch, Some(kind)).unwrap();
            let info = inspect_installer_manifest(
                &route,
                &document,
                target,
                &semver::Version::new(1, 2, 3),
            )
            .unwrap()
            .unwrap();
            assert_eq!(info.installer_type, kind);
            assert_eq!(
                info.installer_url.as_deref(),
                document["platforms"][target.manifest_key()]["url"].as_str()
            );
            assert_eq!(info.body.as_deref(), Some("Release notes"));
            assert_eq!(info.date.as_deref(), Some("2026-10-13T19:33:14.785Z"));
            assert_eq!(info.download_size, None);
            let candidates = installer_download_candidates(&info);
            assert_eq!(
                candidates.len(),
                if route.provider == Provider::Gitee {
                    1
                } else {
                    installer_manifest_sources(InstallerRegion::International).len()
                }
            );
            assert_eq!(candidates[0].route, AssetDownloadRoute::Direct);
            assert_eq!(
                Some(candidates[0].url.as_str()),
                info.installer_url.as_deref()
            );
        }
    }
}

#[test]
fn manifest_winner_does_not_restrict_the_independent_package_pool() {
    let document = fixture(Provider::Github);
    let current = semver::Version::new(1, 0, 0);
    let info = inspect_installer_manifest(&direct(Provider::Github), &document, target(), &current)
        .unwrap()
        .unwrap();
    let expected = installer_download_candidates(&info);
    assert!(expected.len() > 1);
    assert_eq!(expected[0].route, AssetDownloadRoute::Direct);
    for route in installer_manifest_sources(InstallerRegion::International)
        .iter()
        .skip(1)
    {
        let info = inspect_installer_manifest(route, &document, target(), &current)
            .unwrap()
            .unwrap();
        assert_eq!(info.manifest_url, route.endpoint);
        assert_eq!(expected, installer_download_candidates(&info));
    }
    let gitee = inspect_installer_manifest(
        &direct(Provider::Gitee),
        &fixture(Provider::Gitee),
        target(),
        &current,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        installer_download_candidates(&gitee),
        [AssetDownloadCandidate {
            route: AssetDownloadRoute::Direct,
            url: gitee.installer_url.unwrap()
        }]
    );
}

#[test]
fn malformed_missing_platform_unsafe_urls_bad_names_and_empty_signatures_fail() {
    let valid = fixture(Provider::Github);
    let mut invalid = vec![
        serde_json::json!({}),
        serde_json::json!({ "version": "invalid" }),
    ];
    let mut missing = valid.clone();
    missing["platforms"]
        .as_object_mut()
        .unwrap()
        .remove("windows-x86_64-nsis");
    invalid.push(missing);
    for signature in ["", " "] {
        let mut value = valid.clone();
        value["platforms"]["windows-x86_64-nsis"]["signature"] = signature.into();
        invalid.push(value);
    }
    for url in [
        "http://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://github.com/wrong/repo/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64_en-US.msi",
        "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_8.0.0_x64-setup.exe",
    ] {
        let mut value = valid.clone();
        value["platforms"]["windows-x86_64-nsis"]["url"] = url.into();
        invalid.push(value);
    }
    for value in invalid {
        assert!(
            inspect_installer_manifest(
                &direct(Provider::Github),
                &value,
                target(),
                &semver::Version::new(1, 0, 0)
            )
            .is_err()
        );
    }
}

#[test]
fn equal_or_older_versions_do_not_update() {
    for current in [
        semver::Version::new(9, 0, 0),
        semver::Version::new(10, 0, 0),
    ] {
        assert!(
            inspect_installer_manifest(
                &direct(Provider::Github),
                &fixture(Provider::Github),
                target(),
                &current
            )
            .unwrap()
            .is_none()
        );
    }
}

struct Server {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(responses: HashMap<String, (u16, String, Vec<u8>, Duration)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (captured, stopped) = (requests.clone(), stop.clone());
        let responses = Arc::new(responses);
        let worker = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let (captured, responses) = (captured.clone(), responses.clone());
                workers.push(std::thread::spawn(move || {
                    // Accepted sockets can inherit the listener's nonblocking
                    // mode; fragmented HTTP headers must be read to completion.
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(Duration::from_millis(100)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    while !request.windows(4).any(|value| value == b"\r\n\r\n")
                        && request.len() < 16384
                    {
                        match socket.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(size) => request.extend_from_slice(&buffer[..size]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request).into_owned();
                    let path = request
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_string();
                    captured.lock().unwrap().push(request);
                    if let Some((status, headers, body, delay)) = responses.get(&path) {
                        std::thread::sleep(*delay);
                        let response =
                            format!("HTTP/1.1 {status} Test\r\nConnection: close\r\n{headers}\r\n");
                        if socket.write_all(response.as_bytes()).is_ok() {
                            let _ = socket.write_all(body);
                        }
                    } else {
                        let _ = socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                    }
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            base,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn route(&self, path: &str, proxy_host: Option<&str>) -> InstallerChannel {
        let mut route = direct(Provider::Github);
        route.endpoint = format!("{}{path}", self.base);
        route.id = path.into();
        if let Some(host) = proxy_host {
            route.id = format!("github-proxy:{host}");
            route.proxy = Some(GitHubProxy {
                id: host.into(),
                name: host.into(),
                template: format!("https://{host}/{{url}}"),
                regions: Vec::new(),
                description: String::new(),
                enabled: true,
                builtin: true,
                remote: false,
            });
        }
        route
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn response(status: u16, body: Vec<u8>) -> (u16, String, Vec<u8>, Duration) {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body,
        Duration::ZERO,
    )
}

#[tokio::test]
async fn manifest_test_server_reads_fragmented_request_headers() {
    let server = Server::new(HashMap::from([(
        "/fragmented".into(),
        response(200, b"manifest fixture".to_vec()),
    )]));
    let address = server.base.strip_prefix("http://").unwrap().to_string();
    let reply = tokio::task::spawn_blocking(move || {
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket.write_all(b"GET /frag").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        socket
            .write_all(b"mented HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut reply = String::new();
        socket.read_to_string(&mut reply).unwrap();
        reply
    })
    .await
    .unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 Test\r\n"), "{reply}");
    assert!(reply.ends_with("manifest fixture"));
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /fragmented HTTP/1.1\r\n"));
}

#[tokio::test]
async fn github_manifests_race_and_invalid_json_does_not_win_without_tokens() {
    let bytes = serde_json::to_vec(&fixture(Provider::Github)).unwrap();
    let server = Server::new(HashMap::from([
        ("/direct".into(), delayed(200, bytes.clone(), 350)),
        ("/invalid".into(), response(200, b"not JSON".to_vec())),
        ("/slow".into(), delayed(200, bytes.clone(), 350)),
        ("/fast".into(), delayed(200, bytes, 40)),
    ]));
    let channels = [
        server.route("/direct", None),
        server.route("/invalid", Some("invalid.example")),
        server.route("/slow", Some("slow.example")),
        server.route("/fast", Some("fast.example")),
    ];
    let mut statuses = Vec::new();
    let result = run_installer_manifest_routes(
        &reqwest::Client::new(),
        &channels,
        target(),
        &semver::Version::new(1, 2, 3),
        || false,
        |status| statuses.push(status),
        |info| std::future::ready(Ok(info)),
    )
    .await
    .unwrap();
    let InstallerRouteResult::Updated(info) = result else {
        panic!("no update")
    };
    assert_eq!(
        info.source_channel.as_deref(),
        Some("github-proxy:fast.example")
    );
    assert!(
        statuses
            .iter()
            .any(|s| s.id == "github-proxy:invalid.example" && s.status == "failed")
    );
    assert_eq!(
        statuses.iter().filter(|s| s.status == "cancelled").count(),
        2
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    for request in requests.iter() {
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("cache-control: no-cache, no-store"));
        assert!(lower.contains("pragma: no-cache"));
        assert!(
            !lower.contains("authorization:")
                && !lower.contains("access_token")
                && !lower.contains("if-none-match:")
        );
    }
}

fn delayed(status: u16, body: Vec<u8>, milliseconds: u64) -> (u16, String, Vec<u8>, Duration) {
    let mut value = response(status, body);
    value.3 = Duration::from_millis(milliseconds);
    value
}

#[tokio::test]
async fn gitee_finishes_its_single_manifest_and_package_before_github_can_start() {
    for fail_package in [false, true] {
        let server = Server::new(HashMap::from([
            (
                "/gitee".into(),
                response(200, serde_json::to_vec(&fixture(Provider::Gitee)).unwrap()),
            ),
            (
                "/github".into(),
                response(200, serde_json::to_vec(&fixture(Provider::Github)).unwrap()),
            ),
        ]));
        let mut gitee = server.route("/gitee", None);
        gitee.provider = Provider::Gitee;
        gitee.id = "gitee-direct".into();
        let mut packages = Vec::new();
        let mut statuses = Vec::new();
        let result = run_installer_manifest_routes(
            &reqwest::Client::new(),
            &[gitee, server.route("/github", None)],
            target(),
            &semver::Version::new(1, 2, 3),
            || false,
            |status| statuses.push(status),
            |info| {
                packages.push(info.provider.clone());
                let server = &server;
                async move {
                    if info.provider == Some(Provider::Gitee) {
                        tokio::time::sleep(Duration::from_millis(25)).await;
                        assert_eq!(server.requests.lock().unwrap().len(), 1);
                        assert_eq!(installer_download_candidates(&info).len(), 1);
                        if fail_package {
                            return Err("signature failed".into());
                        }
                    }
                    Ok(info)
                }
            },
        )
        .await
        .unwrap();
        let InstallerRouteResult::Updated(info) = result else {
            panic!("no update; channel statuses: {statuses:?}")
        };
        assert_eq!(
            info.provider,
            Some(if fail_package {
                Provider::Github
            } else {
                Provider::Gitee
            })
        );
        assert_eq!(packages.len(), if fail_package { 2 } else { 1 });
        assert_eq!(server.requests.lock().unwrap().len(), packages.len());
    }
}

#[tokio::test]
async fn latest_gitee_skips_github_and_github_exhaustion_reports_failure() {
    let server = Server::new(HashMap::from([
        (
            "/latest".into(),
            response(200, serde_json::to_vec(&fixture(Provider::Gitee)).unwrap()),
        ),
        ("/failed".into(), response(404, Vec::new())),
    ]));
    let mut gitee = server.route("/latest", None);
    gitee.provider = Provider::Gitee;
    let mut statuses = Vec::new();
    let result = run_installer_manifest_routes(
        &reqwest::Client::new(),
        &[gitee, server.route("/unused", None)],
        target(),
        &semver::Version::new(9, 0, 0),
        || false,
        |status| statuses.push(status),
        |_| std::future::ready(Err::<(), String>("must not download".into())),
    )
    .await
    .unwrap();
    assert!(matches!(result, InstallerRouteResult::Latest));
    assert_eq!(statuses.last().unwrap().status, "skipped");
    assert_eq!(server.requests.lock().unwrap().len(), 1);
    let result = run_installer_manifest_routes(
        &reqwest::Client::new(),
        &[
            server.route("/failed", None),
            server.route("/also-failed", None),
        ],
        target(),
        &semver::Version::new(1, 0, 0),
        || false,
        |_| {},
        |_| std::future::ready(Ok(())),
    )
    .await
    .unwrap();
    assert!(matches!(result, InstallerRouteResult::Failed));
}

#[tokio::test]
async fn cancellation_interrupts_gitee_without_starting_github() {
    let server = Server::new(HashMap::from([(
        "/stalled".into(),
        delayed(
            200,
            serde_json::to_vec(&fixture(Provider::Gitee)).unwrap(),
            250,
        ),
    )]));
    let mut gitee = server.route("/stalled", None);
    gitee.provider = Provider::Gitee;
    let channels = [gitee, server.route("/unused", None)];
    let cancelled = AtomicBool::new(false);
    let client = reqwest::Client::new();
    let version = semver::Version::new(1, 0, 0);
    let request = run_installer_manifest_routes(
        &client,
        &channels,
        target(),
        &version,
        || cancelled.load(Ordering::Acquire),
        |_| {},
        |_| std::future::ready(Ok(())),
    );
    let cancel = async {
        for _ in 0..100 {
            if !server.requests.lock().unwrap().is_empty() {
                cancelled.store(true, Ordering::Release);
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("request did not start");
    };
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(request, cancel)
    })
    .await
    .unwrap();
    assert_eq!(result.err().unwrap(), "下载已取消");
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn oversized_and_empty_manifests_fail_without_json_fallback() {
    let server = Server::new(HashMap::from([
        (
            "/large".into(),
            (
                200,
                "Content-Length: 1048577\r\n".into(),
                Vec::new(),
                Duration::ZERO,
            ),
        ),
        (
            "/stream".into(),
            (200, String::new(), vec![b' '; 1048577], Duration::ZERO),
        ),
        ("/empty".into(), response(200, Vec::new())),
    ]));
    let client = reqwest::Client::new();
    for path in ["/large", "/stream", "/empty"] {
        assert!(
            fetch_installer_manifest(
                &client,
                &server.route(path, None),
                target(),
                &semver::Version::new(1, 0, 0),
                || false,
                None
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn actual_package_stream_retains_length_checks_and_unknown_size_progress() {
    let server = Server::new(HashMap::from([
        (
            "/package".into(),
            (
                200,
                String::new(),
                b"package bytes".to_vec(),
                Duration::ZERO,
            ),
        ),
        (
            "/truncated".into(),
            (
                200,
                "Content-Length: 100\r\n".into(),
                b"short".to_vec(),
                Duration::ZERO,
            ),
        ),
        ("/empty".into(), response(200, Vec::new())),
    ]));
    let client = reqwest::Client::new();
    let reporter = archive::DownloadReporter::new("update");
    let bytes = download_installer_with_timeout(
        &client,
        &format!("{}/package", server.base),
        &reporter,
        "update",
    )
    .await
    .unwrap();
    assert_eq!(bytes, b"package bytes");
    assert_eq!(reporter.snapshot().total, None);
    assert_eq!(reporter.snapshot().downloaded, bytes.len() as u64);
    for path in ["/truncated", "/empty"] {
        assert!(
            download_installer_with_timeout(
                &client,
                &format!("{}{path}", server.base),
                &reporter,
                "update"
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn package_race_rejects_bad_signatures_and_can_choose_a_different_route() {
    let key = crate::app_update::TEST_KEY;
    let signature = crate::app_update::TEST_SIGNATURE;
    let valid = b"OC2 signed update fixture".to_vec();
    let server = Server::new(HashMap::from([
        (
            "/manifest-proxy-package".into(),
            response(200, b"tampered package".to_vec()),
        ),
        ("/direct-package".into(), delayed(200, valid.clone(), 40)),
        (
            "/other-proxy-package".into(),
            delayed(200, valid.clone(), 200),
        ),
    ]));
    let mut info = inspect_installer_manifest(
        &direct(Provider::Github),
        &fixture(Provider::Github),
        target(),
        &semver::Version::new(1, 0, 0),
    )
    .unwrap()
    .unwrap();
    info.version = "1.2.0".into();
    info.source_channel = Some("github-proxy:manifest-winner".into());
    info.installer_signature = signature.into();
    let candidates = [
        ("/manifest-proxy-package", AssetDownloadRoute::Proxy),
        ("/direct-package", AssetDownloadRoute::Direct),
        ("/other-proxy-package", AssetDownloadRoute::Proxy),
    ]
    .into_iter()
    .map(|(path, route)| AssetDownloadCandidate {
        route,
        url: format!("{}{path}", server.base),
    })
    .collect();
    let bytes =
        download_verified_installer(&reqwest::Client::new(), &info, candidates, None, |bytes| {
            crate::app_update::verify_with_key(bytes, signature, key, &info.version)
        })
        .await
        .unwrap();
    assert_eq!(bytes, valid);
    assert_eq!(server.requests.lock().unwrap().len(), 3);
    assert!(crate::app_update::verify_with_key(&bytes, signature, key, "9.0.0").is_err());
}

#[tokio::test]
async fn gitee_package_uses_one_get_without_range_requests() {
    let valid = b"OC2 signed update fixture".to_vec();
    let server = Server::new(HashMap::from([(
        "/package".into(),
        response(200, valid.clone()),
    )]));
    let mut info = inspect_installer_manifest(
        &direct(Provider::Gitee),
        &fixture(Provider::Gitee),
        target(),
        &semver::Version::new(1, 0, 0),
    )
    .unwrap()
    .unwrap();
    info.version = "1.2.0".into();
    let candidates = vec![AssetDownloadCandidate {
        route: AssetDownloadRoute::Direct,
        url: format!("{}/package", server.base),
    }];
    let bytes =
        download_verified_installer(&reqwest::Client::new(), &info, candidates, None, |bytes| {
            crate::app_update::verify_with_key(
                bytes,
                crate::app_update::TEST_SIGNATURE,
                crate::app_update::TEST_KEY,
                &info.version,
            )
        })
        .await
        .unwrap();
    assert_eq!(bytes, valid);
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /package "));
    assert!(!requests[0].to_ascii_lowercase().contains("range:"));
}

#[tokio::test]
async fn exhausted_github_package_pool_ends_the_round_without_mixing_manifests() {
    let server = Server::new(HashMap::from([
        (
            "/first".into(),
            response(200, serde_json::to_vec(&fixture(Provider::Github)).unwrap()),
        ),
        (
            "/slow".into(),
            delayed(
                200,
                serde_json::to_vec(&fixture(Provider::Github)).unwrap(),
                200,
            ),
        ),
    ]));
    let mut attempts = 0;
    let result = run_installer_manifest_routes(
        &reqwest::Client::new(),
        &[server.route("/first", None), server.route("/slow", None)],
        target(),
        &semver::Version::new(1, 0, 0),
        || false,
        |_| {},
        |_| {
            attempts += 1;
            std::future::ready(Err::<(), String>("all package signatures failed".into()))
        },
    )
    .await
    .unwrap();
    assert!(matches!(result, InstallerRouteResult::Failed));
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn github_pool_has_at_most_three_live_sources_and_drops_losers() {
    use std::sync::atomic::AtomicUsize;
    struct Active(Arc<AtomicUsize>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let live = Arc::new(AtomicUsize::new(0));
    let peak = AtomicUsize::new(0);
    let channels = (0..6)
        .map(|index| {
            let mut channel = direct(Provider::Github);
            channel.id = index.to_string();
            channel.endpoint = format!("https://source{index}.example/latest.json");
            channel
        })
        .collect::<Vec<_>>();
    let bytes = race_installer_channels(
        &channels,
        || false,
        |channel, _| {
            let active = Active(live.clone());
            peak.fetch_max(live.fetch_add(1, Ordering::AcqRel) + 1, Ordering::AcqRel);
            async move {
                let _active = active;
                tokio::time::sleep(Duration::from_millis(if channel.id == "2" {
                    10
                } else {
                    200
                }))
                .await;
                Ok(channel.id.into_bytes())
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(bytes, b"2");
    assert_eq!(peak.load(Ordering::Acquire), 3);
    assert_eq!(live.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn cancellation_drops_the_whole_github_pool() {
    let server = Server::new(
        (0..5)
            .map(|index| {
                (
                    format!("/{index}"),
                    delayed(
                        200,
                        serde_json::to_vec(&fixture(Provider::Github)).unwrap(),
                        250,
                    ),
                )
            })
            .collect(),
    );
    let channels = (0..5)
        .map(|index| server.route(&format!("/{index}"), None))
        .collect::<Vec<_>>();
    let cancelled = AtomicBool::new(false);
    let client = reqwest::Client::new();
    let current = semver::Version::new(1, 0, 0);
    let request = run_installer_manifest_routes(
        &client,
        &channels,
        target(),
        &current,
        || cancelled.load(Ordering::Acquire),
        |_| {},
        |_| std::future::ready(Ok(())),
    );
    let cancel = async {
        for _ in 0..100 {
            if server.requests.lock().unwrap().len() >= 3 {
                cancelled.store(true, Ordering::Release);
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("parallel requests did not start");
    };
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(request, cancel)
    })
    .await
    .unwrap();
    assert_eq!(result.err().unwrap(), "下载已取消");
    assert_eq!(server.requests.lock().unwrap().len(), 3);
}
