use crate::archive::{self, DownloadRoute};
use crate::commands::{AppState, install_manual_mod};
use crate::download::{
    DownloadCandidate, DownloadContext, candidate_progress, try_download_candidates,
};
use crate::{game, releases};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, State};
use url::Url;

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
                            .ok_or_else(|| "只支持 DLL 或 ZIP 文件".into());
                    }
                }
            }
        }
    }
    url.path_segments()
        .and_then(|mut parts| parts.next_back())
        .and_then(supported_filename)
        .ok_or_else(|| "只支持 DLL 或 ZIP 文件".into())
}

fn validate_mod_file(path: &Path, filename: &str) -> Result<(), String> {
    archive::validate_download_file(path, None, None)?;
    if filename.to_ascii_lowercase().ends_with(".zip") {
        return archive::validate_zip(path);
    }
    // Reject HTML, renamed executables, and truncated files before installing a DLL.
    let invalid = || "下载的文件不是有效的 DLL".to_string();
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

#[tauri::command]
pub async fn install_online_mod(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<crate::models::OperationResult, String> {
    archive::start_download();
    let parsed = Url::parse(url.trim()).map_err(|_| "请输入有效的 HTTP 或 HTTPS 下载直链")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("请输入有效的 HTTP 或 HTTPS 下载直链".into());
    }
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    // One direct candidate; no provider detection, API credentials or proxy configuration.
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(config.network.timeout_seconds.max(1)))
        .read_timeout(Duration::from_secs(120))
        .user_agent(releases::APP_USER_AGENT)
        .build()
        .map_err(|_| "无法创建下载客户端")?;
    let candidate = DownloadCandidate {
        route: DownloadRoute::Direct,
        url: parsed.to_string(),
    };
    archive::emit_progress_event(Some(&app), candidate_progress("在线文件", &candidate, 1, 1));
    log::info!(
        "在线 MOD 直连下载：域名={}，线程数=32",
        parsed.host_str().unwrap_or_default()
    );
    let response = tokio::select! {
        result = client.get(parsed.clone()).timeout(Duration::from_secs(5)).header(reqwest::header::RANGE, "bytes=0-0")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::USER_AGENT, releases::APP_USER_AGENT).send() => {
            result.map_err(|_| "无法获取在线文件信息")?.error_for_status().map_err(|_| "无法获取在线文件信息")?
        }
        _ = async {
            while !archive::is_download_cancelled() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } => return Err("下载已取消".into()),
    };
    let disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok());
    let filename = download_filename(response.url(), disposition).or_else(|error| {
        if disposition.is_none() {
            download_filename(&parsed, None)
        } else {
            Err(error)
        }
    })?;
    drop(response);
    let cache_dir = state.config.cache_dir();
    let temp = try_download_candidates(
        vec![candidate],
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            let filename = &filename;
            async move {
                let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    "在线文件",
                    &reporter,
                    32,
                    Some(Duration::from_secs(5)),
                )
                .await?;
                if let Err(error) = validate_mod_file(&path, filename) {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                Ok(path)
            }
        },
        Some(DownloadContext {
            app: Some(&app),
            label: "在线文件",
            expected_size: None,
        }),
    )
    .await?;
    let staging = cache_dir.join(format!("manual-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        fs::create_dir(&staging).map_err(|_| "无法创建安装临时目录")?;
        let source = staging.join(&filename);
        fs::rename(&temp, &source).map_err(|_| "无法准备安装文件")?;
        if archive::is_download_cancelled() {
            return Err("下载已取消".into());
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
