use super::channels::*;
use super::download::{download_installer, download_verified_installer};
use super::manifest::*;
use crate::archive::{self, DownloadRoute as AssetDownloadRoute};
use crate::download::DownloadExecution;
use crate::models::*;
use crate::network_policy::{NetworkPolicy, Step};
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod manifest_changelog_tests;
mod manifest_mirror_tests;

#[test]
fn installer_channels_follow_the_configured_region_per_call() {
    let policy = NetworkPolicy::shared().unwrap();
    for (configured, expected) in [
        ("cn", InstallerRegion::MainlandChina),
        ("global", InstallerRegion::International),
        ("cn", InstallerRegion::MainlandChina),
    ] {
        let region = installer_region(configured);
        assert_eq!(region, expected);
        let sources = installer_manifest_sources(policy, region);
        assert_eq!(
            sources.first().unwrap().provider,
            if configured == "cn" {
                Provider::Gitee
            } else {
                Provider::Github
            },
        );
    }
}

fn target() -> crate::app_update::UpdateTarget {
    crate::app_update::UpdateTarget::from_parts("windows", "x86_64", Some("exe")).unwrap()
}

fn fixture(provider: Provider) -> serde_json::Value {
    let base = match provider {
        Provider::Github => "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager",
        Provider::Gitee => "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager",
        Provider::Gitcode => "https://gitcode.com/CH3NGYZ/Overcooked-2-Manager",
    };
    let platforms = [
        ("windows-x86_64-nsis", "OC2ModManager_9.0.0_x64-setup.exe"),
        ("windows-x86_64-msi", "OC2ModManager_9.0.0_x64_en-US.msi"),
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
    installer_sources_for_url(
        NetworkPolicy::shared().unwrap(),
        InstallerRegion::International,
        &provider,
        &installer_manifest_url(&provider),
    )
    .unwrap()
    .remove(0)
}

fn direct_test_policy() -> NetworkPolicy {
    let mut policy = NetworkPolicy::shared().unwrap().clone();
    let release = &mut policy.plans.get_mut("github-mainland").unwrap().release;
    release.route = vec![Step::Direct];
    release.max_parallelism = 1;
    release.race = false;
    policy
}

#[test]
fn region_order_uses_all_and_only_the_supplied_snapshot() {
    let policy = NetworkPolicy::shared().unwrap();
    let mainland = installer_manifest_sources(policy, InstallerRegion::MainlandChina);
    let international = installer_manifest_sources(policy, InstallerRegion::International);
    assert_eq!(mainland[0].id, "gitee-direct");
    assert_eq!(mainland[1].id, "gitcode-direct");
    assert_eq!(mainland[2].id, "github-direct");
    assert_eq!(
        international
            .iter()
            .map(|channel| channel.id.as_str())
            .collect::<Vec<_>>(),
        [
            "github-direct",
            "github-proxy:oc2gh-release-cloudflare",
            "github-proxy:oc2gh-release-edgeone"
        ]
    );
    assert!(international.iter().all(|channel| channel.execution.race));
    assert!(mainland[2..].iter().all(|channel| channel.execution.race));
    assert_eq!(
        mainland.len(),
        mainland
            .iter()
            .map(|route| &route.endpoint)
            .collect::<HashSet<_>>()
            .len()
    );
    let mut saved = policy.clone();
    let release = &mut saved.plans.get_mut("github-public").unwrap().release;
    release.route = vec![Step::Proxy {
        id: "git-yylx".into(),
    }];
    release.max_parallelism = 1;
    release.race = false;
    saved.proxies.get_mut("git-yylx").unwrap().template = "https://custom.example/{url}".into();
    let supplied = installer_manifest_sources(&saved, InstallerRegion::International);
    assert_eq!(supplied.len(), 1);
    assert_eq!(supplied[0].id, "github-proxy:git-yylx");
    assert!(supplied[0].endpoint.starts_with("https://custom.example/"));
    assert_eq!(supplied[0].route, AssetDownloadRoute::Proxy);
    assert!(!supplied[0].execution.race);
}

#[test]
fn each_released_format_is_selected_from_json_without_api_size_or_digest() {
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        let route = direct(provider.clone());
        let github = provider == Provider::Github;
        let document = fixture(provider);
        for (os, arch, kind) in [
            ("windows", "x86_64", "exe"),
            ("windows", "x86_64", "msi"),
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
            let candidates = package_channels(&info, InstallerRegion::International);
            assert_eq!(candidates.len(), if github { 3 } else { 1 });
            assert_eq!(candidates[0].route, AssetDownloadRoute::Direct);
            assert_eq!(
                Some(candidates[0].endpoint.as_str()),
                info.installer_url.as_deref()
            );
            if github {
                let path = candidates[0]
                    .endpoint
                    .strip_prefix("https://github.com")
                    .unwrap();
                for (candidate, host) in candidates[1..]
                    .iter()
                    .zip(["oc2gh.ch3ng.top", "oc2gh.5882778.xyz"])
                {
                    assert_eq!(candidate.route, AssetDownloadRoute::Proxy);
                    assert_eq!(candidate.endpoint, format!("https://{host}{path}"));
                }
            }
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
    let region = InstallerRegion::MainlandChina;
    let expected: Vec<_> = package_channels(&info, region)
        .iter()
        .map(installer_candidate)
        .collect();
    assert!(expected.len() > 1);
    assert_eq!(expected[0].route, AssetDownloadRoute::Direct);
    for route in installer_manifest_sources(NetworkPolicy::shared().unwrap(), region)
        .iter()
        .filter(|channel| {
            channel.provider == Provider::Github && channel.route == AssetDownloadRoute::Proxy
        })
    {
        let info = inspect_installer_manifest(route, &document, target(), &current)
            .unwrap()
            .unwrap();
        assert_eq!(info.manifest_url, route.endpoint);
        assert_eq!(
            expected,
            package_channels(&info, region)
                .iter()
                .map(installer_candidate)
                .collect::<Vec<_>>()
        );
    }
    let gitee = inspect_installer_manifest(
        &direct(Provider::Gitee),
        &fixture(Provider::Gitee),
        target(),
        &current,
    )
    .unwrap()
    .unwrap();
    let package = package_channels(&gitee, region);
    assert_eq!(package.len(), 1);
    assert_eq!(package[0].route, AssetDownloadRoute::Direct);
    assert_eq!(
        Some(package[0].endpoint.as_str()),
        gitee.installer_url.as_deref()
    );
}

fn package_channels(info: &AppUpdateInfo, region: InstallerRegion) -> Vec<InstallerChannel> {
    installer_sources_for_url(
        NetworkPolicy::shared().unwrap(),
        region,
        info.provider.as_ref().unwrap(),
        info.installer_url.as_deref().unwrap(),
    )
    .unwrap()
}

#[test]
fn malformed_manifests_unsafe_urls_and_bad_package_names_fail() {
    let valid = fixture(Provider::Github);
    let mut invalid = vec![
        serde_json::json!({}),
        serde_json::json!({ "version": "invalid" }),
    ];
    for url in [
        "http://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://github.com/wrong/repo/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://gitcode.com/CH3NGYZ/untrusted/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://github.com.evil.example/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://user@github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe",
        "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe?token=x",
        "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe#fragment",
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

#[test]
fn empty_signature_becomes_a_confirmed_manual_release() {
    let mut document = fixture(Provider::Github);
    document["platforms"]["windows-x86_64-nsis"]["signature"] = String::new().into();
    let info = inspect_installer_manifest(
        &direct(Provider::Github),
        &document,
        target(),
        &semver::Version::new(1, 0, 0),
    )
    .unwrap()
    .unwrap();
    assert_eq!(info.version, "9.0.0");
    assert!(info.installer_url.is_none());
    assert_eq!(info.installer_signature, "");
    assert_eq!(info.installer_name, "OC2ModManager_9.0.0_x64-setup.exe");
    assert_eq!(info.install_mode, AppUpdateInstallMode::Manual);
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
        route.execution = DownloadExecution {
            max_parallelism: 3,
            race: true,
        };
        route.endpoint = format!("{}{path}", self.base);
        route.id = path.into();
        if let Some(host) = proxy_host {
            route.id = format!("github-proxy:{host}");
            route.route = AssetDownloadRoute::Proxy;
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
        &crate::models::NetworkConfig::default(),
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
                        assert_eq!(
                            package_channels(&info, InstallerRegion::MainlandChina).len(),
                            1
                        );
                        if fail_package {
                            return Err("signature failed".into());
                        }
                    }
                    Ok(info)
                }
            },
            &crate::models::NetworkConfig::default(),
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
        &crate::models::NetworkConfig::default(),
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
        &crate::models::NetworkConfig::default(),
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
    let network = crate::models::NetworkConfig::default();
    let request = run_installer_manifest_routes(
        &client,
        &channels,
        target(),
        &version,
        || cancelled.load(Ordering::Acquire),
        |_| {},
        |_| std::future::ready(Ok(())),
        &network,
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
    assert_eq!(result.err().unwrap(), "download.cancelled");
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
    let bytes = download_installer(
        &client,
        &format!("{}/package", server.base),
        &reporter,
        "update",
        &crate::models::NetworkConfig::default(),
        || false,
    )
    .await
    .unwrap();
    assert_eq!(bytes, b"package bytes");
    assert_eq!(reporter.snapshot().total, None);
    assert_eq!(reporter.snapshot().downloaded, bytes.len() as u64);
    for path in ["/truncated", "/empty"] {
        assert!(
            download_installer(
                &client,
                &format!("{}{path}", server.base),
                &reporter,
                "update",
                &crate::models::NetworkConfig::default(),
                || false,
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn cancelling_a_stalled_installer_does_not_cancel_another_request() {
    let package = b"second package".to_vec();
    let server = Server::new(HashMap::from([
        ("/stalled".into(), delayed(200, package.clone(), 1000)),
        ("/other".into(), delayed(200, package.clone(), 100)),
    ]));
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let network = NetworkConfig::default();
    let cancelled = AtomicBool::new(false);
    let cancelled_reporter = archive::DownloadReporter::new("stalled");
    let other_reporter = archive::DownloadReporter::new("other");
    let stalled_url = format!("{}/stalled", server.base);
    let other_url = format!("{}/other", server.base);
    let stalled = download_installer(
        &client,
        &stalled_url,
        &cancelled_reporter,
        "stalled",
        &network,
        || cancelled.load(Ordering::Acquire),
    );
    let other = download_installer(
        &client,
        &other_url,
        &other_reporter,
        "other",
        &network,
        || false,
    );
    let cancel = async {
        while server.requests.lock().unwrap().len() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        cancelled.store(true, Ordering::Release);
    };
    let (result, other_result, ()) = tokio::time::timeout(Duration::from_millis(800), async {
        tokio::join!(stalled, other, cancel)
    })
    .await
    .expect("the cancelled request should finish before the stalled response");
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(cancelled_reporter.snapshot().downloaded, 0);
    assert_eq!(other_result.unwrap(), package);
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
        server.route("/manifest-proxy-package", Some("manifest-winner")),
        server.route("/direct-package", None),
        server.route("/other-proxy-package", Some("other-proxy")),
    ];
    let version = info.version.clone();
    let bytes = download_verified_installer(
        &reqwest::Client::new(),
        &info,
        &candidates,
        None,
        InstallerRegion::MainlandChina,
        move |bytes| crate::app_update::verify_with_key(bytes, signature, key, &version),
        &crate::models::NetworkConfig::default(),
        || false,
        None,
    )
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
    let mut channel = direct(Provider::Gitee);
    channel.endpoint = format!("{}/package", server.base);
    let candidates = [channel];
    let progress = Arc::new(Mutex::new(Vec::new()));
    let captured = progress.clone();
    let version = info.version.clone();
    let bytes = archive::observe_download_progress(
        false,
        move |value| captured.lock().unwrap().push(value),
        download_verified_installer(
            &reqwest::Client::new(),
            &info,
            &candidates,
            None,
            InstallerRegion::MainlandChina,
            move |bytes| {
                crate::app_update::verify_with_key(
                    bytes,
                    crate::app_update::TEST_SIGNATURE,
                    crate::app_update::TEST_KEY,
                    &version,
                )
            },
            &crate::models::NetworkConfig::default(),
            || false,
            None,
        ),
    )
    .await
    .unwrap();
    assert_eq!(bytes, valid);
    let seen = progress.lock().unwrap();
    let finished = seen.last().unwrap();
    assert!(finished.finished);
    assert_eq!(finished.route, Some(AssetDownloadRoute::Direct));
    assert_eq!(finished.downloaded, valid.len() as u64);
    assert!(finished.race.is_none());
    assert!(seen.iter().all(|p| {
        p.label == "update.downloadingInstaller"
            && p.file_name.as_deref() == Some(info.installer_name.as_str())
    }));
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
        &crate::models::NetworkConfig::default(),
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
            channel.execution = DownloadExecution {
                max_parallelism: 3,
                race: true,
            };
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
        &crate::models::NetworkConfig::default(),
        None,
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
    let network = crate::models::NetworkConfig::default();
    let request = run_installer_manifest_routes(
        &client,
        &channels,
        target(),
        &current,
        || cancelled.load(Ordering::Acquire),
        |_| {},
        |_| std::future::ready(Ok(())),
        &network,
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
    assert_eq!(result.err().unwrap(), "download.cancelled");
    assert_eq!(server.requests.lock().unwrap().len(), 3);
}
