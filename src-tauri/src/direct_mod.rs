use crate::app_state::AppState;
use crate::archive::{self, DownloadRoute};
use crate::download::planning::{
    RemoteUrlKind, classify_remote_url, generic_download_threads, plan_pasted_file,
    provider_for_remote_url,
};
use crate::download::{DownloadCandidate, DownloadContext, try_download_candidates};
use crate::http_diagnostics::HttpRequestExt;
use crate::models::RepositoryKind;
use crate::network_policy::NetworkPolicy;
use crate::releases;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, State};
use url::Url;

/// 直链与 Release / Raw 资产的分片下载线程数。
///
/// 三类都是「一个完整文件」，分片并发能显著提升单文件吞吐；但竞速时
/// 每个源只开 1 个分片连接（见 `download_remote_to_temp`），否则多源 ×
/// 多分片会把带宽切碎，反而更慢。
pub(crate) const DIRECT_DOWNLOAD_THREADS: u32 = 32;

/// 下载完成后的内容校验方式。
///
/// 校验放在竞速回调里而不是整段下载之后：竞速时先到的文件可能是代理返回
/// 的错误页，只有通过校验才算这个源成功，否则让下一个候选继续跑。
#[derive(Debug, Clone, Copy)]
pub(crate) enum RemoteValidation {
    /// MOD 文件：`.zip` 校验 ZIP 结构，其余按 PE 校验（`validate_mod_file`）。
    ModFile,
    /// 关卡集 ZIP：完整读一遍条目（`archive::validate_zip`）。
    Zip,
}

fn decode_filename(value: &str) -> String {
    url::form_urlencoded::parse(
        format!("name={}", value.replace('+', "%2B").replace('&', "%26")).as_bytes(),
    )
    .next()
    .map(|(_, value)| value.into_owned())
    .unwrap_or_default()
}

fn supported_filename(value: &str) -> Option<String> {
    let name = decode_filename(value);
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || name.ends_with(['.', ' '])
    {
        return None;
    }
    let extension = Path::new(&name).extension()?.to_str()?;
    (extension.eq_ignore_ascii_case("dll") || extension.eq_ignore_ascii_case("zip")).then_some(name)
}

fn download_filename(url: &Url, disposition: Option<&str>) -> Result<String, String> {
    if let Some(header) = disposition {
        // Prefer RFC 5987 UTF-8 filenames, then the ordinary filename parameter.
        for extended in [true, false] {
            for parameter in header.split(';').skip(1) {
                if let Some((key, value)) = parameter.trim().split_once('=') {
                    let expected = if extended { "filename*" } else { "filename" };
                    if key.trim().eq_ignore_ascii_case(expected) {
                        let value = value.trim().trim_matches('"');
                        let value = if extended {
                            let Some((encoding, rest)) = value.split_once('\'') else {
                                continue;
                            };
                            if !encoding.eq_ignore_ascii_case("utf-8") {
                                continue;
                            }
                            let Some((_, filename)) = rest.split_once('\'') else {
                                continue;
                            };
                            filename
                        } else {
                            value
                        };
                        return supported_filename(value)
                            .ok_or_else(|| "mods.unsupportedFileType".into());
                    }
                }
            }
        }
    }
    url.path_segments()
        .and_then(|mut parts| parts.next_back())
        .and_then(supported_filename)
        .ok_or_else(|| "mods.unsupportedFileType".into())
}

fn validate_mod_file(
    path: &Path,
    filename: &str,
    cancelled: impl Fn() -> bool,
) -> Result<(), String> {
    archive::validate_download_file(path, None, None)?;
    if filename.to_ascii_lowercase().ends_with(".zip") {
        return archive::validate_zip_with_cancel(path, cancelled);
    }
    // Reject HTML, renamed executables, and truncated files before installing a DLL.
    let invalid = || "direct.notDll".to_string();
    let mut file = File::open(path).map_err(|_| invalid())?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).map_err(|_| invalid())?;
    if &dos[..2] != b"MZ" {
        return Err(invalid());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
    if offset < 64 {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(offset.into()))
        .map_err(|_| invalid())?;
    let mut pe = [0u8; 24];
    file.read_exact(&mut pe).map_err(|_| invalid())?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[22], pe[23]]) & 0x2000 == 0 {
        return Err(invalid());
    }
    Ok(())
}

/// 用一次 `bytes=0-0` 的 Range 请求探测文件名（顺带暴露 404/403 或登录页）。
///
/// 探测本身也响应取消：用户点了取消后不应该再等满首字节超时。
async fn probe_remote_filename(
    client: &reqwest::Client,
    parsed: &Url,
    first_byte_timeout: std::time::Duration,
) -> Result<String, String> {
    let response = tokio::select! {
        result = client.get(parsed.clone()).timeout(first_byte_timeout).header(reqwest::header::RANGE, "bytes=0-0")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send_logged() => {
            result.map_err(|_| "direct.infoFailed")?.error_for_status().map_err(|_| "direct.infoFailed")?
        }
        _ = async {
            while !archive::is_download_cancelled() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } => return Err("download.cancelled".into()),
    };
    let disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok());
    let filename = download_filename(response.url(), disposition).or_else(|error| {
        if disposition.is_none() {
            download_filename(parsed, None)
        } else {
            Err(error)
        }
    })?;
    drop(response);
    Ok(filename)
}

/// 把一个远程 URL 下载到缓存目录的临时文件，返回（临时路径，文件名）。
///
/// Release 和 Raw 使用平台与地区的文件下载计划，其它链接沿用直连策略。
/// 取消标记在这里重置 —— 调用方若在等待元数据后再进来，用户已点的取消
/// 会被清掉，所以带元数据等待的命令必须先自己 `archive::start_download`。
pub(crate) async fn download_remote_to_temp(
    state: &AppState,
    url: &str,
    label: &str,
    configured_threads: u32,
    validation: RemoteValidation,
) -> Result<(PathBuf, String), String> {
    archive::start_download();
    let parsed = Url::parse(url.trim()).map_err(|_| "direct.invalidUrl")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("direct.invalidUrl".into());
    }
    let config = state.config.load_or_default();
    let kind = classify_remote_url(parsed.as_str());
    let (candidates, threads, execution) = match kind {
        RemoteUrlKind::Other => (
            vec![DownloadCandidate {
                route: DownloadRoute::Direct,
                url: parsed.to_string(),
                identity: None,
            }],
            generic_download_threads(
                provider_for_remote_url(parsed.as_str()).as_ref(),
                &config.region,
                configured_threads,
            ),
            None,
        ),
        _ => {
            let policy = config
                .network_policy
                .clone()
                .map(Ok)
                .unwrap_or_else(NetworkPolicy::active)?;
            let repository_kind = match validation {
                RemoteValidation::ModFile => RepositoryKind::Mod,
                RemoteValidation::Zip => RepositoryKind::Map,
            };
            let planned = NetworkPolicy::with_snapshot(policy, || {
                plan_pasted_file(&parsed, kind, repository_kind, &config.region)
            })?;
            (
                planned.candidates,
                planned.download_threads,
                Some(planned.execution),
            )
        }
    };
    let client = releases::build_download_client(config.network.first_byte_timeout())
        .map_err(|_| "direct.clientFailed")?;
    /*
     * 文件名：只有「单一候选且就是直连」时才先探测一次。
     *
     * 竞速或走代理时不能拿原始 URL 去探测 —— 中国大陆直连 GitHub 本身就可能
     * 超时，探测失败会把本来能靠代理成功的下载提前掐掉。这两种情况直接用
     * URL 最后一段：Release 资产与 raw 文件名本来就是路径末段。
     */
    let probe_direct = candidates.len() == 1 && candidates[0].route == DownloadRoute::Direct;
    let filename = if probe_direct {
        let mut connecting = archive::DownloadProgress::connecting(label);
        connecting.route = Some(DownloadRoute::Direct);
        connecting.source = parsed.host_str().map(str::to_owned);
        archive::publish_download_progress(connecting);
        probe_remote_filename(&client, &parsed, config.network.first_byte_timeout()).await?
    } else {
        download_filename(&parsed, None)?
    };
    let cache_dir = state.config.cache_dir();
    log::info!(
        "远程文件下载：类别={:?}，域名={}，候选数={}，线程数={}，文件名={}",
        kind,
        parsed.host_str().unwrap_or_default(),
        candidates.len(),
        threads,
        filename
    );
    let first_byte_timeout = config.network.first_byte_timeout();
    let session = state.download_history.session();
    let race_session = execution
        .is_some_and(|execution| execution.race && candidates.len() > 1)
        .then_some(&session);
    let temp = try_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            let filename = filename.clone();
            async move {
                let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    label,
                    &reporter,
                    threads,
                    Some(first_byte_timeout),
                )
                .await?;
                archive::verify_temp_download(path, &reporter, move |path, cancelled| {
                    match validation {
                        RemoteValidation::ModFile => validate_mod_file(path, &filename, cancelled),
                        RemoteValidation::Zip => archive::validate_zip_with_cancel(path, cancelled),
                    }
                })
                .await
            }
        },
        &config.network,
        Some(DownloadContext {
            execution,
            race_session,
            label,
            file_name: Some(&filename),
            expected_size: None,
        }),
    )
    .await?;
    session.commit();
    Ok((temp, filename))
}

/// 只下载不安装：把直链内容落到临时文件，返回（临时路径，文件名）。
///
/// 安装预览用它：先下载一次、解析、让用户确认，确认后从暂存安装，
/// 避免「预览下载一次、安装再下载一次」。调用方负责删除返回的临时文件
/// （`analyze_mod_source` 会把它搬到预览暂存目录）。
pub(crate) async fn download_for_preview(
    state: &AppState,
    url: &str,
) -> Result<(PathBuf, String), String> {
    let (temp, filename) = download_remote_to_temp(
        state,
        url,
        "direct.onlineFile",
        DIRECT_DOWNLOAD_THREADS,
        RemoteValidation::ModFile,
    )
    .await?;
    log::info!("在线 MOD 预览下载完成：文件名={filename}");
    Ok((temp, filename))
}

#[tauri::command]
pub async fn install_online_mod(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<crate::models::OperationResult, String> {
    let _ = (app, state, url);
    crate::mod_changes::guard::require_shared_plan()
}

#[cfg(test)]
mod tests;
