use super::control::is_download_cancelled;
use super::integrity::validate_download_size;
use super::progress::{DownloadProgress, DownloadReporter, emit_progress};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const FIRST_BYTE_TIMEOUT_ERROR: &str = "download.firstByteTimeout";

/// 「多久没有动静就判定这个下载源已死」的唯一判定线。
///
/// 这份文件的所有下载入口（Release 资产、直链、BepInEx 框架包、安装器）
/// 共用它：竞速会同时向多个候选发请求，等太久会把一个坏源拖成整批失败，
/// 太快又会误杀慢速但可用的源。它是网络判定而不是用户偏好，所以写死在这里，
/// 不进配置、不开放修改。
pub const FIRST_BYTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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
                    // HEAD has no body, so Reqwest's body size can be zero even
                    // when Content-Length advertises the downloadable file size.
                    response
                        .headers()
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
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
                    Err(error) if is_download_cancelled() => {
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
        return Err("download.cancelled".into());
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
    .map_err(|error| error.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(crate::locale_tag::with_params(
            "download.httpStatus",
            &[("status", &response.status().to_string())],
        ));
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
            return Err("download.cancelled".into());
        }
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
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
        return Err(crate::locale_tag::with_params(
            "download.diskLengthMismatch",
            &[
                ("actual", &actual.to_string()),
                ("downloaded", &downloaded.to_string()),
            ],
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
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        reporter.received(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > expected {
            return Err(crate::locale_tag::with_params(
                "download.chunkTooLong",
                &[("bytes", &bytes.len().to_string())],
            ));
        }
        next_chunk = body.next().await;
    }
    if bytes.len() as u64 != expected {
        return Err(crate::locale_tag::with_params(
            "download.chunkLengthMismatch",
            &[("bytes", &bytes.len().to_string())],
        ));
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
                if is_download_cancelled() {
                    return Err("download.cancelled".to_string());
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
                            last_error = "download.badRangeResponse".into();
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
                        last_error = crate::locale_tag::with_params(
                            "download.rangeUnsupported",
                            &[("status", &status.to_string())],
                        );
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
                        last_error = error.without_url().to_string();
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
        if is_download_cancelled() {
            return Err("download.cancelled".into());
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
        return Err(crate::locale_tag::with_params(
            "download.chunkDiskLengthMismatch",
            &[
                ("actual", &actual_size.to_string()),
                ("total", &total.to_string()),
            ],
        ));
    }
    emit_progress(reporter, label, total, Some(total), started.elapsed(), true);
    Ok(())
}
