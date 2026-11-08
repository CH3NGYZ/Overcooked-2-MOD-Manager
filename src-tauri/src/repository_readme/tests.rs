use super::*;
use crate::archive::DownloadRoute;
use std::io::{Read, Write};
use std::net::TcpListener;

fn server(
    status: u16,
    delay: Duration,
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
                started.elapsed() < Duration::from_secs(5),
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
        let _ = stream.write_all(&body);
        String::from_utf8(request).unwrap()
    });
    (url, task)
}

fn candidate(url: String, route: DownloadRoute) -> DownloadCandidate {
    DownloadCandidate { url, route }
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
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let proxies = vec![GitHubProxy {
        id: "unused-raw-proxy".into(),
        name: "Unused proxy".into(),
        template: format!("http://{}/{{url}}", proxy.local_addr().unwrap()),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];
    let client = Client::builder().no_proxy().build().unwrap();
    for status in [200, 404] {
        let (url, task) = server(status, Duration::ZERO, b"# Gitee README".to_vec());
        let result = download_repository_readme(
            &client,
            &Provider::Gitee,
            &url,
            "main",
            &proxies,
            &AtomicBool::new(false),
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
    assert_eq!(
        proxy.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        download_repository_readme(
            &client,
            &Provider::Gitee,
            "http://127.0.0.1:1/README.md",
            "main",
            &proxies,
            &AtomicBool::new(true)
        )
        .await
        .unwrap_err(),
        "download.cancelled"
    );
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
            vec![
                candidate(slow, DownloadRoute::Direct),
                candidate(missing, missing_route),
            ],
            &AtomicBool::new(false),
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
        vec![
            candidate(failed, DownloadRoute::Direct),
            candidate(success, DownloadRoute::Proxy),
        ],
        &AtomicBool::new(false),
    )
    .await
    .unwrap();
    assert_eq!(content, b"# README\nBody");
    finish_server(failed_task).await;
    finish_server(success_task).await;

    let (invalid, task) = server(200, Duration::ZERO, vec![0xff]);
    assert!(
        fetch_readme(&client, &invalid, &DownloadReporter::new("README"))
            .await
            .unwrap_err()
            .contains("readme.invalidUtf8")
    );
    finish_server(task).await;
}

#[tokio::test]
async fn raw_readme_size_and_pre_cancelled_downloads_are_bounded() {
    let (large, task) = server(200, Duration::ZERO, vec![b'x'; MAX_README_BYTES + 1]);
    let client = Client::builder().no_proxy().build().unwrap();
    assert!(
        fetch_readme(&client, &large, &DownloadReporter::new("README"))
            .await
            .unwrap_err()
            .contains("readme.tooLarge")
    );
    finish_server(task).await;
    let result = download_readme(
        &client,
        vec![candidate(
            "http://127.0.0.1:1".into(),
            DownloadRoute::Direct,
        )],
        &AtomicBool::new(true),
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
