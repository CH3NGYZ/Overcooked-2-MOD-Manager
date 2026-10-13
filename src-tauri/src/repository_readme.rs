use crate::archive::DownloadReporter;
use crate::commands::{AppState, asset_proxy_urls, ordered_asset_download_candidates};
use crate::download::{DownloadCandidate, try_download_candidates_until_error};
use crate::models::{GitHubProxy, Provider, ProxyMode};
use crate::releases;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::State;

const NOT_FOUND: &str = "文件不存在";
const MAX_README_BYTES: usize = 2 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize)]
pub struct RepositoryReadme {
    pub content: String,
    pub exists: bool,
    pub url: String,
}

struct RequestState {
    cancelled: Arc<AtomicBool>,
    started: bool,
    created: Instant,
}

#[derive(Default)]
pub struct ReadmeDownloads {
    requests: Mutex<HashMap<String, RequestState>>,
}

struct ReadmeRequest<'a> {
    downloads: &'a ReadmeDownloads,
    id: String,
    cancelled: Arc<AtomicBool>,
}

impl Drop for ReadmeRequest<'_> {
    fn drop(&mut self) {
        if let Ok(mut requests) = self.downloads.requests.lock() {
            requests.remove(&self.id);
        }
    }
}

impl ReadmeDownloads {
    fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }

    fn begin(&self, id: &str) -> Result<ReadmeRequest<'_>, String> {
        if !Self::valid_id(id) {
            return Err("仓库说明请求标识无效".into());
        }
        let mut requests = self.requests.lock().map_err(|_| "操作锁异常")?;
        requests.retain(|_, request| {
            request.started || request.created.elapsed() < Duration::from_secs(60)
        });
        if requests.len() >= 32 {
            return Err("仓库说明请求过多，请稍后重试".into());
        }
        let request = requests.entry(id.into()).or_insert_with(|| RequestState {
            cancelled: Arc::new(AtomicBool::new(false)),
            started: false,
            created: Instant::now(),
        });
        if request.started {
            return Err("正在读取仓库说明".into());
        }
        request.started = true;
        Ok(ReadmeRequest {
            downloads: self,
            id: id.into(),
            cancelled: request.cancelled.clone(),
        })
    }

    fn cancel(&self, id: &str) {
        if !Self::valid_id(id) {
            return;
        }
        if let Ok(mut requests) = self.requests.lock() {
            if let Some(request) = requests.get(id) {
                request.cancelled.store(true, Ordering::Release);
            } else if requests.len() < 32 {
                // Closing the dialog may reach IPC before its download starts.
                requests.insert(
                    id.into(),
                    RequestState {
                        cancelled: Arc::new(AtomicBool::new(true)),
                        started: false,
                        created: Instant::now(),
                    },
                );
            }
        }
    }
}

async fn fetch_readme(
    client: &Client,
    url: &str,
    reporter: &DownloadReporter,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .timeout(REQUEST_TIMEOUT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache")
        .send()
        .await
        .map_err(|error| error.without_url().to_string())?;
    let status = response.status();
    log::info!(
        "仓库说明 Raw 响应：地址={}，状态={}",
        crate::repository_sources::safe_log_address(url),
        status
    );
    if status == StatusCode::NOT_FOUND {
        return Err(NOT_FOUND.into());
    }
    if !status.is_success() {
        return Err(format!("仓库说明下载失败：HTTP {status}"));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_README_BYTES as u64)
    {
        return Err("仓库说明超过 2 MiB，无法展示".into());
    }
    let mut content = Vec::new();
    let mut progress = reporter.snapshot();
    progress.total = response.content_length();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        if content.len() + chunk.len() > MAX_README_BYTES {
            return Err("仓库说明超过 2 MiB，无法展示".into());
        }
        content.extend_from_slice(&chunk);
        reporter.received(chunk.len() as u64);
        progress.downloaded = content.len() as u64;
        reporter.update(progress.clone());
    }
    std::str::from_utf8(&content).map_err(|_| "仓库说明不是有效的 UTF-8 文本")?;
    Ok(content)
}

async fn download_readme(
    client: &Client,
    candidates: Vec<DownloadCandidate>,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, String> {
    try_download_candidates_until_error(
        candidates,
        || cancelled.load(Ordering::Acquire),
        |candidate, reporter| async move { fetch_readme(client, &candidate.url, &reporter).await },
        NOT_FOUND,
    )
    .await
}

async fn download_repository_readme(
    client: &Client,
    provider: &Provider,
    url: &str,
    branch: &str,
    raw_proxies: &[GitHubProxy],
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("下载已取消".into());
    }
    if *provider == Provider::Gitee {
        // Gitee Raw uses exactly one direct GET, never proxy racing or ranged downloads.
        return fetch_readme(client, url, &DownloadReporter::new("README")).await;
    }
    let candidates = ordered_asset_download_candidates(
        ProxyMode::AutoDirect,
        url,
        asset_proxy_urls(
            raw_proxies,
            url,
            true,
            branch,
            "README.md",
            ProxyMode::AutoDirect,
            None,
        ),
    );
    log::info!("仓库说明 GitHub Raw 竞速：候选数={}", candidates.len());
    download_readme(client, candidates, cancelled).await
}

#[tauri::command]
pub async fn get_repository_readme(
    state: State<'_, AppState>,
    repository_id: String,
    request_id: String,
) -> Result<RepositoryReadme, String> {
    let request = state.readme.begin(&request_id)?;
    let cancelled = &request.cancelled;
    let config = {
        let _guard = state.operation.lock().map_err(|_| "操作锁异常")?;
        state.config.load_or_default()
    };
    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("仓库不存在")?;
    crate::community::ensure_repository_region(repo, crate::config::current_user_region())?;
    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let result = async {
        let branch = if crate::community::is_repository(repo) && !repo.branch.trim().is_empty() {
            repo.branch.clone()
        } else {
            let proxies = match repo.provider {
                Provider::Github => &config.github_proxies.github_api_proxies,
                Provider::Gitee => &config.github_proxies.gitee_api_proxies,
            };
            let token = state.config.repository_api_token(&config, repo);
            releases::fetch_repository_default_branch(&client, repo, token.as_deref(), proxies)
                .await
                .map_err(|error| {
                    log::warn!(
                        "读取仓库默认分支失败：仓库={}，原因={}",
                        repository_id,
                        error
                    );
                    "无法确定仓库默认分支".to_string()
                })?
        };
        let url = releases::repository_file_url(
            &repo.provider,
            &repo.username,
            &repo.repository,
            &branch,
            "README.md",
        )
        .map_err(|error| error.to_string())?;
        log::info!(
            "正在读取仓库说明：仓库={}，渠道={:?}，地址={}",
            repository_id,
            repo.provider,
            crate::repository_sources::safe_log_address(&url)
        );
        match download_repository_readme(
            &client,
            &repo.provider,
            &url,
            &branch,
            &config.github_proxies.raw_proxies,
            cancelled,
        )
        .await
        {
            Ok(bytes) => Ok(RepositoryReadme {
                content: String::from_utf8(bytes).map_err(|_| "仓库说明不是有效的 UTF-8 文本")?,
                exists: true,
                url,
            }),
            Err(error) if error == NOT_FOUND => Ok(RepositoryReadme {
                content: String::new(),
                exists: false,
                url,
            }),
            Err(error) => Err(error),
        }
    };
    tokio::select! {
        biased;
        _ = async {
            while !cancelled.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(50)).await; }
        } => Err("仓库说明读取已取消".into()),
        result = result => result,
    }
}

#[tauri::command]
pub fn cancel_repository_readme(state: State<'_, AppState>, request_id: String) {
    state.readme.cancel(&request_id);
}

#[cfg(test)]
mod tests {
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
            "下载已取消"
        );
    }

    #[tokio::test]
    async fn any_started_404_stops_the_real_raw_race_without_waiting_for_slow_success() {
        for missing_route in [DownloadRoute::Direct, DownloadRoute::Proxy] {
            let (slow, slow_task) =
                server(200, Duration::from_millis(500), b"# Late README".to_vec());
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
                .contains("UTF-8")
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
                .contains("2 MiB")
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
        assert_eq!(result.unwrap_err(), "下载已取消");
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
}
