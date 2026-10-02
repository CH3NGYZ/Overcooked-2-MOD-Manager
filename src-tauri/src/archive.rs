use crate::game::safe_game_path;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use zip::ZipArchive;

static DOWNLOAD_CANCELLED: AtomicBool = AtomicBool::new(false);

pub fn cancel_download() {
    DOWNLOAD_CANCELLED.store(true, Ordering::Release);
}

pub fn start_download() {
    DOWNLOAD_CANCELLED.store(false, Ordering::Release);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub label: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes: u64,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallManifest {
    pub repository_id: String,
    pub release_tag: String,
    pub installed_at: String,
    pub files: Vec<String>,
}

pub async fn download_to_temp(
    client: &Client,
    url: &str,
    temp_dir: &Path,
    label: &str,
    app: Option<&AppHandle>,
) -> Result<PathBuf, String> {
    start_download();
    download_to_temp_with_threads(client, url, temp_dir, label, app, 1).await
}

pub async fn download_to_temp_with_threads(
    client: &Client,
    url: &str,
    temp_dir: &Path,
    label: &str,
    app: Option<&AppHandle>,
    threads: u32,
) -> Result<PathBuf, String> {
    fs::create_dir_all(temp_dir).map_err(|e| e.to_string())?;
    let target = temp_dir.join(format!("{}.download", Uuid::new_v4()));
    if threads > 1 {
        if let Some(total) = client
            .head(url)
            .header("User-Agent", crate::releases::APP_USER_AGENT)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await
            .ok()
            .filter(|response| response.status().is_success())
            .and_then(|response| response.content_length())
            .filter(|total| *total > 1024 * 1024)
        {
            let range_supported = client
                .get(url)
                .header("User-Agent", crate::releases::APP_USER_AGENT)
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .header(reqwest::header::RANGE, "bytes=0-0")
                .send()
                .await
                .ok()
                .is_some_and(|response| response.status() == reqwest::StatusCode::PARTIAL_CONTENT);
            if range_supported {
                match download_ranges(client, url, &target, label, app, total, threads).await {
                    Ok(()) => return Ok(target),
                    Err(error) if DOWNLOAD_CANCELLED.load(Ordering::Acquire) => {
                        let _ = fs::remove_file(&target);
                        return Err(error);
                    }
                    Err(_) => { let _ = fs::remove_file(&target); }
                }
            }
        }
    }
    download_stream(client, url, &target, label, app).await?;
    Ok(target)
}

async fn download_stream(
    client: &Client,
    url: &str,
    target: &Path,
    label: &str,
    app: Option<&AppHandle>,
) -> Result<(), String> {
    let mut last_error = String::new();
    for _attempt in 0..3 {
        if DOWNLOAD_CANCELLED.load(Ordering::Acquire) { return Err("下载已取消".into()); }
        let response = client
        .get(url)
        .header("User-Agent", crate::releases::APP_USER_AGENT)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|e| e.to_string());
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => { last_error = format!("下载返回 HTTP {}", response.status()); continue; }
            Err(error) => { last_error = error; continue; }
        };
        let total = response.content_length();
        let mut file = File::create(target).map_err(|e| e.to_string())?;
        let mut body = response.bytes_stream();
        let mut downloaded = 0_u64;
        let started = std::time::Instant::now();
        let mut failed = None;
        while let Some(chunk) = body.next().await {
            if DOWNLOAD_CANCELLED.load(Ordering::Acquire) { failed = Some("下载已取消".to_string()); break; }
            let chunk = match chunk { Ok(chunk) => chunk, Err(error) => { failed = Some(error.to_string()); break; } };
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;
            emit_progress(app, label, downloaded, total, started.elapsed(), false);
        }
        if failed.is_none() && total.is_some_and(|expected| expected != downloaded) {
            failed = Some(format!("下载长度不完整: 收到 {downloaded} 字节，预期 {} 字节", total.unwrap_or_default()));
        }
        if let Some(error) = failed {
            let _ = fs::remove_file(target);
            if error == "下载已取消" { return Err(error); }
            last_error = error;
            continue;
        }
        file.flush().map_err(|e| e.to_string())?;
        emit_progress(app, label, downloaded, total, started.elapsed(), true);
        return Ok(());
    }
    let _ = fs::remove_file(target);
    Err(format!("下载重试 3 次后失败: {last_error}"))
}

async fn download_ranges(
    client: &Client,
    url: &str,
    target: &Path,
    label: &str,
    app: Option<&AppHandle>,
    total: u64,
    threads: u32,
) -> Result<(), String> {
    const PART_SIZE: u64 = 4 * 1024 * 1024;
    let ranges = (0..total.div_ceil(PART_SIZE))
        .map(|part| { let start = part * PART_SIZE; (start, (start + PART_SIZE - 1).min(total - 1)) })
        .collect::<Vec<_>>();
    let mut output = File::create(target).map_err(|error| error.to_string())?;
    output.set_len(total).map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let mut downloaded = 0_u64;
    let mut pending = stream::iter(ranges.into_iter().map(|(start, end)| {
        let client = client.clone();
        let url = url.to_string();
        async move {
            let mut last_error = String::new();
            for _ in 0..3 {
                if DOWNLOAD_CANCELLED.load(Ordering::Acquire) { return Err("下载已取消".to_string()); }
                let response = client.get(&url)
                    .header("User-Agent", crate::releases::APP_USER_AGENT)
                    .header(reqwest::header::ACCEPT_ENCODING, "identity")
                    .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
                    .send().await;
                match response {
                    Ok(response) if response.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                        let expected_range = format!("bytes {start}-{end}/{total}");
                        let actual_range = response.headers().get(reqwest::header::CONTENT_RANGE)
                            .and_then(|value| value.to_str().ok());
                        if actual_range != Some(expected_range.as_str()) {
                            last_error = "服务器返回了错误的分段范围".into();
                            continue;
                        }
                        match response.bytes().await {
                            Ok(bytes) if bytes.len() as u64 == end - start + 1 => return Ok((start, bytes)),
                            Ok(bytes) => last_error = format!("分段长度错误: 收到 {} 字节", bytes.len()),
                            Err(error) => last_error = error.to_string(),
                        }
                    }
                    Ok(response) => last_error = format!("服务器不支持分段下载 (HTTP {})", response.status()),
                    Err(error) => last_error = error.to_string(),
                }
            }
            Err(last_error)
        }
    })).buffer_unordered(threads.clamp(1, 32) as usize);
    while let Some(result) = pending.next().await {
        if DOWNLOAD_CANCELLED.load(Ordering::Acquire) { return Err("下载已取消".into()); }
        let (offset, bytes) = result?;
        output.seek(SeekFrom::Start(offset)).map_err(|error| error.to_string())?;
        output.write_all(&bytes).map_err(|error| error.to_string())?;
        downloaded += bytes.len() as u64;
        emit_progress(app, label, downloaded, Some(total), started.elapsed(), false);
    }
    output.flush().map_err(|error| error.to_string())?;
    emit_progress(app, label, total, Some(total), started.elapsed(), true);
    Ok(())
}

fn emit_progress(app: Option<&AppHandle>, label: &str, downloaded: u64, total: Option<u64>, elapsed: std::time::Duration, finished: bool) {
    if let Some(app) = app {
        let _ = app.emit("download-progress", DownloadProgress {
            label: label.into(), downloaded, total,
            speed_bytes: downloaded / elapsed.as_secs().max(1), finished,
        });
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_digest(path: &Path, digest: Option<&str>) -> Result<(), String> {
    if let Some(expected) = digest {
        let expected = expected
            .strip_prefix("sha256:")
            .unwrap_or(expected)
            .trim()
            .to_ascii_lowercase();
        let actual = sha256_file(path)?;
        if expected != actual {
            return Err(format!("SHA-256 校验失败，期望 {expected}，实际 {actual}"));
        }
    }
    Ok(())
}

pub fn extract_zip_safe(zip_path: &Path, game_root: &Path) -> Result<Vec<String>, String> {
    extract_zip_safe_under(zip_path, game_root, "")
}

pub fn extract_zip_safe_under(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
) -> Result<Vec<String>, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    let mut created = Vec::new();
    let extraction = (|| -> Result<(), String> {
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
            let name = entry.name().replace('\\', "/");
            if name.is_empty() || name.ends_with('/') {
                continue;
            }
            let enclosed = entry
                .enclosed_name()
                .ok_or_else(|| format!("拒绝不安全 ZIP 路径: {name}"))?;
            let relative = Path::new(target_dir).join(enclosed);
            let destination = safe_game_path(game_root, &relative.to_string_lossy())?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            clear_readonly(&destination)?;
            let mut output = File::create(&destination).map_err(|e| e.to_string())?;
            io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
            created.push(path_relative(game_root, &destination));
        }
        Ok(())
    })();
    if let Err(error) = extraction {
        for relative in &created {
            if let Ok(path) = safe_game_path(game_root, relative) {
                let _ = clear_readonly(&path);
                let _ = fs::remove_file(path);
            }
        }
        return Err(error);
    }
    Ok(created)
}

pub fn copy_asset(
    asset_path: &Path,
    game_root: &Path,
    install_target: &str,
    file_name: &str,
) -> Result<Vec<String>, String> {
    let relative = Path::new(install_target).join(file_name);
    let destination = safe_game_path(game_root, &relative.to_string_lossy())?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    clear_readonly(&destination)?;
    if let Err(error) = fs::copy(asset_path, &destination) {
        let _ = clear_readonly(&destination);
        let _ = fs::remove_file(&destination);
        return Err(error.to_string());
    }
    Ok(vec![path_relative(game_root, &destination)])
}

pub fn write_manifest(manifest_dir: &Path, manifest: &InstallManifest) -> Result<(), String> {
    fs::create_dir_all(manifest_dir).map_err(|e| e.to_string())?;
    let path = manifest_dir.join(format!("{}.toml", manifest.repository_id));
    let text = toml::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let temp = manifest_dir.join(format!("{}.{}.tmp", manifest.repository_id, Uuid::new_v4()));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

pub fn read_manifest(manifest_dir: &Path, repository_id: &str) -> Option<InstallManifest> {
    let path = manifest_dir.join(format!("{}.toml", repository_id));
    let text = fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

pub fn remove_manifest_files(
    game_root: &Path,
    manifest: &InstallManifest,
) -> Result<usize, String> {
    let mut removed = 0;
    for relative in &manifest.files {
        let path = safe_game_path(game_root, relative)?;
        if path.is_file() {
            clear_readonly(&path)?;
            fs::remove_file(path).map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn clear_readonly(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
    {
        let metadata = fs::metadata(entry.path()).map_err(|e| e.to_string())?;
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            fs::set_permissions(entry.path(), permissions).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn path_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn new_manifest(repository_id: &str, release_tag: &str, files: Vec<String>) -> InstallManifest {
    InstallManifest {
        repository_id: repository_id.into(),
        release_tag: release_tag.into(),
        installed_at: Utc::now().to_rfc3339(),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
}
