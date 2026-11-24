use super::*;
use crate::archive::DownloadRoute;
use crate::download::{DownloadCandidate, DownloadExecution};
use crate::models::{NetworkPlanSelection, Provider, RepositoryKind};
use std::io::{Read, Write};
use std::net::TcpListener;

fn server(
    status: u16,
    delay: Duration,
    body: Vec<u8>,
) -> (String, std::thread::JoinHandle<String>) {
    server_with_body_delay(status, delay, Duration::ZERO, body)
}

fn server_with_body_delay(
    status: u16,
    delay: Duration,
    body_delay: Duration,
    body: Vec<u8>,
) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/README.md", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let started = Instant::now();
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "README request did not start"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut chunk = [0; 2048];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&chunk[..count]);
        }
        std::thread::sleep(delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        if let Some((first, rest)) = body.split_first() {
            let _ = stream.write_all(&[*first]);
            std::thread::sleep(body_delay);
            let _ = stream.write_all(rest);
        }
        String::from_utf8(request).unwrap()
    });
    (url, task)
}

fn candidate(url: String, route: DownloadRoute) -> DownloadCandidate {
    DownloadCandidate {
        url,
        route,
        identity: None,
    }
}

fn planned(candidates: Vec<DownloadCandidate>, max_parallelism: u32) -> RepositoryDownloadPlan {
    RepositoryDownloadPlan {
        candidates,
        download_threads: 1,
        execution: DownloadExecution {
            max_parallelism,
            race: max_parallelism > 1,
        },
    }
}

async fn finish_server(task: std::thread::JoinHandle<String>) -> String {
    // Closing a rejected response may need the runtime before the server write can finish.
    tokio::task::spawn_blocking(move || task.join().unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn finishing_readme_server_does_not_block_async_connection_cleanup() {
    let (closed, connection_closed) = std::sync::mpsc::channel();
    let task = std::thread::spawn(move || {
        connection_closed
            .recv_timeout(Duration::from_secs(2))
            .expect("README server cleanup starved the current-thread async runtime");
        String::new()
    });
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1)).await;
        let _ = closed.send(());
    });
    finish_server(task).await;
}

#[test]
fn gitee_readme_url_uses_its_raw_branch_layout() {
    assert_eq!(
        releases::repository_file_url(
            &Provider::Gitee,
            "ch3ngyz",
            "Overcooked-2-HostUtilities-Stable-Releases",
            "main",
            "README.md"
        )
        .unwrap(),
        "https://gitee.com/ch3ngyz/Overcooked-2-HostUtilities-Stable-Releases/raw/main/README.md"
    );
}

#[tokio::test]
async fn gitee_readme_makes_one_direct_get_without_ranges_tokens_or_proxy_requests() {
    let client = Client::builder().no_proxy().build().unwrap();
    for status in [200, 404] {
        let (url, task) = server(status, Duration::ZERO, b"# Gitee README".to_vec());
        let plan = plan_repository_download(
            &Provider::Gitee,
            RepositoryKind::Mod,
            &NetworkPlanSelection::default(),
            "cn",
            &url,
            Some(RawProxyContext {
                branch: "main",
                file_path: "README.md",
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(plan.candidates, [candidate(url, DownloadRoute::Direct)]);
        assert_eq!(plan.execution.max_parallelism, 1);
        let result = download_readme(
            &client,
            plan,
            &AtomicBool::new(false),
            &NetworkConfig::default(),
        )
        .await;
        if status == 200 {
            assert_eq!(result.unwrap(), b"# Gitee README");
        } else {
            assert_eq!(result.unwrap_err(), NOT_FOUND);
        }
        let request = finish_server(task).await.to_ascii_lowercase();
        assert!(request.starts_with("get /readme.md http/1.1\r\n"));
        assert!(!request.contains("range:"));
        assert!(!request.contains("authorization:"));
        assert!(request.contains("cache-control: no-cache"));
    }
}

#[tokio::test]
async fn any_started_404_stops_the_real_raw_race_without_waiting_for_slow_success() {
    for missing_route in [DownloadRoute::Direct, DownloadRoute::Proxy] {
        let (slow, slow_task) = server(200, Duration::from_millis(500), b"# Late README".to_vec());
        let (missing, missing_task) = server(404, Duration::from_millis(30), vec![]);
        let client = Client::builder().no_proxy().build().unwrap();
        let started = Instant::now();
        let result = download_readme(
            &client,
            planned(
                vec![
                    candidate(slow, DownloadRoute::Direct),
                    candidate(missing, missing_route),
                ],
                2,
            ),
            &AtomicBool::new(false),
            &NetworkConfig::default(),
        )
        .await;
        assert_eq!(result.unwrap_err(), NOT_FOUND);
        assert!(started.elapsed() < Duration::from_millis(400));
        for request in [
            finish_server(slow_task).await,
            finish_server(missing_task).await,
        ] {
            let request = request.to_ascii_lowercase();
            assert!(request.contains("cache-control: no-cache"));
            assert!(!request.contains("authorization:"));
        }
    }
}

#[tokio::test]
async fn ordinary_failures_do_not_abort_the_raw_race_and_utf8_is_required() {
    let (failed, failed_task) = server(500, Duration::ZERO, vec![]);
    let (success, success_task) =
        server(200, Duration::from_millis(30), b"# README\nBody".to_vec());
    let client = Client::builder().no_proxy().build().unwrap();
    let content = download_readme(
        &client,
        planned(
            vec![
                candidate(failed, DownloadRoute::Direct),
                candidate(success, DownloadRoute::Proxy),
            ],
            2,
        ),
        &AtomicBool::new(false),
        &NetworkConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(content, b"# README\nBody");
    finish_server(failed_task).await;
    finish_server(success_task).await;

    let (invalid, task) = server(200, Duration::ZERO, vec![0xff]);
    assert!(
        fetch_readme(
            &client,
            &invalid,
            &DownloadReporter::new("README"),
            &NetworkConfig::default(),
        )
        .await
        .unwrap_err()
        .contains("readme.invalidUtf8")
    );
    finish_server(task).await;
}

#[test]
fn readme_plan_expands_the_repository_raw_override_with_the_exact_branch() {
    let selection = toml::from_str(
        "international = { raw = { route = [{ kind = 'proxy', id = 'oc2gh-raw-edgeone' }] } }",
    )
    .unwrap();
    let url = "https://github.com/acme/mod/raw/refs/heads/release/1.x/README.md";
    let plan = plan_repository_download(
        &Provider::Github,
        RepositoryKind::Mod,
        &selection,
        "global",
        url,
        Some(RawProxyContext {
            branch: "release/1.x",
            file_path: "README.md",
        }),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        plan.candidates,
        [candidate(
            "https://oc2gh.5882778.xyz/acme/mod/raw/refs/heads/release/1.x/README.md".into(),
            DownloadRoute::Proxy,
        )]
    );
    assert!(plan.execution.race);
    assert_eq!(plan.execution.max_parallelism, 3);
}

#[tokio::test]
async fn sequential_readme_fallback_gets_a_new_full_body_timeout() {
    let (stalled, stalled_task) = server_with_body_delay(
        200,
        Duration::ZERO,
        Duration::from_secs(6),
        b"# Stalled README".to_vec(),
    );
    let (backup, backup_task) =
        server(200, Duration::from_millis(300), b"# Backup README".to_vec());
    let client = Client::builder().no_proxy().build().unwrap();
    let started = Instant::now();
    let content = download_readme(
        &client,
        planned(
            vec![
                candidate(stalled, DownloadRoute::Direct),
                candidate(backup, DownloadRoute::Proxy),
            ],
            1,
        ),
        &AtomicBool::new(false),
        &NetworkConfig {
            api_timeout_seconds: 5,
            first_byte_timeout_seconds: 10,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(content, b"# Backup README");
    assert!(started.elapsed() >= Duration::from_secs(5));
    for request in [
        finish_server(stalled_task).await,
        finish_server(backup_task).await,
    ] {
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }
}

#[tokio::test]
async fn raw_readme_size_and_pre_cancelled_downloads_are_bounded() {
    let (large, task) = server(200, Duration::ZERO, vec![b'x'; MAX_README_BYTES + 1]);
    let client = Client::builder().no_proxy().build().unwrap();
    assert!(
        fetch_readme(
            &client,
            &large,
            &DownloadReporter::new("README"),
            &NetworkConfig::default(),
        )
        .await
        .unwrap_err()
        .contains("readme.tooLarge")
    );
    finish_server(task).await;
    let result = download_readme(
        &client,
        planned(
            vec![candidate(
                "http://127.0.0.1:1".into(),
                DownloadRoute::Direct,
            )],
            1,
        ),
        &AtomicBool::new(true),
        &NetworkConfig::default(),
    )
    .await;
    assert_eq!(result.unwrap_err(), "download.cancelled");
}

#[test]
fn readme_cancellation_is_request_scoped_and_survives_cancel_before_begin() {
    let downloads = ReadmeDownloads::default();
    downloads.cancel("closed-before-start");
    let cancelled = downloads.begin("closed-before-start").unwrap();
    assert!(cancelled.cancelled.load(Ordering::Acquire));
    let other = downloads.begin("other-request").unwrap();
    assert!(!other.cancelled.load(Ordering::Acquire));
    assert!(downloads.begin("other-request").is_err());
    assert!(downloads.begin("../invalid").is_err());
    downloads.cancel("other-request");
    assert!(other.cancelled.load(Ordering::Acquire));
    drop(other);
    drop(cancelled);
    assert!(downloads.requests.lock().unwrap().is_empty());
}
