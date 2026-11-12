use crate::app_state::AppState;
use crate::archive::DownloadReporter;
use crate::download::planning::{asset_proxy_urls, ordered_asset_download_candidates};
use crate::download::{DownloadCandidate, try_download_candidates_until_error};
use crate::http_diagnostics::HttpRequestExt;
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

const NOT_FOUND: &str = "readme.fileMissing";
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
            return Err("readme.invalidRequestId".into());
        }
        let mut requests = self.requests.lock().map_err(|_| "lock.poisoned")?;
        requests.retain(|_, request| {
            request.started || request.created.elapsed() < Duration::from_secs(60)
        });
        if requests.len() >= 32 {
            return Err("readme.tooManyRequests".into());
        }
        let request = requests.entry(id.into()).or_insert_with(|| RequestState {
            cancelled: Arc::new(AtomicBool::new(false)),
            started: false,
            created: Instant::now(),
        });
        if request.started {
            return Err("readme.loading".into());
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
        .send_logged()
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
        return Err(crate::locale_tag::with_params(
            "readme.httpStatus",
            &[("status", &status.to_string())],
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_README_BYTES as u64)
    {
        return Err("readme.tooLarge".into());
    }
    let mut content = Vec::new();
    let mut progress = reporter.snapshot();
    progress.total = response.content_length();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        if content.len() + chunk.len() > MAX_README_BYTES {
            return Err("readme.tooLarge".into());
        }
        content.extend_from_slice(&chunk);
        reporter.received(chunk.len() as u64);
        progress.downloaded = content.len() as u64;
        reporter.update(progress.clone());
    }
    std::str::from_utf8(&content).map_err(|_| "readme.invalidUtf8")?;
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
        return Err("download.cancelled".into());
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
        let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
        state.config.load_or_default()
    };
    /*
     * 只认 MOD 仓库。地图详情不再拉仓库 README：README 是**仓库级**的，
     * 一个仓库多张地图（一个 tag 一张图）时把它塞进每张图的详情只会串味。
     * 地图的说明来自条目自己的 `note_i18n`，长文是那个版本 Release note 的正文。
     */
    let Some(repo) = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
    else {
        return Err("source.repositoryMissing".into());
    };
    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let result = async {
        let branch = if crate::community::is_repository(repo) && !repo.branch.trim().is_empty() {
            repo.branch.clone()
        } else {
            let proxies = crate::providers::api_proxies(&config, &repo.provider);
            let token = state.config.mod_repository_api_token(&config, repo);
            releases::fetch_repository_default_branch(
                &client,
                &crate::releases::RepositoryRequest::from(repo),
                token.as_deref(),
                proxies,
            )
            .await
            .map_err(|error| {
                log::warn!("读取仓库默认分支失败：id={}，原因={}", repository_id, error);
                "readme.defaultBranchUnknown".to_string()
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
            "正在读取仓库说明：id={}，渠道={:?}，地址={}",
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
                content: String::from_utf8(bytes).map_err(|_| "readme.invalidUtf8")?,
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
        } => Err("readme.cancelled".into()),
        result = result => result,
    }
}

#[tauri::command]
pub fn cancel_repository_readme(state: State<'_, AppState>, request_id: String) {
    state.readme.cancel(&request_id);
}

#[cfg(test)]
mod tests;
