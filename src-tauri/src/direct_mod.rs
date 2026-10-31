use crate::archive::{self, DownloadRoute};
use crate::commands::{AppState, install_manual_mod, plan_remote_download_candidates};
use crate::download::{DownloadCandidate, DownloadContext, try_download_candidates};
use crate::models::Provider;
use crate::{game, releases};
use std::fs::{self, File};
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

/// 远程 URL 的下载类别。
///
/// 三类走完全不同的路由：Release 资产只认 `release_proxies`，仓库 raw
/// 文件认 `raw_proxies` 且代理模板要用分支与文件路径展开，其它直链没有
/// 代理可用、直接单源多线程。判定结果只决定「用哪套代理与线程策略」，
/// 不决定 URL 是否可信（那由下载入口校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteUrlKind {
    ReleaseAsset,
    Raw,
    Other,
}

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

/// 按 URL 判定下载类别；解析失败或不是 HTTP(S) 一律按 `Other`（真正的
/// 拒绝在下载入口，这里只决定用哪套下载策略）。
pub(crate) fn classify_remote_url(url: &str) -> RemoteUrlKind {
    let Ok(parsed) = Url::parse(url.trim()) else {
        return RemoteUrlKind::Other;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return RemoteUrlKind::Other;
    }
    let host = parsed.host_str().unwrap_or_default();
    let is_github = host.eq_ignore_ascii_case("github.com");
    let is_gitee = host.eq_ignore_ascii_case("gitee.com");
    let path = parsed.path();
    if (is_github || is_gitee) && path.contains("/releases/download/") {
        return RemoteUrlKind::ReleaseAsset;
    }
    if host.eq_ignore_ascii_case("raw.githubusercontent.com") {
        return RemoteUrlKind::Raw;
    }
    if (is_github || is_gitee) && is_repository_raw_path(path) {
        return RemoteUrlKind::Raw;
    }
    RemoteUrlKind::Other
}

/// `/{owner}/{repo}/raw/{...}`：GitHub / Gitee 仓库文件的 raw 入口。
fn is_repository_raw_path(path: &str) -> bool {
    let mut segments = path.trim_start_matches('/').split('/');
    let _owner = segments.next();
    let _repo = segments.next();
    segments.next() == Some("raw") && segments.next().is_some()
}

/// 按 URL 判定下载源平台：决定用哪组代理、以及是否需要竞速。
fn provider_for_remote_url(url: &str) -> Provider {
    let host = Url::parse(url.trim())
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase));
    match host.as_deref() {
        Some("gitee.com") => Provider::Gitee,
        _ => Provider::Github,
    }
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

fn validate_mod_file(path: &Path, filename: &str) -> Result<(), String> {
    archive::validate_download_file(path, None, None)?;
    if filename.to_ascii_lowercase().ends_with(".zip") {
        return archive::validate_zip(path);
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
/// 探测本身也响应取消：用户点了取消后不应该再等这 5 秒超时。
async fn probe_remote_filename(client: &reqwest::Client, parsed: &Url) -> Result<String, String> {
    let response = tokio::select! {
        result = client.get(parsed.clone()).timeout(Duration::from_secs(5)).header(reqwest::header::RANGE, "bytes=0-0")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::USER_AGENT, releases::APP_USER_AGENT).send() => {
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
/// 这是「直链 / Release 资产 / 仓库 raw 文件」唯一的下载入口，候选与线程
/// 策略按 [`classify_remote_url`] 统一决定：
/// - Release 资产：GitHub 用 `release_proxies` 竞速，Gitee 只直连；
/// - Raw 文件：GitHub 用 `raw_proxies` 竞速，Gitee 只直连；
/// - 其它直链：单一直连候选，固定 [`DIRECT_DOWNLOAD_THREADS`] 线程。
///
/// 竞速时每个源只用 1 个分片连接；非竞速时用 `configured_threads`。
/// 取消标记在这里重置 —— 调用方若在等待元数据后再进来，用户已点的取消
/// 会被清掉，所以带元数据等待的命令必须先自己 `archive::start_download`。
pub(crate) async fn download_remote_to_temp(
    app: &AppHandle,
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
    let candidates = match kind {
        RemoteUrlKind::Other => vec![DownloadCandidate {
            route: DownloadRoute::Direct,
            url: parsed.to_string(),
        }],
        _ => plan_remote_download_candidates(
            &config,
            &provider_for_remote_url(parsed.as_str()),
            kind,
            parsed.as_str(),
        )?,
    };
    let racing = candidates.len() > 1;
    let threads = if racing { 1 } else { configured_threads };
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(config.network.timeout_seconds.max(1)))
        .read_timeout(Duration::from_secs(120))
        .user_agent(releases::APP_USER_AGENT)
        .build()
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
        probe_remote_filename(&client, &parsed).await?
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
                    Some(Duration::from_secs(5)),
                )
                .await?;
                let checked = match validation {
                    RemoteValidation::ModFile => validate_mod_file(&path, &filename),
                    RemoteValidation::Zip => archive::validate_zip(&path),
                };
                if let Err(error) = checked {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                Ok(path)
            }
        },
        Some(DownloadContext {
            // 传入 app：直链、Release 资产与仓库资产共用同一套下载进度事件；
            // 路径是临时文件、没有 Release 元数据，所以 expected_size 留空，
            // 大小由下载源自己上报（有 Content-Length 时显示确定进度）。
            app: Some(app),
            label,
            expected_size: None,
        }),
    )
    .await?;
    Ok((temp, filename))
}

/// 只下载不安装：把直链内容落到临时文件，返回（临时路径，文件名）。
///
/// 安装预览用它：先下载一次、解析、让用户确认，确认后从暂存安装，
/// 避免「预览下载一次、安装再下载一次」。调用方负责删除返回的临时文件
/// （`analyze_mod_source` 会把它搬到预览暂存目录）。
pub(crate) async fn download_for_preview(
    app: &tauri::AppHandle,
    state: &AppState,
    url: &str,
) -> Result<(PathBuf, String), String> {
    let (temp, filename) = download_remote_to_temp(
        app,
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
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let (temp, filename) = download_remote_to_temp(
        &app,
        &state,
        &url,
        "direct.onlineFile",
        DIRECT_DOWNLOAD_THREADS,
        RemoteValidation::ModFile,
    )
    .await?;
    let cache_dir = state.config.cache_dir();
    let staging = cache_dir.join(format!("manual-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        fs::create_dir(&staging).map_err(|_| "direct.tempDirFailed")?;
        let source = staging.join(&filename);
        fs::rename(&temp, &source).map_err(|_| "direct.prepareFailed")?;
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        install_manual_mod(&state, &source, Some(&game_root))
    })();
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_dir_all(&staging);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_url_kind_follows_host_and_path() {
        // Release 资产：两个平台的 /releases/download/ 路径
        for url in [
            "https://github.com/CH3NGYZ/demo/releases/download/v1.0.0/demo.zip",
            "https://gitee.com/ch3ngyz/demo/releases/download/v1.0.0/demo.zip",
            "https://github.com/CH3NGYZ/demo/releases/download/v1.0.0/sub/demo.dll",
        ] {
            assert_eq!(
                classify_remote_url(url),
                RemoteUrlKind::ReleaseAsset,
                "{url}"
            );
        }
        // Raw：raw.githubusercontent 与两个平台的仓库 raw 入口
        for url in [
            "https://raw.githubusercontent.com/CH3NGYZ/demo/main/README.md",
            "https://github.com/CH3NGYZ/demo/raw/main/maps/plus.zip",
            "https://github.com/CH3NGYZ/demo/raw/refs/heads/main/maps/plus.zip",
            "https://gitee.com/ch3ngyz/demo/raw/main/maps/plus.zip",
        ] {
            assert_eq!(classify_remote_url(url), RemoteUrlKind::Raw, "{url}");
        }
        // 其它直链：普通站点、仓库主页、以及只出现在查询串里的关键词都不改类别
        for url in [
            "https://example.com/files/map.zip",
            "https://github.com/CH3NGYZ/demo",
            "https://github.com/CH3NGYZ/demo/releases",
            "https://example.com/?next=/releases/download/v1/x.zip",
            "https://github.com/CH3NGYZ/demo/blob/main/README.md",
            "ftp://github.com/CH3NGYZ/demo/releases/download/v1/x.zip",
            "not a url",
        ] {
            assert_eq!(classify_remote_url(url), RemoteUrlKind::Other, "{url}");
        }
    }

    #[test]
    fn remote_provider_comes_from_the_url_host() {
        assert_eq!(
            provider_for_remote_url("https://gitee.com/ch3ngyz/demo/raw/main/a.zip"),
            Provider::Gitee
        );
        assert_eq!(
            provider_for_remote_url("https://raw.githubusercontent.com/o/r/main/a.zip"),
            Provider::Github
        );
        assert_eq!(
            provider_for_remote_url("https://example.com/a.zip"),
            Provider::Github
        );
    }

    #[tokio::test]
    async fn direct_download_uses_ranges_and_preserves_bytes() {
        use std::io::{BufRead, BufReader, Write};
        use std::sync::{Arc, Mutex};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/mod.dll", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let recorded = ranges.clone();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let server_done = done.clone();
        let truncated = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let server_truncated = truncated.clone();
        listener.set_nonblocking(true).unwrap();
        const SIZE: usize = 9 * 1024 * 1024;
        // Serve the real HEAD, range probe, and three file segments, including
        // a truncated response that exercises the downloader's retry policy.
        let server = std::thread::spawn(move || {
            let mut workers = Vec::new();
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while !server_done.load(std::sync::atomic::Ordering::Acquire)
                && std::time::Instant::now() < deadline
            {
                let (mut socket, _) = match listener.accept() {
                    Ok(socket) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                let recorded = recorded.clone();
                let truncated = server_truncated.clone();
                workers.push(std::thread::spawn(move || {
                    // Read the request in blocking mode even on Windows.
                    socket.set_nonblocking(false).unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
                    let mut reader = BufReader::new(socket.try_clone().unwrap());
                    let mut request = String::new();
                    reader.read_line(&mut request).unwrap();
                    let head = request.starts_with("HEAD ");
                    let mut range = None;
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        if line == "\r\n" || line.is_empty() { break; }
                        if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                            let (start, end) = value.trim().split_once('-').unwrap();
                            range = Some((start.parse::<usize>().unwrap(), end.parse::<usize>().unwrap()));
                        }
                    }
                    let (start, end) = range.unwrap_or((0, SIZE - 1));
                    let count = end - start + 1;
                    let status = if range.is_some() { "206 Partial Content" } else { "200 OK" };
                    let content_range = if range.is_some() { format!("Content-Range: bytes {start}-{end}/{SIZE}\r\n") } else { String::new() };
                    write!(socket, "HTTP/1.1 {status}\r\nContent-Length: {count}\r\n{content_range}Connection: close\r\n\r\n").unwrap();
                    if !head {
                        recorded.lock().unwrap().push((start, end));
                        let bytes: Vec<u8> = (start..=end).map(|index| (index % 251) as u8).collect();
                        if start == 4 * 1024 * 1024
                            && !truncated.swap(true, std::sync::atomic::Ordering::AcqRel)
                        {
                            socket.write_all(&bytes[..bytes.len() / 2]).unwrap();
                        } else {
                            socket.write_all(&bytes).unwrap();
                        }
                    }
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        let cache = std::env::temp_dir().join(format!("direct-download-{}", uuid::Uuid::new_v4()));
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let reporter = archive::DownloadReporter::new("direct test");
        let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
            &client,
            &url,
            &cache,
            "direct test",
            &reporter,
            32,
            Some(Duration::from_secs(5)),
        )
        .await
        .unwrap();
        done.store(true, std::sync::atomic::Ordering::Release);
        server.join().unwrap();
        assert!(truncated.load(std::sync::atomic::Ordering::Acquire));
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.len(), SIZE);
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| *byte == (index % 251) as u8)
        );
        let mut actual = ranges.lock().unwrap().clone();
        // A valid range can be retried up to three times after a truncated body.
        // Verify the unique partition and the retry without assuming one request
        // per segment under parallel test load.
        let retried = (4 * 1024 * 1024, 8 * 1024 * 1024 - 1);
        assert!(actual.iter().filter(|range| **range == retried).count() >= 2);
        for range in &actual {
            assert!(actual.iter().filter(|other| *other == range).count() <= 3);
        }
        actual.sort_unstable();
        actual.dedup();
        assert_eq!(
            actual,
            vec![
                (0, 0),
                (0, 4 * 1024 * 1024 - 1),
                (4 * 1024 * 1024, 8 * 1024 * 1024 - 1),
                (8 * 1024 * 1024, SIZE - 1)
            ]
        );
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn direct_filename_validation() {
        let url = Url::parse("https://example.com/My%20Mod.DLL?signature=hidden").unwrap();
        assert_eq!(download_filename(&url, None).unwrap(), "My Mod.DLL");
        assert_eq!(supported_filename("A&B.dll").unwrap(), "A&B.dll");
        assert_eq!(
            download_filename(&url, Some("attachment; filename*=UTF-8''Bundle%20Pack.zip"))
                .unwrap(),
            "Bundle Pack.zip"
        );
        assert!(download_filename(&url, Some("attachment; filename=page.html")).is_err());
        for name in [
            "../mod.zip",
            "x%2Fmod.dll",
            "x.exe",
            "x.dll:stream",
            "x.zip.",
        ] {
            assert!(supported_filename(name).is_none(), "{name}");
        }
    }

    #[test]
    fn downloaded_content_validation() {
        let path = std::env::temp_dir().join(format!("direct-mod-{}", uuid::Uuid::new_v4()));
        fs::write(&path, b"<html>not a mod</html>").unwrap();
        assert!(validate_mod_file(&path, "mod.dll").is_err());
        assert!(validate_mod_file(&path, "mod.zip").is_err());
        let mut pe = vec![0u8; 88];
        pe[..2].copy_from_slice(b"MZ");
        pe[60..64].copy_from_slice(&64u32.to_le_bytes());
        pe[64..68].copy_from_slice(b"PE\0\0");
        fs::write(&path, &pe).unwrap();
        assert!(validate_mod_file(&path, "mod.dll").is_err());
        pe[87] = 0x20;
        fs::write(&path, pe).unwrap();
        assert!(validate_mod_file(&path, "mod.dll").is_ok());
        fs::remove_file(path).unwrap();
    }
}
