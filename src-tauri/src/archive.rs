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
        log::info!("下载策略：尝试分片，线程数={}，任务={}", threads, label);
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
            log::info!("分片下载文件大小：{} 字节，任务={}", total, label);
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
                log::info!("服务器支持 Range，开始分片下载，任务={}", label);
                match download_ranges(client, url, &target, label, app, total, threads).await {
                    Ok(()) => return Ok(target),
                    Err(error) if DOWNLOAD_CANCELLED.load(Ordering::Acquire) => {
                        let _ = fs::remove_file(&target);
                        return Err(error);
                    }
                    Err(error) => {
                        log::warn!("分片下载失败，回退单线程：任务={}，原因={}", label, error);
                        let _ = fs::remove_file(&target);
                    }
                }
            } else {
                log::warn!("服务器不支持 Range，回退单线程：任务={}", label);
            }
        } else {
            log::warn!("无法获取可用于分片的文件长度，回退单线程：任务={}", label);
        }
    } else {
        log::info!("下载策略：单线程直连流式下载，任务={}", label);
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
    for _attempt in 0..5 {
        if DOWNLOAD_CANCELLED.load(Ordering::Acquire) { return Err("下载已取消".into()); }
        let attempt = _attempt + 1;
        log::info!("单线程下载请求：任务={}，尝试={}", label, attempt);
        let response = client
            .get(url)
            .header("User-Agent", crate::releases::APP_USER_AGENT)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(|e| e.to_string());
        let response = match response {
            Ok(response) if response.status().is_success() => {
                log::info!("下载响应：任务={}，HTTP={}，长度={:?}", label, response.status(), response.content_length());
                response
            }
            Ok(response) => {
                last_error = format!("下载返回 HTTP {}", response.status());
                log::warn!("下载响应失败：任务={}，{}", label, last_error);
                continue;
            }
            Err(error) => {
                last_error = error;
                log::warn!("下载请求失败：任务={}，尝试={}，原因={}", label, attempt, last_error);
                continue;
            }
        };
        let total = response.content_length();
        let mut file = File::create(target).map_err(|e| e.to_string())?;
        let mut body = response.bytes_stream();
        let mut downloaded = 0_u64;
        let started = std::time::Instant::now();
        let mut failed = None;
        while let Some(chunk) = body.next().await {
            if DOWNLOAD_CANCELLED.load(Ordering::Acquire) { failed = Some("下载已取消".to_string()); break; }
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    failed = Some(error.to_string());
                    log::warn!("下载响应体读取失败：任务={}，尝试={}，已收到={}，原因={}", label, attempt, downloaded, error);
                    break;
                }
            };
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;
            emit_progress(app, label, downloaded, total, started.elapsed(), false);
        }
        if failed.is_none() && total.is_some_and(|expected| expected != downloaded) {
            failed = Some(format!("下载长度不完整: 收到 {downloaded} 字节，预期 {} 字节", total.unwrap_or_default()));
        }
        if let Some(error) = failed {
            if error == "下载已取消" { return Err(error); }
            last_error = error;
            log::warn!("单线程下载本次未完成：任务={}，尝试={}，原因={}", label, attempt, last_error);
            continue;
        }
        file.flush().map_err(|e| e.to_string())?;
        emit_progress(app, label, downloaded, total, started.elapsed(), true);
        return Ok(());
    }
    let _ = fs::remove_file(target);
    Err(format!("下载重试 5 次后失败: {last_error}"))
}

async fn read_response_bytes(response: reqwest::Response, expected: u64) -> Result<Vec<u8>, String> {
    let mut body = response.bytes_stream();
    let mut bytes = Vec::with_capacity(expected.min(16 * 1024 * 1024) as usize);
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > expected {
            return Err(format!("分段长度超出预期: 收到 {} 字节", bytes.len()));
        }
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
                        match read_response_bytes(response, end - start + 1).await {
                            Ok(bytes) => return Ok((start, bytes)),
                            Err(error) => {
                                last_error = error.to_string();
                                log::warn!("分片响应体读取失败：范围={}-{}，原因={}", start, end, last_error);
                            }
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
    if let Some(app) = app {
        let _ = app.emit("download-progress", DownloadProgress {
            label: label.into(), downloaded, total,
            speed_bytes, finished,
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
    extract_zip_into(zip_path, game_root, "", false)
}

pub fn extract_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    let layout = inspect_mod_zip(zip_path, archive_name)?;
    let (extract_target, replace_target) = match layout {
        ModZipLayout::SingleDirectory(name) => {
            (PathBuf::from(target_dir), Path::new(target_dir).join(name))
        }
        ModZipLayout::SingleDll(name) => {
            (PathBuf::from(target_dir), Path::new(target_dir).join(name))
        }
        ModZipLayout::Wrapped(name) => {
            let path = Path::new(target_dir).join(name);
            (path.clone(), path)
        }
    };
    let replace_target = safe_game_path(game_root, &replace_target.to_string_lossy())?;
    let snapshot = snapshot_path(&replace_target)?;
    remove_existing_path(&replace_target)?;
    let result = extract_zip_into(
        zip_path,
        game_root,
        &extract_target.to_string_lossy().replace('\\', "/"),
        true,
    );
    match result {
        Ok(files) => {
            discard_snapshot(snapshot);
            Ok(files)
        }
        Err(error) => {
            let restore_error = restore_snapshot(snapshot, &replace_target).err();
            match restore_error {
                Some(restore_error) => Err(format!(
                    "解压失败: {error}；恢复旧文件失败: {restore_error}"
                )),
                None => Err(error),
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ModZipLayout {
    SingleDirectory(String),
    SingleDll(String),
    Wrapped(String),
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

fn inspect_mod_zip(zip_path: &Path, archive_name: &str) -> Result<ModZipLayout, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP 读取失败: {e}"))?;
    let mut top_level = HashSet::new();
    let mut root_files = Vec::new();
    let mut root_file_count = 0;
    let mut root_dll_count = 0;
    let mut only_root_is_directory = true;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("ZIP 条目读取失败: {e}"))?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| format!("拒绝不安全 ZIP 路径: {}", entry.name()))?;
        let components = path.components().collect::<Vec<_>>();
        let Some(root) = components.first() else {
            continue;
        };
        top_level.insert(root.as_os_str().to_string_lossy().into_owned());
        let nested = components.len() > 1;
        if !nested && !entry.is_dir() {
            root_file_count += 1;
            root_files.push(path.to_string_lossy().into_owned());
            only_root_is_directory = false;
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("dll"))
            {
                root_dll_count += 1;
            }
        }
    }

    let one_directory = top_level.len() == 1 && only_root_is_directory;
    let one_dll = top_level.len() == 1 && root_file_count == 1 && root_dll_count == 1;
    if one_directory {
        let name = top_level
            .into_iter()
            .next()
            .ok_or("ZIP 文件夹结构无效")?;
        Ok(ModZipLayout::SingleDirectory(name))
    } else if one_dll {
        Ok(ModZipLayout::SingleDll(
            root_files
                .into_iter()
                .next()
                .ok_or("ZIP DLL 结构无效")?,
        ))
    } else {
        Ok(ModZipLayout::Wrapped(safe_zip_stem(archive_name)?.into()))
    }
}

fn snapshot_path(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let snapshot = std::env::temp_dir().join(format!("oc2-mod-snapshot-{}", Uuid::new_v4()));
    copy_path(path, &snapshot)?;
    Ok(Some(snapshot))
}

fn restore_snapshot(snapshot: Option<PathBuf>, target: &Path) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    remove_existing_path(target)?;
    copy_path(&snapshot, target)?;
    remove_existing_path(&snapshot)
}

fn discard_snapshot(snapshot: Option<PathBuf>) {
    if let Some(snapshot) = snapshot {
        if let Err(error) = remove_existing_path(&snapshot) {
            log::warn!("清理 MOD 临时快照失败：路径={}，原因={}", snapshot.display(), error);
        }
    }
}

fn remove_existing_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    clear_readonly(path)?;
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
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

fn extract_zip_into(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    reject_existing: bool,
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
            if reject_existing && destination.exists() {
                return Err(format!("目标文件已存在: {}", destination.display()));
            }
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
    let snapshot = snapshot_path(&destination)?;
    remove_existing_path(&destination)?;
    if let Err(error) = fs::copy(asset_path, &destination) {
        let restore_error = restore_snapshot(snapshot, &destination).err();
        return Err(match restore_error {
            Some(restore_error) => format!(
                "复制 MOD 文件失败: {error}；恢复旧文件失败: {restore_error}"
            ),
            None => error.to_string(),
        });
    }
    discard_snapshot(snapshot);
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

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, contents) in entries {
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
    fn mod_zip_layouts_use_one_shared_install_policy() {
        let root = std::env::temp_dir().join(format!("oc2-zip-layout-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        let folder_zip = root.join("folder.zip");
        write_zip(&folder_zip, &[("Plugin/Plugin.dll", b"plugin")]);
        fs::create_dir_all(game.join("BepInEx/plugins/Plugin")).unwrap();
        fs::write(game.join("BepInEx/plugins/Plugin/old.txt"), b"old").unwrap();
        extract_mod_zip(&folder_zip, &game, "BepInEx/plugins", "folder.zip").unwrap();
        assert!(game.join("BepInEx/plugins/Plugin/Plugin.dll").is_file());
        assert!(!game.join("BepInEx/plugins/Plugin/old.txt").exists());
        assert!(!game.join("BepInEx/plugins/folder/Plugin/Plugin.dll").exists());

        let dll_zip = root.join("single.dll.zip");
        write_zip(&dll_zip, &[("Single.dll", b"plugin")]);
        fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        fs::write(game.join("BepInEx/plugins/Single.dll"), b"old").unwrap();
        extract_mod_zip(&dll_zip, &game, "BepInEx/plugins", "single.dll.zip").unwrap();
        assert!(game.join("BepInEx/plugins/Single.dll").is_file());
        assert_eq!(fs::read(game.join("BepInEx/plugins/Single.dll")).unwrap(), b"plugin");

        let mixed_zip = root.join("mixed.zip");
        write_zip(
            &mixed_zip,
            &[("Plugin/Plugin.dll", b"plugin"), ("Loose.dll", b"loose")],
        );
        fs::create_dir_all(game.join("BepInEx/plugins/mixed")).unwrap();
        fs::write(game.join("BepInEx/plugins/mixed/old.txt"), b"old").unwrap();
        extract_mod_zip(&mixed_zip, &game, "BepInEx/plugins", "mixed.zip").unwrap();
        assert!(game.join("BepInEx/plugins/mixed/Plugin/Plugin.dll").is_file());
        assert!(game.join("BepInEx/plugins/mixed/Loose.dll").is_file());
        assert!(!game.join("BepInEx/plugins/mixed/old.txt").exists());

        fs::remove_dir_all(root).unwrap();
    }
}
