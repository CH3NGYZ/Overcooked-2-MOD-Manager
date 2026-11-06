use super::*;
use crate::archive::DownloadRoute;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Server = (
    DownloadCandidate,
    Arc<Mutex<String>>,
    std::thread::JoinHandle<()>,
);

fn serve(status: &str, body: Vec<u8>, delay: Duration, declared_size: Option<usize>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/manifest.toml", listener.local_addr().unwrap());
    let request = Arc::new(Mutex::new(String::new()));
    let captured = request.clone();
    let status = status.to_string();
    let thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() > deadline {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("{error}"),
            }
        };
        // macOS may inherit the listener's nonblocking mode; wait for complete headers.
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buf = [0; 1024];
        while !bytes.ends_with(b"\r\n\r\n") {
            let Ok(count) = stream.read(&mut buf) else {
                return;
            };
            if count == 0 {
                return;
            }
            bytes.extend_from_slice(&buf[..count]);
        }
        *captured.lock().unwrap() = String::from_utf8(bytes).unwrap();
        std::thread::sleep(delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            declared_size.unwrap_or(body.len())
        );
        let _ = stream.write_all(&body);
    });
    (
        DownloadCandidate {
            route: DownloadRoute::Direct,
            url,
        },
        request,
        thread,
    )
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

const MANIFEST: &str = r#"schema_version = 1
[[release_proxies]]
id = "online"
name = "Online"
template = "https://online.example/{url_path}"
[[github_api_proxies]]
id = "api"
name = "API"
template = "https://api.example/{url_path}"
"#;

#[tokio::test]
async fn valid_manifest_atomically_replaces_all_groups_without_merging() {
    let runtime = ProxyRuntime::default();
    let before = runtime.snapshot();
    let (route, request, server) = serve(
        "200 OK",
        MANIFEST.as_bytes().to_vec(),
        Duration::from_millis(50),
        None,
    );
    let client = client();
    let (result, _) = tokio::join!(runtime.refresh_from(&client, vec![route]), async {
        tokio::task::yield_now().await;
        assert_eq!(
            runtime.snapshot(),
            before,
            "in-flight request must not publish a partial list"
        );
    });
    result.unwrap();
    server.join().unwrap();
    let after = runtime.snapshot();
    assert_eq!(after.release_proxies.len(), 1);
    assert_eq!(after.release_proxies[0].id, "online");
    assert!(after.release_proxies[0].remote);
    assert!(!after.release_proxies[0].builtin);
    assert_eq!(after.github_api_proxies.len(), 1);
    assert!(after.raw_proxies.is_empty());
    assert!(after.avatar_proxies.is_empty());
    assert!(after.gitee_api_proxies.is_empty());
    let request = request.lock().unwrap().to_ascii_lowercase();
    assert!(!request.contains("authorization:"));
    assert!(!request.contains("cookie:"));
    assert!(request.contains("no-cache, no-store"));
    let (route, _, server) = serve(
        "200 OK",
        b"schema_version = 1".to_vec(),
        Duration::ZERO,
        None,
    );
    runtime.refresh_from(&client, vec![route]).await.unwrap();
    server.join().unwrap();
    assert_eq!(runtime.snapshot(), GitHubProxyConfig::default());
    assert_eq!(
        ProxyRuntime::default().snapshot(),
        before,
        "new process starts with bundled fallback"
    );
}

#[tokio::test]
async fn bad_responses_and_timeouts_preserve_current_snapshot() {
    let runtime = ProxyRuntime::default();
    runtime.replace_for_test(github_proxies::parse_manifest(MANIFEST).unwrap());
    let before = runtime.snapshot();
    for (status, body, size) in [
        ("503 Unavailable", MANIFEST.as_bytes().to_vec(), None),
        ("200 OK", Vec::new(), None),
        ("200 OK", b"<html>failure</html>".to_vec(), None),
        ("200 OK", b"unrelated = true".to_vec(), None),
        ("200 OK", b"schema_version = 2".to_vec(), None),
        (
            "200 OK",
            b"schema_version = 1\n[[release_proxies]]\nid = 'incomplete'".to_vec(),
            None,
        ),
        ("200 OK", vec![255], None),
        ("200 OK", Vec::new(), Some(1024 * 1024 + 1)),
        ("200 OK", b"schema_version = 1".to_vec(), Some(100)),
    ] {
        let (route, _, server) = serve(status, body, Duration::ZERO, size);
        assert!(runtime.refresh_from(&client(), vec![route]).await.is_err());
        server.join().unwrap();
        assert_eq!(runtime.snapshot(), before);
    }
    let (route, _, server) = serve(
        "200 OK",
        MANIFEST.as_bytes().to_vec(),
        Duration::from_millis(150),
        None,
    );
    let short_client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(30))
        .build()
        .unwrap();
    assert!(
        runtime
            .refresh_from(&short_client, vec![route])
            .await
            .is_err()
    );
    server.join().unwrap();
    assert_eq!(runtime.snapshot(), before);
}

#[tokio::test]
async fn invalid_fast_response_cannot_win_over_valid_manifest() {
    let runtime = ProxyRuntime::default();
    let (bad, _, bad_server) = serve(
        "200 OK",
        b"<html>blocked</html>".to_vec(),
        Duration::ZERO,
        None,
    );
    let (good, _, good_server) = serve(
        "200 OK",
        MANIFEST.as_bytes().to_vec(),
        Duration::from_millis(40),
        None,
    );
    runtime
        .refresh_from(&client(), vec![bad, good])
        .await
        .unwrap();
    bad_server.join().unwrap();
    good_server.join().unwrap();
    assert_eq!(runtime.snapshot().release_proxies[0].id, "online");
}

#[test]
fn routes_are_official_sibling_documents_with_direct_github_first() {
    let snapshot = github_proxies::bundled_defaults();
    let international = manifest_routes(crate::config::REGION_INTERNATIONAL, &snapshot).unwrap();
    assert_eq!(international.len(), 1);
    assert_eq!(international[0][0].route, DownloadRoute::Direct);
    assert_eq!(
        international[0][0].url,
        repository_sources::DEFAULT_INDEX_GITHUB_URL
            .replace("repositories.toml", "repositories-proxies.toml")
    );
    let mainland = manifest_routes(crate::config::REGION_CHINESE_MAINLAND, &snapshot).unwrap();
    assert_eq!(mainland.len(), 2);
    assert_eq!(mainland[0].len(), 1);
    assert_eq!(
        mainland[0][0].url,
        repository_sources::DEFAULT_INDEX_GITEE_URL
            .replace("repositories.toml", "repositories-proxies.toml")
    );
    assert_eq!(mainland[1], international[0]);
    let empty = manifest_routes(
        crate::config::REGION_INTERNATIONAL,
        &GitHubProxyConfig::default(),
    )
    .unwrap();
    assert_eq!(
        empty[0].len(),
        1,
        "empty online snapshot cannot resurrect bundled routes"
    );
}

#[tokio::test]
#[ignore = "read-only live network: official manifest, no config or game writes"]
async fn live_official_proxy_manifest() {
    let runtime = ProxyRuntime::default();
    runtime
        .refresh(crate::config::REGION_INTERNATIONAL)
        .await
        .unwrap();
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.release_proxies.len(), 1);
    assert_eq!(snapshot.raw_proxies.len(), 1);
    let github = "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/repositories.toml";
    let raw = "https://raw.githubusercontent.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/main/repositories.toml";
    let github_urls = github_proxies::proxy_urls(&snapshot.raw_proxies, github, true, None);
    let raw_urls = github_proxies::proxy_urls(&snapshot.raw_proxies, raw, true, None);
    assert_eq!(raw_urls, github_urls);
    let client = crate::releases::build_client(30).unwrap();
    let reporter = crate::archive::DownloadReporter::new("live-proxy-probe");
    let bytes =
        repository_sources::fetch_document_bytes(&client, &raw_urls[0], &reporter, None, None)
            .await
            .unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    repository_sources::parse_document(text, &SourceLocation::parse(github).unwrap()).unwrap();
    println!(
        "live Raw: bytes={} equivalent_hosts=true parsed=true",
        bytes.len()
    );

    let api = github_proxies::proxy_urls(
        &snapshot.github_api_proxies,
        "https://api.github.com/repos/CH3NGYZ/oc2/releases/tags/muxixiao",
        false,
        None,
    );
    let metadata =
        repository_sources::fetch_document_bytes(&client, &api[0], &reporter, None, None)
            .await
            .unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&metadata).unwrap();
    let asset = metadata["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["name"] == "muxixiao_v0.7.zip")
        .unwrap();
    let url = asset["browser_download_url"].as_str().unwrap();
    let expected = asset["size"].as_u64().unwrap();
    let digest = asset["digest"]
        .as_str()
        .expect("live fixture must expose SHA-256");
    let urls = github_proxies::proxy_urls(&snapshot.release_proxies, url, false, None);
    let directory = std::env::temp_dir().join(format!("oc2-live-proxy-{}", uuid::Uuid::new_v4()));
    let started = std::time::Instant::now();
    let result = crate::download::try_download_candidates(
        ordered_asset_download_candidates(ProxyMode::AutoDirect, url, urls),
        || false,
        |candidate, reporter| {
            let directory = &directory;
            let client = &client;
            async move {
                let path = crate::archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    directory,
                    "live-proxy-probe",
                    &reporter,
                    1,
                    Some(crate::archive::FIRST_BYTE_TIMEOUT),
                )
                .await?;
                if let Err(error) =
                    crate::archive::validate_download_file(&path, Some(expected), Some(digest))
                        .and_then(|_| crate::archive::validate_zip(&path))
                {
                    let _ = std::fs::remove_file(&path);
                    return Err(error);
                }
                Ok(path)
            }
        },
        None,
    )
    .await;
    let elapsed = started.elapsed();
    let file_count = std::fs::read_dir(&directory)
        .map(|files| files.count())
        .unwrap_or(0);
    if let Ok(path) = &result {
        std::fs::remove_file(path).unwrap();
    }
    let remaining = std::fs::read_dir(&directory)
        .map(|files| files.count())
        .unwrap_or(0);
    if directory.exists() {
        std::fs::remove_dir_all(&directory).unwrap();
    }
    result.unwrap();
    assert_eq!(
        file_count, 1,
        "only the winner remains before consumer cleanup"
    );
    assert_eq!(remaining, 0);
    println!(
        "live ZIP: bytes={expected} digest_ok=true zip_ok=true elapsed_ms={} losers_remaining=0 final_remaining={remaining}",
        elapsed.as_millis()
    );
    println!(
        "live proxies: release={} raw={} github_api={} gitee_api={} avatar={}",
        snapshot.release_proxies.len(),
        snapshot.raw_proxies.len(),
        snapshot.github_api_proxies.len(),
        snapshot.gitee_api_proxies.len(),
        snapshot.avatar_proxies.len()
    );
}
