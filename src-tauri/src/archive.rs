use crate::game::safe_game_path;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use zip::ZipArchive;

static DOWNLOAD_CANCELLED: AtomicBool = AtomicBool::new(false);
const FIRST_BYTE_TIMEOUT_ERROR: &str = "下载首字节等待超过 5 秒";

pub fn cancel_download() {
    log::info!("收到下载取消请求");
    DOWNLOAD_CANCELLED.store(true, Ordering::Release);
}

pub fn is_download_cancelled() -> bool {
    DOWNLOAD_CANCELLED.load(Ordering::Acquire)
}

pub fn start_download() {
    log::debug!("开始新的下载任务，清除取消状态");
    DOWNLOAD_CANCELLED.store(false, Ordering::Release);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub label: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes: u64,
    pub finished: bool,
    pub phase: DownloadPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<DownloadRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_sources: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    Connecting,
    Switching,
    Downloading,
    Finished,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadRoute {
    Direct,
    Proxy,
}

impl DownloadProgress {
    pub fn connecting(label: &str) -> Self {
        Self {
            label: label.into(),
            downloaded: 0,
            total: None,
            speed_bytes: 0,
            finished: false,
            phase: DownloadPhase::Connecting,
            route: None,
            source: None,
            attempt: None,
            attempt_total: None,
            parallel_sources: None,
        }
    }
}

// Every source reports into its own snapshot. The race coordinator publishes
// one source at a time so duplicate downloads never inflate the progress bar.
pub struct DownloadReporter {
    progress: Mutex<DownloadProgress>,
    transferred: AtomicU64,
}

impl DownloadReporter {
    pub fn new(label: &str) -> Self {
        Self {
            progress: Mutex::new(DownloadProgress::connecting(label)),
            transferred: AtomicU64::new(0),
        }
    }

    pub fn update(&self, progress: DownloadProgress) {
        if let Ok(mut current) = self.progress.lock() {
            *current = progress;
        }
    }

    pub fn snapshot(&self) -> DownloadProgress {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn received(&self, bytes: u64) {
        self.transferred.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn transferred(&self) -> u64 {
        self.transferred.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallManifest {
    pub repository_id: String,
    pub release_tag: String,
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub installed_at: String,
    pub files: Vec<String>,
}

// Dropping an in-flight download (including HEAD, GET and range waits) must
// close its files and remove incomplete data before switching candidates.
struct TemporaryDownload {
    path: PathBuf,
    keep: bool,
}

impl Drop for TemporaryDownload {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub async fn download_to_temp_with_threads_and_first_byte_timeout(
    client: &Client,
    url: &str,
    temp_dir: &Path,
    label: &str,
    reporter: &DownloadReporter,
    threads: u32,
    first_byte_timeout: Option<std::time::Duration>,
) -> Result<PathBuf, String> {
    // Update this source before HEAD/GET or first-byte waits. Only the race
    // coordinator publishes progress to the application window.
    reporter.update(DownloadProgress::connecting(label));
    log::info!(
        "开始下载到临时文件：任务={}，线程数={}，首字节超时={:?}",
        label,
        threads,
        first_byte_timeout
    );
    fs::create_dir_all(temp_dir).map_err(|e| {
        log::error!("创建下载临时目录失败：任务={}，原因={}", label, e);
        e.to_string()
    })?;
    let target = temp_dir.join(format!("{}.download", Uuid::new_v4()));
    let mut temporary = TemporaryDownload {
        path: target.clone(),
        keep: false,
    };
    log::trace!(
        "下载临时文件路径已分配：任务={}，路径={}",
        label,
        target.display()
    );
    if threads > 1 {
        log::info!("下载策略：尝试分片，线程数={}，任务={}", threads, label);
        let head = client
            .head(url)
            .header("User-Agent", crate::releases::APP_USER_AGENT)
            .header(reqwest::header::ACCEPT_ENCODING, "identity");
        let head_response = if let Some(timeout) = first_byte_timeout {
            match tokio::time::timeout(timeout, head.send()).await {
                Ok(Ok(response)) => Some(response),
                Ok(Err(error)) => {
                    log::warn!(
                        "分片 HEAD 探测失败，回退检查：任务={}，超时={}，连接错误={}",
                        label,
                        error.is_timeout(),
                        error.is_connect()
                    );
                    None
                }
                Err(_) => {
                    log::warn!("分片 HEAD 探测超时：任务={}", label);
                    return Err(FIRST_BYTE_TIMEOUT_ERROR.into());
                }
            }
        } else {
            match head.send().await {
                Ok(response) => Some(response),
                Err(error) => {
                    log::warn!(
                        "分片 HEAD 探测失败，回退检查：任务={}，超时={}，连接错误={}",
                        label,
                        error.is_timeout(),
                        error.is_connect()
                    );
                    None
                }
            }
        };
        let total = head_response
            .as_ref()
            .and_then(|response| {
                if response.status().is_success() {
                    response.content_length()
                } else {
                    log::warn!(
                        "分片 HEAD 探测返回非成功状态，回退检查：任务={}，状态码={}",
                        label,
                        response.status().as_u16()
                    );
                    None
                }
            })
            .filter(|total| *total > 1024 * 1024);
        if let Some(total) = total {
            log::info!("分片下载文件大小：{} 字节，任务={}", total, label);
            let range_request = client
                .get(url)
                .header("User-Agent", crate::releases::APP_USER_AGENT)
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .header(reqwest::header::RANGE, "bytes=0-0");
            let range_response = if let Some(timeout) = first_byte_timeout {
                match tokio::time::timeout(timeout, range_request.send()).await {
                    Ok(Ok(response)) => Some(response),
                    Ok(Err(error)) => {
                        log::warn!(
                            "Range 能力探测请求失败，回退单线程：任务={}，超时={}，连接错误={}",
                            label,
                            error.is_timeout(),
                            error.is_connect()
                        );
                        None
                    }
                    Err(_) => {
                        log::warn!("Range 能力探测超时：任务={}", label);
                        return Err(FIRST_BYTE_TIMEOUT_ERROR.into());
                    }
                }
            } else {
                match range_request.send().await {
                    Ok(response) => Some(response),
                    Err(error) => {
                        log::warn!(
                            "Range 能力探测请求失败，回退单线程：任务={}，超时={}，连接错误={}",
                            label,
                            error.is_timeout(),
                            error.is_connect()
                        );
                        None
                    }
                }
            };
            let range_supported = range_response.is_some_and(|response| {
                if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                    true
                } else {
                    log::warn!(
                        "服务器不支持 Range，回退单线程：任务={}，状态码={}",
                        label,
                        response.status().as_u16()
                    );
                    false
                }
            });
            if range_supported {
                log::info!("服务器支持 Range，开始分片下载，任务={}", label);
                match download_ranges(
                    client,
                    url,
                    &target,
                    label,
                    reporter,
                    total,
                    threads,
                    first_byte_timeout,
                )
                .await
                {
                    Ok(()) => {
                        log::info!("分片下载完成：任务={}，路径={}", label, target.display());
                        temporary.keep = true;
                        return Ok(target);
                    }
                    Err(error) if DOWNLOAD_CANCELLED.load(Ordering::Acquire) => {
                        let _ = fs::remove_file(&target);
                        return Err(error);
                    }
                    Err(error) if error == FIRST_BYTE_TIMEOUT_ERROR => {
                        let _ = fs::remove_file(&target);
                        return Err(error);
                    }
                    Err(error) => {
                        log::warn!("分片下载失败，回退单线程：任务={}，原因={}", label, error);
                        let _ = fs::remove_file(&target);
                    }
                }
            }
        } else {
            log::warn!("无法获取可用于分片的文件长度，回退单线程：任务={}", label);
        }
    } else {
        log::info!("下载策略：单线程直连流式下载，任务={}", label);
    }
    download_stream(client, url, &target, label, reporter, first_byte_timeout).await?;
    log::info!("单线程下载完成：任务={}，路径={}", label, target.display());
    temporary.keep = true;
    Ok(target)
}

async fn download_stream(
    client: &Client,
    url: &str,
    target: &Path,
    label: &str,
    reporter: &DownloadReporter,
    first_byte_timeout: Option<std::time::Duration>,
) -> Result<(), String> {
    if is_download_cancelled() {
        return Err("下载已取消".into());
    }
    reporter.update(DownloadProgress::connecting(label));
    let request = client
        .get(url)
        .header("User-Agent", crate::releases::APP_USER_AGENT)
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    let response = if let Some(timeout) = first_byte_timeout {
        tokio::time::timeout(timeout, request.send())
            .await
            .map_err(|_| FIRST_BYTE_TIMEOUT_ERROR.to_string())?
    } else {
        request.send().await
    }
    .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("下载返回 HTTP {}", response.status()));
    }
    let total = response.content_length();
    log::info!(
        "下载响应：任务={}，HTTP={}，长度={:?}",
        label,
        response.status(),
        total
    );
    let mut file = File::create(target).map_err(|error| error.to_string())?;
    let mut body = response.bytes_stream();
    let started = std::time::Instant::now();
    let mut downloaded = 0_u64;
    let first_chunk = if let Some(timeout) = first_byte_timeout {
        tokio::time::timeout(timeout, body.next())
            .await
            .map_err(|_| FIRST_BYTE_TIMEOUT_ERROR.to_string())?
    } else {
        body.next().await
    };
    let mut next_chunk = first_chunk;
    while let Some(chunk) = next_chunk {
        if is_download_cancelled() {
            return Err("下载已取消".into());
        }
        let chunk = chunk.map_err(|error| error.to_string())?;
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        reporter.received(chunk.len() as u64);
        downloaded += chunk.len() as u64;
        emit_progress(reporter, label, downloaded, total, started.elapsed(), false);
        next_chunk = body.next().await;
    }
    validate_download_size(downloaded, total)?;
    file.flush().map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    let actual = fs::metadata(target)
        .map_err(|error| error.to_string())?
        .len();
    if actual != downloaded {
        return Err(format!(
            "下载文件落盘长度不完整: 文件为 {actual} 字节，实际收到 {downloaded} 字节"
        ));
    }
    emit_progress(reporter, label, downloaded, total, started.elapsed(), true);
    Ok(())
}

async fn read_response_bytes(
    response: reqwest::Response,
    expected: u64,
    reporter: &DownloadReporter,
    first_byte_timeout: Option<std::time::Duration>,
) -> Result<Vec<u8>, String> {
    let mut body = response.bytes_stream();
    let mut bytes = Vec::with_capacity(expected.min(16 * 1024 * 1024) as usize);
    let first_chunk = if let Some(timeout) = first_byte_timeout {
        match tokio::time::timeout(timeout, body.next()).await {
            Ok(chunk) => chunk,
            Err(_) => return Err(FIRST_BYTE_TIMEOUT_ERROR.into()),
        }
    } else {
        body.next().await
    };
    let mut next_chunk = first_chunk;
    while let Some(chunk) = next_chunk {
        let chunk = chunk.map_err(|error| error.to_string())?;
        reporter.received(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > expected {
            return Err(format!("分段长度超出预期: 收到 {} 字节", bytes.len()));
        }
        next_chunk = body.next().await;
    }
    if bytes.len() as u64 != expected {
        return Err(format!("分段长度错误: 收到 {} 字节", bytes.len()));
    }
    Ok(bytes)
}

async fn download_ranges(
    client: &Client,
    url: &str,
    target: &Path,
    label: &str,
    reporter: &DownloadReporter,
    total: u64,
    threads: u32,
    first_byte_timeout: Option<std::time::Duration>,
) -> Result<(), String> {
    const PART_SIZE: u64 = 4 * 1024 * 1024;
    let ranges = (0..total.div_ceil(PART_SIZE))
        .map(|part| {
            let start = part * PART_SIZE;
            (start, (start + PART_SIZE - 1).min(total - 1))
        })
        .collect::<Vec<_>>();
    let mut output = File::create(target).map_err(|error| {
        log::error!("创建分片下载文件失败：任务={}，原因={}", label, error);
        error.to_string()
    })?;
    output.set_len(total).map_err(|error| {
        log::error!(
            "预分配分片下载文件失败：任务={}，字节数={}，原因={}",
            label,
            total,
            error
        );
        error.to_string()
    })?;
    let started = std::time::Instant::now();
    let mut downloaded = 0_u64;
    let mut pending = stream::iter(ranges.into_iter().map(|(start, end)| {
        let client = client.clone();
        let url = url.to_string();
        async move {
            let mut last_error = String::new();
            let mut last_error_kind = "unknown";
            for attempt in 1..=3 {
                if DOWNLOAD_CANCELLED.load(Ordering::Acquire) {
                    return Err("下载已取消".to_string());
                }
                let request = client
                    .get(&url)
                    .header("User-Agent", crate::releases::APP_USER_AGENT)
                    .header(reqwest::header::ACCEPT_ENCODING, "identity")
                    .header(reqwest::header::RANGE, format!("bytes={start}-{end}"));
                let response = if let Some(timeout) = first_byte_timeout {
                    match tokio::time::timeout(timeout, request.send()).await {
                        Ok(result) => result,
                        Err(_) => {
                            log::warn!("分片首字节超时：范围={}-{}，尝试={}", start, end, attempt);
                            return Err(FIRST_BYTE_TIMEOUT_ERROR.to_string());
                        }
                    }
                } else {
                    request.send().await
                };
                match response {
                    Ok(response) if response.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                        let expected_range = format!("bytes {start}-{end}/{total}");
                        let actual_range = response
                            .headers()
                            .get(reqwest::header::CONTENT_RANGE)
                            .and_then(|value| value.to_str().ok());
                        if actual_range != Some(expected_range.as_str()) {
                            last_error = "服务器返回了错误的分段范围".into();
                            last_error_kind = "invalid_content_range";
                            continue;
                        }
                        match read_response_bytes(
                            response,
                            end - start + 1,
                            reporter,
                            first_byte_timeout,
                        )
                        .await
                        {
                            Ok(bytes) => return Ok((start, bytes)),
                            Err(error) => {
                                if error == FIRST_BYTE_TIMEOUT_ERROR {
                                    log::warn!(
                                        "分片响应首字节超时：范围={}-{}，尝试={}",
                                        start,
                                        end,
                                        attempt
                                    );
                                    return Err(error);
                                }
                                last_error = error.to_string();
                                last_error_kind = "response_body";
                            }
                        }
                    }
                    Ok(response) => {
                        let status = response.status().as_u16();
                        last_error = format!("服务器不支持分段下载 (HTTP {status})");
                        last_error_kind = "unexpected_http_status";
                    }
                    Err(error) => {
                        last_error_kind = if error.is_timeout() {
                            "request_timeout"
                        } else if error.is_connect() {
                            "connection_error"
                        } else {
                            "request_error"
                        };
                        last_error = error.to_string();
                    }
                }
            }
            log::warn!(
                "分片重试耗尽：范围={}-{}，尝试次数=3，错误类别={}",
                start,
                end,
                last_error_kind
            );
            Err(last_error)
        }
    }))
    .buffer_unordered(threads.clamp(1, 32) as usize);
    while let Some(result) = pending.next().await {
        if DOWNLOAD_CANCELLED.load(Ordering::Acquire) {
            return Err("下载已取消".into());
        }
        let (offset, bytes) = result?;
        output.seek(SeekFrom::Start(offset)).map_err(|error| {
            log::error!(
                "定位分片写入位置失败：任务={}，偏移={}，原因={}",
                label,
                offset,
                error
            );
            error.to_string()
        })?;
        output.write_all(&bytes).map_err(|error| {
            log::error!(
                "写入下载分片失败：任务={}，偏移={}，字节数={}，原因={}",
                label,
                offset,
                bytes.len(),
                error
            );
            error.to_string()
        })?;
        downloaded += bytes.len() as u64;
        emit_progress(
            reporter,
            label,
            downloaded,
            Some(total),
            started.elapsed(),
            false,
        );
    }
    output.flush().map_err(|error| {
        log::error!("刷新分片下载文件失败：任务={}，原因={}", label, error);
        error.to_string()
    })?;
    output.sync_all().map_err(|error| {
        log::error!("同步分片下载文件失败：任务={}，原因={}", label, error);
        error.to_string()
    })?;
    drop(output);
    let actual_size = fs::metadata(target)
        .map_err(|error| {
            log::error!("读取分片下载文件属性失败：任务={}，原因={}", label, error);
            error.to_string()
        })?
        .len();
    if actual_size != total {
        return Err(format!(
            "分片下载落盘长度不完整: 文件为 {actual_size} 字节，预期 {total} 字节"
        ));
    }
    emit_progress(reporter, label, total, Some(total), started.elapsed(), true);
    Ok(())
}

pub fn emit_progress(
    reporter: &DownloadReporter,
    label: &str,
    downloaded: u64,
    total: Option<u64>,
    elapsed: std::time::Duration,
    finished: bool,
) {
    let percent = total
        .filter(|value| *value > 0)
        .map(|value| downloaded as f64 / value as f64 * 100.0);
    let speed_bytes = downloaded / elapsed.as_secs().max(1);
    log::debug!(
        "下载进度：任务={}，已下载={}，总长度={:?}，百分比={:?}%，速度={}B/s，完成={}",
        label,
        downloaded,
        total,
        percent,
        speed_bytes,
        finished
    );
    reporter.update(DownloadProgress {
        label: label.into(),
        downloaded,
        total,
        speed_bytes,
        finished,
        phase: if finished {
            DownloadPhase::Finished
        } else {
            DownloadPhase::Downloading
        },
        // The coordinator adds the selected source's connection context.
        route: None,
        source: None,
        attempt: None,
        attempt_total: None,
        parallel_sources: None,
    });
}

pub fn emit_progress_event(app: Option<&AppHandle>, progress: DownloadProgress) {
    if let Some(app) = app {
        let _ = app.emit("download-progress", progress);
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    log::debug!("开始计算 SHA-256：文件={}", path.display());
    let mut file = File::open(path).map_err(|e| {
        log::error!("打开待校验文件失败：文件={}，原因={}", path.display(), e);
        e.to_string()
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = format!("{:x}", hasher.finalize());
    log::trace!("SHA-256 计算完成：文件={}，摘要={}", path.display(), digest);
    Ok(digest)
}

pub fn verify_digest(path: &Path, digest: Option<&str>) -> Result<(), String> {
    if let Some(expected) = digest {
        log::debug!("开始校验文件摘要：文件={}", path.display());
        let expected = expected
            .strip_prefix("sha256:")
            .unwrap_or(expected)
            .trim()
            .to_ascii_lowercase();
        let actual = sha256_file(path)?;
        if expected != actual {
            log::error!("SHA-256 校验失败：文件={}", path.display());
            return Err(format!("SHA-256 校验失败，期望 {expected}，实际 {actual}"));
        }
        log::info!("文件摘要校验通过：文件={}", path.display());
    } else {
        log::trace!("未提供文件摘要，跳过校验：文件={}", path.display());
    }
    Ok(())
}

pub fn validate_download_file(
    path: &Path,
    expected_size: Option<u64>,
    digest: Option<&str>,
) -> Result<(), String> {
    validate_download_size(
        fs::metadata(path).map_err(|error| error.to_string())?.len(),
        expected_size,
    )?;
    verify_digest(path, digest)
}

fn validate_download_size(actual: u64, expected: Option<u64>) -> Result<(), String> {
    if actual == 0 {
        return Err("下载文件为空".into());
    }
    if expected.is_some_and(|size| size != actual) {
        log::warn!(
            "下载长度不完整: 收到 {actual} 字节，预期 {} 字节",
            expected.unwrap()
        );
        return Err("下载长度不完整".into());
    }
    Ok(())
}

pub fn validate_download_bytes(
    bytes: &[u8],
    expected_size: Option<u64>,
    digest: Option<&str>,
) -> Result<(), String> {
    validate_download_size(bytes.len() as u64, expected_size)?;
    if let Some(expected) = digest {
        let expected = expected
            .strip_prefix("sha256:")
            .unwrap_or(expected)
            .trim()
            .to_ascii_lowercase();
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != expected {
            return Err("SHA-256 校验失败".into());
        }
    }
    Ok(())
}

pub fn extract_zip_safe(zip_path: &Path, game_root: &Path) -> Result<Vec<String>, String> {
    log::info!("开始安全解压 ZIP：文件={}", zip_path.display());
    let result = extract_zip_into_with_root(zip_path, game_root, "", None);
    if let Err(error) = &result {
        log::error!(
            "安全解压 ZIP 失败：文件={}，原因={}",
            zip_path.display(),
            error
        );
    }
    result
}

/// Read every ZIP entry before changing the game directory. `zip` validates
/// each entry's compressed stream and CRC while it is read, so this catches a
/// truncated or otherwise damaged download before installation starts.
pub fn validate_zip(zip_path: &Path) -> Result<(), String> {
    log::debug!("开始校验 ZIP：文件={}", zip_path.display());
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
        if !entry.is_dir() {
            io::copy(&mut entry, &mut io::sink()).map_err(|e| format!("ZIP 内容校验失败: {e}"))?;
        }
    }
    log::info!(
        "ZIP 内容校验通过：文件={}，条目数={}",
        zip_path.display(),
        archive.len()
    );
    Ok(())
}

#[cfg(test)]
fn extract_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    log::info!(
        "开始解压 MOD ZIP：文件={}，目标={}",
        zip_path.display(),
        target_dir
    );
    let plan = prepare_repository_mod_zip(zip_path, game_root, target_dir, archive_name)?;
    extract_prepared_mod_zip(zip_path, game_root, &plan).map(PendingModInstall::commit)
}

#[cfg(test)]
fn extract_local_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    extract_local_mod_zip_pending(zip_path, game_root, archive_name).map(PendingModInstall::commit)
}

pub(crate) fn extract_local_mod_zip_pending(
    zip_path: &Path,
    game_root: &Path,
    archive_name: &str,
) -> Result<PendingModInstall, String> {
    log::info!(
        "开始解压本地 MOD ZIP：文件={}，归档名={}",
        zip_path.display(),
        archive_name
    );
    let plan = local_mod_zip_plan(zip_path, archive_name)?;
    extract_prepared_mod_zip(zip_path, game_root, &plan)
}

pub(crate) struct ModZipInstallPlan {
    target_dir: String,
    strip_root: Option<String>,
}

pub(crate) fn prepare_repository_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    install_target: &str,
    archive_name: &str,
) -> Result<ModZipInstallPlan, String> {
    let plan = repository_mod_zip_plan(zip_path, install_target, archive_name)?;
    zip_entry_destinations(
        zip_path,
        game_root,
        &plan.target_dir,
        plan.strip_root.as_deref(),
    )?;
    Ok(plan)
}

pub(crate) fn extract_prepared_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    plan: &ModZipInstallPlan,
) -> Result<PendingModInstall, String> {
    extract_zip_pending(
        zip_path,
        game_root,
        &plan.target_dir,
        plan.strip_root.as_deref(),
    )
}

pub(crate) fn prepared_mod_zip_files(
    zip_path: &Path,
    game_root: &Path,
    plan: &ModZipInstallPlan,
) -> Result<Vec<String>, String> {
    zip_entry_destinations(
        zip_path,
        game_root,
        &plan.target_dir,
        plan.strip_root.as_deref(),
    )
    .map(|destinations| {
        destinations
            .into_iter()
            .map(|(_, path)| path_relative(game_root, &path))
            .collect()
    })
}

fn repository_mod_zip_plan(
    zip_path: &Path,
    install_target: &str,
    archive_name: &str,
) -> Result<ModZipInstallPlan, String> {
    let layout = inspect_mod_zip(zip_path)?;
    let strip_root = match layout {
        ModZipLayout::TopLevelBepInEx | ModZipLayout::TopLevelPlugins(_) => {
            online_zip_strip_root(zip_path, install_target)?
        }
        _ => None,
    };
    let plan = ModZipInstallPlan {
        target_dir: if layout == ModZipLayout::Wrapped {
            wrapped_zip_target(install_target, archive_name)?
        } else {
            install_target.to_string()
        },
        strip_root,
    };
    log::debug!(
        "生成在线 MOD ZIP 安装计划：目标={}，剥离根目录={:?}",
        plan.target_dir,
        plan.strip_root
    );
    Ok(plan)
}

fn local_mod_zip_plan(zip_path: &Path, archive_name: &str) -> Result<ModZipInstallPlan, String> {
    let layout = inspect_mod_zip(zip_path)?;
    log::debug!(
        "识别本地 MOD ZIP 布局：文件={}，布局={:?}",
        zip_path.display(),
        layout
    );
    match layout {
        ModZipLayout::TopLevelBepInEx => Ok(ModZipInstallPlan {
            target_dir: "BepInEx".into(),
            strip_root: Some("BepInEx".into()),
        }),
        ModZipLayout::TopLevelPlugins(root) => Ok(ModZipInstallPlan {
            target_dir: "BepInEx/plugins".into(),
            strip_root: Some(root),
        }),
        ModZipLayout::TopLevelDlls | ModZipLayout::SingleDirectory => Ok(ModZipInstallPlan {
            target_dir: "BepInEx/plugins".into(),
            strip_root: None,
        }),
        ModZipLayout::Wrapped => Ok(ModZipInstallPlan {
            target_dir: wrapped_zip_target("BepInEx/plugins", archive_name)?,
            strip_root: None,
        }),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ModZipLayout {
    TopLevelBepInEx,
    TopLevelPlugins(String),
    TopLevelDlls,
    SingleDirectory,
    Wrapped,
}

fn safe_zip_stem(archive_name: &str) -> Result<&str, String> {
    if archive_name
        .chars()
        .any(|character| matches!(character, '/' | '\\' | ':'))
    {
        return Err("ZIP 文件名无效".into());
    }
    let stem = Path::new(archive_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("ZIP 文件名无效")?;
    if stem.is_empty() || stem == "." || stem == ".." {
        return Err("ZIP 文件名无效".into());
    }
    Ok(stem)
}

fn wrapped_zip_target(install_target: &str, archive_name: &str) -> Result<String, String> {
    let stem = safe_zip_stem(archive_name)?;
    let target = Path::new(install_target);
    let is_package_directory = !stem.eq_ignore_ascii_case("BepInEx")
        && !stem.eq_ignore_ascii_case("plugins")
        && target
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(stem));
    Ok(if is_package_directory {
        target.to_string_lossy().into_owned()
    } else {
        target.join(stem).to_string_lossy().into_owned()
    })
}

fn inspect_mod_zip(zip_path: &Path) -> Result<ModZipLayout, String> {
    log::trace!("检查 MOD ZIP 顶层布局：文件={}", zip_path.display());
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    let mut top_level = HashSet::new();
    let mut directory_root = None;
    let mut only_root_dlls = !archive.is_empty();

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| format!("拒绝不安全 ZIP 路径: {}", entry.name()))?;
        let components = path.components().collect::<Vec<_>>();
        let Some(root) = components.first() else {
            only_root_dlls = false;
            continue;
        };
        let root_name = root.as_os_str().to_string_lossy().into_owned();
        top_level.insert(root_name.to_ascii_lowercase());
        if components.len() > 1 || entry.is_dir() {
            only_root_dlls = false;
            directory_root = Some(root_name);
        } else if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            only_root_dlls = false;
        }
    }

    if top_level.len() == 1 {
        if let Some(root) = directory_root {
            return Ok(if root.eq_ignore_ascii_case("BepInEx") {
                ModZipLayout::TopLevelBepInEx
            } else if root.eq_ignore_ascii_case("plugins") {
                ModZipLayout::TopLevelPlugins(root)
            } else {
                ModZipLayout::SingleDirectory
            });
        }
    }
    if only_root_dlls {
        return Ok(ModZipLayout::TopLevelDlls);
    }
    Ok(ModZipLayout::Wrapped)
}

fn online_zip_strip_root(zip_path: &Path, install_target: &str) -> Result<Option<String>, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    let mut file_paths = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| format!("拒绝不安全 ZIP 路径: {}", entry.name()))?;
        if !entry.is_dir() {
            file_paths.push(path.to_path_buf());
        }
    }
    if file_paths.is_empty() {
        return Ok(None);
    }
    let target_components = Path::new(install_target).components().collect::<Vec<_>>();
    // Only remove directories already represented by the configured target.
    // A lone ordinary MOD folder is payload, not an archive wrapper.
    for start in 0..target_components.len() {
        let expected = &target_components[start..];
        let target_matches_all = file_paths.iter().all(|path| {
            let components = path.components().collect::<Vec<_>>();
            components.len() > expected.len()
                && components
                    .iter()
                    .zip(expected.iter())
                    .all(|(actual, expected)| {
                        actual
                            .as_os_str()
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
                    })
        });
        if target_matches_all {
            let strip_root = expected.iter().collect::<PathBuf>();
            return Ok(Some(strip_root.to_string_lossy().into_owned()));
        }
    }
    if let Some(expected_root) = target_components.first() {
        let expected_root = expected_root.as_os_str().to_string_lossy();
        if !expected_root.is_empty()
            && file_paths.iter().all(|path| {
                path.components().next().is_some_and(|actual| {
                    actual
                        .as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&expected_root)
                })
            })
        {
            return Ok(Some(expected_root.into_owned()));
        }
    }
    Ok(None)
}

fn snapshot_path(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        log::trace!("无需创建文件快照，目标不存在：{}", path.display());
        return Ok(None);
    }
    let snapshot = std::env::temp_dir().join(format!("oc2-mod-snapshot-{}", Uuid::new_v4()));
    log::debug!(
        "创建文件快照：源={}，快照={}",
        path.display(),
        snapshot.display()
    );
    copy_path(path, &snapshot)?;
    Ok(Some(snapshot))
}

fn restore_snapshot(snapshot: Option<PathBuf>, target: &Path) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    remove_existing_path(target)?;
    copy_path(&snapshot, target)?;
    log::warn!("已恢复文件快照：目标={}", target.display());
    remove_existing_path(&snapshot)
}

fn discard_snapshot(snapshot: Option<PathBuf>) {
    if let Some(snapshot) = snapshot {
        if let Err(error) = remove_existing_path(&snapshot) {
            log::warn!(
                "清理 MOD 临时快照失败：路径={}，原因={}",
                snapshot.display(),
                error
            );
        }
    }
}

fn remove_existing_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    clear_readonly(path)?;
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| {
            log::error!("删除目录失败：路径={}，原因={}", path.display(), e);
            e.to_string()
        })
    } else {
        fs::remove_file(path).map_err(|e| {
            log::error!("删除文件失败：路径={}，原因={}", path.display(), e);
            e.to_string()
        })
    }
}

fn copy_path(source: &Path, destination: &Path) -> Result<(), String> {
    if source.is_dir() {
        fs::create_dir_all(destination).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn extract_zip_into_with_root(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    strip_root: Option<&str>,
) -> Result<Vec<String>, String> {
    extract_zip_pending(zip_path, game_root, target_dir, strip_root).map(PendingModInstall::commit)
}

/// Keep overwritten files until the installation manifest has been committed.
/// Dropping an unfinished installation restores them and removes new files.
pub(crate) struct PendingModInstall {
    game_root: PathBuf,
    files: Vec<String>,
    snapshots: Vec<(PathBuf, Option<PathBuf>)>,
}

impl PendingModInstall {
    fn new(game_root: &Path) -> Self {
        Self {
            game_root: game_root.to_path_buf(),
            files: Vec::new(),
            snapshots: Vec::new(),
        }
    }

    fn track_destination(&mut self, destination: &Path) -> Result<(), String> {
        if destination.is_dir() {
            return Err(format!("目标文件已存在: {}", destination.display()));
        }
        let snapshot = snapshot_path(destination)?;
        self.snapshots.push((destination.to_path_buf(), snapshot));
        Ok(())
    }

    pub(crate) fn files(&self) -> &[String] {
        &self.files
    }

    pub(crate) fn commit(mut self) -> Vec<String> {
        for (_, snapshot) in self.snapshots.drain(..) {
            discard_snapshot(snapshot);
        }
        std::mem::take(&mut self.files)
    }

    pub(crate) fn rollback(&mut self) -> Result<(), String> {
        let mut failed = false;
        for (destination, snapshot) in self.snapshots.drain(..).rev() {
            let result = if snapshot.is_some() {
                restore_snapshot(snapshot, &destination)
            } else {
                remove_existing_path(&destination).and_then(|_| {
                    destination
                        .parent()
                        .filter(|parent| parent.is_dir())
                        .map_or(Ok(()), |parent| {
                            prune_empty_parent_dirs(&self.game_root, parent)
                        })
                })
            };
            if let Err(error) = result {
                failed = true;
                log::error!(
                    "MOD 安装回滚失败：路径={}，原因={}",
                    destination.display(),
                    error
                );
            }
        }
        if failed {
            Err("操作失败，部分文件保留在回收站，请查看日志".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for PendingModInstall {
    fn drop(&mut self) {
        let _ = self.rollback();
    }
}

fn zip_entry_destinations(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    strip_root: Option<&str>,
) -> Result<Vec<(usize, PathBuf)>, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    let mut destinations = Vec::new();
    let mut unique = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("拒绝不安全 ZIP 路径: {}", entry.name()))?;
        if entry.is_dir() {
            continue;
        }
        let relative_entry = if let Some(root) = strip_root {
            strip_zip_root(&enclosed, root)
                .ok_or_else(|| format!("ZIP 顶层目录不是预期的 {root}: {}", entry.name()))?
        } else {
            enclosed.to_path_buf()
        };
        let relative = Path::new(target_dir).join(relative_entry);
        let destination = safe_game_path(game_root, &relative.to_string_lossy())?;
        if !unique.insert(destination.to_string_lossy().to_lowercase()) {
            return Err(format!("目标文件已存在: {}", destination.display()));
        }
        destinations.push((index, destination));
    }
    if destinations.is_empty() {
        return Err("所选文件没有可安装内容".into());
    }
    Ok(destinations)
}

fn extract_zip_pending(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    strip_root: Option<&str>,
) -> Result<PendingModInstall, String> {
    let destinations = zip_entry_destinations(zip_path, game_root, target_dir, strip_root)?;
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    let mut pending = PendingModInstall::new(game_root);
    let extraction = (|| -> Result<(), String> {
        for (index, destination) in destinations {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
            pending.track_destination(&destination)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            clear_readonly(&destination)?;
            let mut output = File::create(&destination).map_err(|e| e.to_string())?;
            io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
            pending.files.push(path_relative(game_root, &destination));
        }
        Ok(())
    })();
    if let Err(error) = extraction {
        pending.rollback()?;
        return Err(error);
    }
    Ok(pending)
}

fn strip_zip_root(path: &Path, expected_root: &str) -> Option<PathBuf> {
    let path_components = path.components().collect::<Vec<_>>();
    let expected_components = Path::new(expected_root).components().collect::<Vec<_>>();
    if expected_components.is_empty() || path_components.is_empty() {
        return None;
    }
    let full_match = path_components.len() >= expected_components.len()
        && path_components
            .iter()
            .zip(expected_components.iter())
            .all(|(actual, expected)| {
                actual
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
            });
    let strip_count = if full_match {
        expected_components.len()
    } else if path_components[0]
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected_components[0].as_os_str().to_string_lossy())
    {
        1
    } else {
        return None;
    };
    Some(path_components.into_iter().skip(strip_count).collect())
}

#[cfg(test)]
fn copy_asset(
    asset_path: &Path,
    game_root: &Path,
    install_target: &str,
    file_name: &str,
) -> Result<Vec<String>, String> {
    copy_asset_pending(asset_path, game_root, install_target, file_name)
        .map(PendingModInstall::commit)
}

pub(crate) fn copy_asset_pending(
    asset_path: &Path,
    game_root: &Path,
    install_target: &str,
    file_name: &str,
) -> Result<PendingModInstall, String> {
    log::info!(
        "复制 MOD 资产：文件={}，目标目录={}，文件名={}",
        asset_path.display(),
        install_target,
        file_name
    );
    let relative = Path::new(install_target).join(file_name);
    let destination = safe_game_path(game_root, &relative.to_string_lossy())?;
    let mut pending = PendingModInstall::new(game_root);
    pending.track_destination(&destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    clear_readonly(&destination)?;
    if let Err(error) = fs::copy(asset_path, &destination) {
        let restore_error = pending.rollback().err();
        let result = match restore_error {
            Some(restore_error) => {
                format!("复制 MOD 文件失败: {error}；恢复旧文件失败: {restore_error}")
            }
            None => error.to_string(),
        };
        log::error!(
            "复制 MOD 资产失败：文件={}，原因={}",
            asset_path.display(),
            result
        );
        return Err(result);
    }
    let relative = path_relative(game_root, &destination);
    log::info!("MOD 资产复制完成：目标={}", relative);
    pending.files.push(relative);
    Ok(pending)
}

pub fn write_manifest(manifest_dir: &Path, manifest: &InstallManifest) -> Result<(), String> {
    log::debug!(
        "写入 MOD 安装清单：仓库={}，文件数={}",
        manifest.repository_id,
        manifest.files.len()
    );
    fs::create_dir_all(manifest_dir).map_err(|e| e.to_string())?;
    let path = manifest_dir.join(format!("{}.toml", manifest.repository_id));
    let text = toml::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let temp = manifest_dir.join(format!("{}.{}.tmp", manifest.repository_id, Uuid::new_v4()));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    let result = fs::rename(temp, path).map_err(|e| e.to_string());
    match &result {
        Ok(()) => log::info!("MOD 安装清单写入完成：仓库={}", manifest.repository_id),
        Err(error) => log::error!(
            "MOD 安装清单写入失败：仓库={}，原因={}",
            manifest.repository_id,
            error
        ),
    }
    result
}

pub fn read_manifest(manifest_dir: &Path, repository_id: &str) -> Option<InstallManifest> {
    let path = manifest_dir.join(format!("{}.toml", repository_id));
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            log::trace!(
                "读取 MOD 安装清单失败或不存在：仓库={}，原因={}",
                repository_id,
                error
            );
            return None;
        }
    };
    match toml::from_str(&text) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            log::warn!(
                "解析 MOD 安装清单失败：仓库={}，原因={}",
                repository_id,
                error
            );
            None
        }
    }
}

pub fn read_manifests(manifest_dir: &Path) -> Vec<InstallManifest> {
    let manifests = fs::read_dir(manifest_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        })
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| toml::from_str::<InstallManifest>(&text).ok())
        .collect::<Vec<_>>();
    log::debug!(
        "读取 MOD 安装清单完成：目录={}，数量={}",
        manifest_dir.display(),
        manifests.len()
    );
    manifests
}

pub fn prune_empty_parent_dirs(game_root: &Path, path: &Path) -> Result<(), String> {
    log::trace!("清理空 MOD 目录：起点={}", path.display());
    let root = fs::canonicalize(game_root).unwrap_or_else(|_| game_root.to_path_buf());
    let mut current = Some(path.to_path_buf());
    while let Some(directory) = current {
        let canonical = fs::canonicalize(&directory).unwrap_or(directory.clone());
        if canonical == root || !canonical.starts_with(&root) {
            break;
        }
        let name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if name.eq_ignore_ascii_case("BepInEx") || name.eq_ignore_ascii_case("plugins") {
            break;
        }
        let mut entries = fs::read_dir(&canonical).map_err(|e| e.to_string())?;
        if entries.next().is_some() {
            break;
        }
        fs::remove_dir(&canonical).map_err(|e| e.to_string())?;
        current = canonical.parent().map(Path::to_path_buf);
    }
    Ok(())
}

pub fn clear_readonly(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let mut changed = 0;
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
    {
        let metadata = fs::metadata(entry.path()).map_err(|e| e.to_string())?;
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            fs::set_permissions(entry.path(), permissions).map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    if changed > 0 {
        log::debug!("清除只读属性：路径={}，条目={}", path.display(), changed);
    }
    Ok(())
}

fn path_relative(root: &Path, path: &Path) -> String {
    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&canonical_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn new_manifest(
    repository_id: &str,
    release_tag: &str,
    package_name: Option<&str>,
    note: Option<&str>,
    files: Vec<String>,
) -> InstallManifest {
    InstallManifest {
        repository_id: repository_id.into(),
        release_tag: release_tag.into(),
        package_name: package_name.map(str::to_string),
        note: note.map(str::to_string),
        installed_at: Utc::now().to_rfc3339(),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn copying_an_installed_dll_onto_itself_keeps_its_contents() {
        let root = std::env::temp_dir().join(format!("oc2-same-asset-{}", Uuid::new_v4()));
        let plugins = root.join("BepInEx/plugins");
        fs::create_dir_all(&plugins).unwrap();
        let source = plugins.join("Existing.dll");
        fs::write(&source, b"original plugin").unwrap();
        assert!(copy_asset_pending(&source, &root, "BepInEx/plugins", "Existing.dll").is_err());
        assert_eq!(fs::read(&source).unwrap(), b"original plugin");
        fs::remove_file(source).unwrap();
        fs::remove_dir(plugins).unwrap();
        fs::remove_dir(root.join("BepInEx")).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn downloaded_bytes_require_nonempty_complete_data_and_the_expected_sha256() {
        let digest = format!("sha256:{:x}", Sha256::digest(b"hello"));
        assert!(validate_download_bytes(b"hello", Some(5), Some(&digest)).is_ok());
        assert!(validate_download_bytes(b"HELLO", Some(5), Some(&digest)).is_err());
        assert!(validate_download_bytes(b"hell", Some(5), Some(&digest)).is_err());
        assert!(validate_download_bytes(b"", None, None).is_err());
        assert!(validate_download_bytes(b"hello", Some(5), None).is_ok());
    }

    #[test]
    fn waiting_progress_is_visible_before_size_or_bytes_and_omits_connection_context() {
        let progress = DownloadProgress::connecting("MOD 下载");
        let event = serde_json::to_value(&progress).unwrap();
        assert_eq!(event["label"], "MOD 下载");
        assert_eq!(event["phase"], "connecting");
        assert_eq!(event["downloaded"], 0);
        assert!(event["total"].is_null());
        assert_eq!(event["finished"], false);
        for key in ["route", "source", "attempt", "attempt_total"] {
            assert!(!event.as_object().unwrap().contains_key(key));
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, contents) in entries {
            if name.ends_with('/') {
                writer
                    .add_directory(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                continue;
            }
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn sha256_is_stable() {
        let path = std::env::temp_dir().join("oc2-sha-test.bin");
        fs::write(&path, b"hello").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn zip_slip_is_rejected() {
        let zip_path = std::env::temp_dir().join("oc2-slip-test.zip");
        let root = std::env::temp_dir().join("oc2-slip-root");
        fs::create_dir_all(&root).unwrap();
        let file = File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../outside.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"bad").unwrap();
        writer.finish().unwrap();
        assert!(extract_zip_safe(&zip_path, &root).is_err());
    }

    #[test]
    fn truncated_zip_is_rejected_before_installation() {
        let root = std::env::temp_dir().join(format!("oc2-truncated-zip-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let valid = root.join("valid.zip");
        let truncated = root.join("truncated.zip");
        write_zip(&valid, &[("Plugin.dll", b"plugin")]);
        let bytes = fs::read(&valid).unwrap();
        fs::write(&truncated, &bytes[..bytes.len() - 1]).unwrap();

        assert!(validate_zip(&valid).is_ok());
        assert!(validate_zip(&truncated).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_zip_with_only_root_dlls_merges_into_plugins() {
        let root = std::env::temp_dir().join(format!("oc2-two-dll-zip-{}", Uuid::new_v4()));
        let game = root.join("game");
        let plugins = game.join("BepInEx/plugins");
        fs::create_dir_all(plugins.join("Existing MOD")).unwrap();
        fs::write(plugins.join("First.dll"), b"old DLL").unwrap();
        fs::write(plugins.join("Other.dll"), b"other DLL").unwrap();
        fs::write(plugins.join("Existing MOD/config.json"), b"existing config").unwrap();
        let zip_path = root.join("Two MODs.zip");
        write_zip(
            &zip_path,
            &[("First.dll", b"first DLL"), ("Second.DLL", b"second DLL")],
        );

        let mut installed = extract_local_mod_zip(&zip_path, &game, "Two MODs.zip").unwrap();
        installed.sort();
        assert_eq!(
            installed,
            vec![
                "BepInEx/plugins/First.dll".to_string(),
                "BepInEx/plugins/Second.DLL".to_string()
            ]
        );
        assert!(!plugins.join("Two MODs").exists());
        assert_eq!(fs::read(plugins.join("First.dll")).unwrap(), b"first DLL");
        assert_eq!(fs::read(plugins.join("Second.DLL")).unwrap(), b"second DLL");
        assert_eq!(fs::read(plugins.join("Other.dll")).unwrap(), b"other DLL");
        assert_eq!(
            fs::read(plugins.join("Existing MOD/config.json")).unwrap(),
            b"existing config"
        );

        let canonical_root = fs::canonicalize(&root).unwrap();
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(canonical_root.starts_with(&temp) && canonical_root != temp);
        fs::remove_dir_all(canonical_root).unwrap();
    }

    #[test]
    fn mod_zip_layouts_use_one_shared_install_policy() {
        let root = std::env::temp_dir().join(format!("oc2-zip-layout-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        let folder_zip = root.join("folder.zip");
        write_zip(&folder_zip, &[("Package/Plugin.dll", b"plugin")]);
        fs::create_dir_all(game.join("BepInEx/plugins/Package")).unwrap();
        fs::write(game.join("BepInEx/plugins/Package/Plugin.dll"), b"old").unwrap();
        fs::write(game.join("BepInEx/plugins/Package/other.txt"), b"other").unwrap();
        extract_local_mod_zip(&folder_zip, &game, "folder.zip").unwrap();
        assert!(game.join("BepInEx/plugins/Package/Plugin.dll").is_file());
        assert!(!game.join("BepInEx/plugins/folder").exists());
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/Package/Plugin.dll")).unwrap(),
            b"plugin"
        );
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/Package/other.txt")).unwrap(),
            b"other"
        );

        let dll_zip = root.join("single.dll.zip");
        write_zip(&dll_zip, &[("Single.dll", b"plugin")]);
        fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        fs::write(game.join("BepInEx/plugins/Single.dll"), b"old").unwrap();
        let files = extract_local_mod_zip(&dll_zip, &game, "single.dll.zip").unwrap();
        assert_eq!(files, vec!["BepInEx/plugins/Single.dll"]);
        assert!(game.join("BepInEx/plugins/Single.dll").is_file());
        assert!(!game.join("BepInEx/plugins/single.dll/Single.dll").exists());
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/Single.dll")).unwrap(),
            b"plugin"
        );
        assert!(game.join("BepInEx/plugins/Package/Plugin.dll").is_file());

        let documented_zip = root.join("documented.zip");
        write_zip(
            &documented_zip,
            &[
                ("Documented.dll", b"plugin"),
                ("README.txt", b"instructions"),
            ],
        );
        let files = extract_local_mod_zip(&documented_zip, &game, "documented.zip").unwrap();
        assert_eq!(
            files,
            vec![
                "BepInEx/plugins/documented/Documented.dll",
                "BepInEx/plugins/documented/README.txt"
            ]
        );
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/documented/README.txt")).unwrap(),
            b"instructions"
        );
        assert!(!game.join("BepInEx/plugins/Documented.dll").exists());

        let mixed_zip = root.join("mixed.zip");
        write_zip(
            &mixed_zip,
            &[("Package/Plugin.dll", b"plugin"), ("Loose.dll", b"loose")],
        );
        fs::create_dir_all(game.join("BepInEx/plugins/mixed")).unwrap();
        fs::write(game.join("BepInEx/plugins/mixed/old.txt"), b"old").unwrap();
        extract_local_mod_zip(&mixed_zip, &game, "mixed.zip").unwrap();
        assert!(
            game.join("BepInEx/plugins/mixed/Package/Plugin.dll")
                .is_file()
        );
        assert!(game.join("BepInEx/plugins/mixed/Loose.dll").is_file());
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/mixed/old.txt")).unwrap(),
            b"old"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mod_install_scenarios_respect_local_and_repository_targets() {
        struct Case {
            entries: &'static [(&'static str, &'static [u8])],
            local: &'static [&'static str],
            target: &'static str,
            online: &'static [&'static str],
        }
        let cases = [
            Case {
                entries: &[("Only.dll", b"DLL")],
                local: &["BepInEx/plugins/Only.dll"],
                target: "BepInEx/plugins/Configured",
                online: &["BepInEx/plugins/Configured/Only.dll"],
            },
            Case {
                entries: &[
                    ("OC2HostUtilities/", b""),
                    ("OC2HostUtilities/Main.dll", b"DLL"),
                    ("OC2HostUtilities/data/config.json", b"data"),
                ],
                local: &[
                    "BepInEx/plugins/OC2HostUtilities/Main.dll",
                    "BepInEx/plugins/OC2HostUtilities/data/config.json",
                ],
                target: "BepInEx/plugins/Configured",
                online: &[
                    "BepInEx/plugins/Configured/OC2HostUtilities/Main.dll",
                    "BepInEx/plugins/Configured/OC2HostUtilities/data/config.json",
                ],
            },
            Case {
                entries: &[
                    ("BepInEx/", b""),
                    ("BepInEx/plugins/Main.dll", b"DLL"),
                    ("BepInEx/config/mod.cfg", b"config"),
                ],
                local: &["BepInEx/plugins/Main.dll", "BepInEx/config/mod.cfg"],
                target: "BepInEx",
                online: &["BepInEx/plugins/Main.dll", "BepInEx/config/mod.cfg"],
            },
            Case {
                entries: &[("plugins/", b""), ("plugins/Main.dll", b"DLL")],
                local: &["BepInEx/plugins/Main.dll"],
                target: "BepInEx",
                online: &["BepInEx/plugins/Main.dll"],
            },
            Case {
                entries: &[
                    ("OC2HostUtilities/Main.dll", b"DLL"),
                    ("HostUtilities.dll", b"loose"),
                    ("README.txt", b"readme"),
                ],
                local: &[
                    "BepInEx/plugins/Scenario/OC2HostUtilities/Main.dll",
                    "BepInEx/plugins/Scenario/HostUtilities.dll",
                    "BepInEx/plugins/Scenario/README.txt",
                ],
                target: "BepInEx/plugins/Configured",
                online: &[
                    "BepInEx/plugins/Configured/Scenario/OC2HostUtilities/Main.dll",
                    "BepInEx/plugins/Configured/Scenario/HostUtilities.dll",
                    "BepInEx/plugins/Configured/Scenario/README.txt",
                ],
            },
        ];
        let root = std::env::temp_dir().join(format!("oc2-install-scenarios-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let zip = root.join("Scenario.zip");
        for (index, case) in cases.iter().enumerate() {
            write_zip(&zip, case.entries);
            for online in [false, true] {
                let game = root.join(format!("game-{index}-{online}"));
                let expected = if online { case.online } else { case.local };
                // Exercise merging into existing folders and creating missing subfolders.
                let replaced = game.join(expected[0]);
                fs::create_dir_all(replaced.parent().unwrap()).unwrap();
                fs::write(&replaced, b"old").unwrap();
                fs::write(game.join("unrelated.txt"), b"other").unwrap();
                let files = if online {
                    extract_mod_zip(&zip, &game, case.target, "Scenario.zip").unwrap()
                } else {
                    extract_local_mod_zip(&zip, &game, "Scenario.zip").unwrap()
                };
                assert_eq!(files, expected, "scenario {index}, online={online}");
                for (relative, (_, contents)) in files
                    .iter()
                    .zip(case.entries.iter().filter(|(name, _)| !name.ends_with('/')))
                {
                    assert_eq!(&fs::read(game.join(relative)).unwrap(), contents);
                }
                assert_eq!(fs::read(game.join("unrelated.txt")).unwrap(), b"other");
            }
        }
        let source = root.join("Only.dll");
        fs::write(&source, b"direct DLL").unwrap();
        for target in ["BepInEx/plugins", "BepInEx/plugins/Configured"] {
            let game = root.join(format!("direct-{}", target.len()));
            fs::create_dir_all(&game).unwrap();
            let files = copy_asset(&source, &game, target, "Only.dll").unwrap();
            assert_eq!(files, vec![format!("{target}/Only.dll")]);
            assert_eq!(fs::read(game.join(&files[0])).unwrap(), b"direct DLL");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn special_root_merging_obeys_empty_or_explicit_online_target_and_creates_missing_folders() {
        let root = std::env::temp_dir().join(format!("oc2-special-targets-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let zip = root.join("Scenario.zip");
        for (prefix, target, expected) in [
            ("BepInEx/plugins/", "", "BepInEx/plugins/Main.dll"),
            ("BepInEx/plugins/", "BepInEx", "BepInEx/plugins/Main.dll"),
            ("plugins/", "", "plugins/Main.dll"),
            ("plugins/", "BepInEx", "BepInEx/plugins/Main.dll"),
            ("plugins/", "BepInEx/plugins", "BepInEx/plugins/Main.dll"),
        ] {
            for existing in [false, true] {
                let game = root.join(Uuid::new_v4().to_string());
                fs::create_dir_all(&game).unwrap();
                if existing {
                    let path = game.join(expected);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(&path, b"old").unwrap();
                    fs::write(path.parent().unwrap().join("Other.dll"), b"other").unwrap();
                }
                let name = format!("{prefix}Main.dll");
                write_zip(&zip, &[(name.as_str(), b"new")]);
                let files = extract_mod_zip(&zip, &game, target, "Scenario.zip").unwrap();
                assert_eq!(files, vec![expected]);
                assert_eq!(fs::read(game.join(expected)).unwrap(), b"new");
                if existing {
                    assert_eq!(
                        fs::read(game.join(expected).parent().unwrap().join("Other.dll")).unwrap(),
                        b"other"
                    );
                }
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_manifest_commit_restores_overwritten_dll_and_zip_files() {
        for zip in [false, true] {
            let root =
                std::env::temp_dir().join(format!("oc2-manifest-rollback-{}", Uuid::new_v4()));
            let game = root.join("game");
            let plugins = game.join("BepInEx/plugins");
            fs::create_dir_all(&plugins).unwrap();
            fs::write(plugins.join("Existing.dll"), b"old DLL").unwrap();
            let asset = root.join(if zip { "update.zip" } else { "Existing.dll" });
            let pending = if zip {
                write_zip(
                    &asset,
                    &[("Existing.dll", b"new DLL"), ("New.dll", b"new member")],
                );
                extract_local_mod_zip_pending(&asset, &game, "update.zip").unwrap()
            } else {
                fs::write(&asset, b"new DLL").unwrap();
                copy_asset_pending(&asset, &game, "BepInEx/plugins", "Existing.dll").unwrap()
            };
            assert_eq!(fs::read(plugins.join("Existing.dll")).unwrap(), b"new DLL");
            let manifest = new_manifest("package", "v2", None, None, pending.files().to_vec());
            let blocked = root.join("blocked-manifests");
            fs::write(&blocked, b"not a directory").unwrap();
            assert!(write_manifest(&blocked, &manifest).is_err());
            drop(pending);
            assert_eq!(fs::read(plugins.join("Existing.dll")).unwrap(), b"old DLL");
            assert!(!plugins.join("New.dll").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn multiple_folder_fallback_preserves_structure_and_duplicate_targets_are_rejected() {
        let root = std::env::temp_dir().join(format!("oc2-zip-fallback-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        let zip = root.join("Bundle.zip");
        write_zip(
            &zip,
            &[("First/A.dll", b"first"), ("Second/B.dll", b"second")],
        );
        let local = extract_local_mod_zip(&zip, &game, "Bundle.zip").unwrap();
        assert_eq!(
            local,
            vec![
                "BepInEx/plugins/Bundle/First/A.dll",
                "BepInEx/plugins/Bundle/Second/B.dll"
            ]
        );
        let online = extract_mod_zip(&zip, &game, "Configured", "Bundle.zip").unwrap();
        assert_eq!(
            online,
            vec![
                "Configured/Bundle/First/A.dll",
                "Configured/Bundle/Second/B.dll"
            ]
        );
        write_zip(
            &zip,
            &[("Duplicated.dll", b"first"), ("DUPLICATED.DLL", b"second")],
        );
        assert!(extract_local_mod_zip(&zip, &game, "Bundle.zip").is_err());
        assert!(!game.join("BepInEx/plugins/Duplicated.dll").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn online_mixed_zip_uses_archive_named_directory_and_reuses_explicit_package_target() {
        let root = std::env::temp_dir().join(format!("oc2-mixed-targets-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        // Cache names do not identify the downloaded ZIP's package directory.
        let zip = root.join("random-cache-file.tmp");
        write_zip(
            &zip,
            &[
                ("Folder/Main.dll", b"plugin"),
                ("Loose.dll", b"loose"),
                ("README.txt", b"readme"),
            ],
        );
        for (target, name, package) in [
            ("BepInEx/plugins", "Bundle.zip", "BepInEx/plugins/Bundle"),
            (
                "BepInEx/plugins/Bundle",
                "Bundle.zip",
                "BepInEx/plugins/Bundle",
            ),
            (
                "BepInEx/plugins/bundle",
                "Bundle.zip",
                "BepInEx/plugins/bundle",
            ),
            ("", "Bundle.zip", "Bundle"),
            ("BepInEx/plugins", "plugins.zip", "BepInEx/plugins/plugins"),
            ("BepInEx", "BepInEx.zip", "BepInEx/BepInEx"),
        ] {
            let game = root.join(Uuid::new_v4().to_string());
            let package_path = game.join(package);
            fs::create_dir_all(package_path.join("Folder")).unwrap();
            fs::write(package_path.join("Folder/Main.dll"), b"old").unwrap();
            fs::write(game.join("Unrelated.dll"), b"other").unwrap();
            let files = extract_mod_zip(&zip, &game, target, name).unwrap();
            assert_eq!(
                files,
                vec![
                    format!("{package}/Folder/Main.dll"),
                    format!("{package}/Loose.dll"),
                    format!("{package}/README.txt")
                ]
            );
            assert_eq!(
                fs::read(package_path.join("Folder/Main.dll")).unwrap(),
                b"plugin"
            );
            assert_eq!(fs::read(package_path.join("Loose.dll")).unwrap(), b"loose");
            assert_eq!(
                fs::read(package_path.join("README.txt")).unwrap(),
                b"readme"
            );
            assert_eq!(fs::read(game.join("Unrelated.dll")).unwrap(), b"other");
            if target != package {
                assert!(!game.join(target).join("Loose.dll").exists());
                assert!(!game.join(target).join("README.txt").exists());
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn online_zip_merges_target_wrappers_and_preserves_ordinary_directories() {
        let root = std::env::temp_dir().join(format!("oc2-online-zip-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        let bepinex_zip = root.join("bepinex.zip");
        write_zip(&bepinex_zip, &[("BepInEx/plugins/Online.dll", b"plugin")]);
        extract_mod_zip(&bepinex_zip, &game, "BepInEx", "bepinex.zip").unwrap();
        assert!(game.join("BepInEx/plugins/Online.dll").is_file());
        assert!(!game.join("BepInEx/BepInEx/plugins/Online.dll").exists());

        let plugins_zip = root.join("plugins.zip");
        write_zip(&plugins_zip, &[("plugins/Online2.dll", b"plugin")]);
        extract_mod_zip(&plugins_zip, &game, "BepInEx/plugins", "plugins.zip").unwrap();
        assert!(game.join("BepInEx/plugins/Online2.dll").is_file());

        let arbitrary_zip = root.join("arbitrary.zip");
        write_zip(&arbitrary_zip, &[("Package/Online3.dll", b"plugin")]);
        extract_mod_zip(&arbitrary_zip, &game, "BepInEx/plugins", "arbitrary.zip").unwrap();
        assert!(game.join("BepInEx/plugins/Package/Online3.dll").is_file());
        assert!(!game.join("BepInEx/plugins/Online3.dll").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn online_zip_keeps_diy_level_root_and_nested_resource_paths() {
        let root = std::env::temp_dir().join(format!("oc2-diy-level-zip-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        let zip = root.join("DIYLevel.zip");
        write_zip(
            &zip,
            &[
                ("OC2DIYLevel/OC2DIYLevel.dll", b"plugin"),
                ("OC2DIYLevel/levels/story/example.json", b"level"),
                ("OC2DIYLevel/assets/icons/example.png", b"icon"),
            ],
        );

        let files = extract_mod_zip(&zip, &game, "BepInEx/plugins", "DIYLevel.zip").unwrap();
        assert_eq!(
            files,
            vec![
                "BepInEx/plugins/OC2DIYLevel/OC2DIYLevel.dll",
                "BepInEx/plugins/OC2DIYLevel/levels/story/example.json",
                "BepInEx/plugins/OC2DIYLevel/assets/icons/example.png",
            ]
        );
        for relative in files {
            assert!(game.join(relative).is_file());
        }
        assert!(!game.join("BepInEx/plugins/OC2DIYLevel.dll").exists());
        assert!(!game.join("BepInEx/plugins/levels").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn online_zip_explicit_target_deduplicates_only_special_directory_prefixes() {
        let root = std::env::temp_dir().join(format!("oc2-explicit-target-zip-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        for (index, prefix) in [
            "BepInEx/plugins/OC2DIYLevel",
            "plugins/OC2DIYLevel",
            "OC2DIYLevel",
        ]
        .iter()
        .enumerate()
        {
            let zip = root.join(format!("target-{index}.zip"));
            let entry = format!("{prefix}/resources/Level{index}.json");
            write_zip(&zip, &[(entry.as_str(), b"level")]);
            let files = extract_mod_zip(&zip, &game, "BepInEx/plugins/OC2DIYLevel", "DIYLevel.zip")
                .unwrap();
            assert_eq!(
                files,
                vec![if index < 2 {
                    format!("BepInEx/plugins/OC2DIYLevel/resources/Level{index}.json")
                } else {
                    format!("BepInEx/plugins/OC2DIYLevel/OC2DIYLevel/resources/Level{index}.json")
                }]
            );
            assert!(game.join(&files[0]).is_file());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn online_zip_rejects_unsafe_paths_before_writing_preserved_directories() {
        let root = std::env::temp_dir().join(format!("oc2-online-unsafe-zip-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        let zip = root.join("unsafe.zip");
        write_zip(
            &zip,
            &[
                ("OC2DIYLevel/Plugin.dll", b"plugin"),
                ("../outside.dll", b"unsafe"),
            ],
        );

        assert!(extract_mod_zip(&zip, &game, "BepInEx/plugins", "unsafe.zip").is_err());
        assert!(!game.join("BepInEx/plugins/OC2DIYLevel/Plugin.dll").exists());
        assert!(!root.join("outside.dll").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn online_zip_rolls_back_written_files_when_preserved_directory_install_fails() {
        let root = std::env::temp_dir().join(format!("oc2-online-rollback-zip-{}", Uuid::new_v4()));
        let game = root.join("game");
        let blocked = game.join("BepInEx/plugins/OC2DIYLevel/Blocked.dll");
        fs::create_dir_all(&blocked).unwrap();
        fs::write(blocked.join("existing.txt"), b"existing").unwrap();
        let replaced = game.join("BepInEx/plugins/OC2DIYLevel/Plugin.dll");
        fs::write(&replaced, b"old plugin").unwrap();
        let zip = root.join("rollback.zip");
        write_zip(
            &zip,
            &[
                ("OC2DIYLevel/Plugin.dll", b"plugin"),
                ("OC2DIYLevel/Blocked.dll", b"blocked"),
            ],
        );

        assert!(extract_mod_zip(&zip, &game, "BepInEx/plugins", "rollback.zip").is_err());
        assert_eq!(fs::read(replaced).unwrap(), b"old plugin");
        assert_eq!(fs::read(blocked.join("existing.txt")).unwrap(), b"existing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_non_special_plugin_directory_is_preserved() {
        let root = std::env::temp_dir().join(format!("oc2-local-plugin-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        let plugin_zip = root.join("plugin.zip");
        write_zip(&plugin_zip, &[("plugin/Plugin.dll", b"plugin")]);
        extract_local_mod_zip(&plugin_zip, &game, "plugin.zip").unwrap();
        assert!(game.join("BepInEx/plugins/plugin/Plugin.dll").is_file());
        assert!(!game.join("BepInEx/plugin/Plugin.dll").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_special_roots_with_other_top_level_entries_preserve_the_whole_archive() {
        let root = std::env::temp_dir().join(format!("oc2-local-special-root-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        let bepinex_zip = root.join("bepinex-mixed.zip");
        write_zip(
            &bepinex_zip,
            &[
                ("BepInEx/plugins/Mixed.dll", b"plugin"),
                ("README.txt", b"documentation"),
            ],
        );
        extract_local_mod_zip(&bepinex_zip, &game, "bepinex-mixed.zip").unwrap();
        assert!(
            game.join("BepInEx/plugins/bepinex-mixed/BepInEx/plugins/Mixed.dll")
                .is_file()
        );
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/bepinex-mixed/README.txt")).unwrap(),
            b"documentation"
        );
        assert!(!game.join("README.txt").exists());

        let plugins_zip = root.join("plugins-mixed.zip");
        write_zip(
            &plugins_zip,
            &[
                ("plugins/PluginsMixed.dll", b"plugin"),
                ("Loose.dll", b"documentation"),
            ],
        );
        extract_local_mod_zip(&plugins_zip, &game, "plugins-mixed.zip").unwrap();
        assert!(
            game.join("BepInEx/plugins/plugins-mixed/plugins/PluginsMixed.dll")
                .is_file()
        );
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/plugins-mixed/Loose.dll")).unwrap(),
            b"documentation"
        );
        assert!(!game.join("BepInEx/plugins/Loose.dll").exists());

        fs::remove_dir_all(root).unwrap();
    }
}
