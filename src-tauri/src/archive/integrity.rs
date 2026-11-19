use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use zip::ZipArchive;

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
        let actual = sha256_file(path)?;
        validate_download_digest(&actual, expected)?;
        log::info!("文件摘要校验通过：文件={}", path.display());
    } else {
        log::trace!("未提供文件摘要，跳过校验：文件={}", path.display());
    }
    Ok(())
}

/// SHA-256 is the only digest we can verify for Release and update assets.
/// Accept the tagged form from hosting APIs as well as the legacy bare hex.
pub(crate) fn normalize_sha256(digest: &str) -> Option<&str> {
    let digest = digest.trim();
    let hex = match digest.split_once(':') {
        Some((algorithm, hex)) if algorithm.eq_ignore_ascii_case("sha256") => hex.trim(),
        Some(_) => return None,
        None => digest,
    };
    (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hex)
}

/// Compare a SHA-256 already computed while reading a file with Release metadata.
pub(crate) fn validate_download_digest(actual: &str, expected: &str) -> Result<(), String> {
    let Some(expected) = normalize_sha256(expected) else {
        return Err("download.checksumFailed".into());
    };
    if actual.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    log::error!("下载文件 SHA-256 校验失败");
    Err(crate::locale_tag::with_params(
        "download.checksumMismatch",
        &[
            ("expected", &expected.to_ascii_lowercase()),
            ("actual", actual),
        ],
    ))
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

pub(crate) fn validate_download_size(actual: u64, expected: Option<u64>) -> Result<(), String> {
    if actual == 0 {
        return Err("download.empty".into());
    }
    if expected.is_some_and(|size| size != actual) {
        log::warn!(
            "下载长度不完整: 收到 {actual} 字节，预期 {} 字节",
            expected.unwrap()
        );
        return Err("download.incomplete".into());
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
        let actual = format!("{:x}", Sha256::digest(bytes));
        validate_download_digest(&actual, expected).map_err(|_| "download.checksumFailed")?;
    }
    Ok(())
}

/// Read every ZIP entry before changing the game directory. `zip` validates
/// each entry's compressed stream and CRC while it is read, so this catches a
/// truncated or otherwise damaged download before installation starts.
pub fn validate_zip(zip_path: &Path) -> Result<(), String> {
    validate_zip_with_cancel(zip_path, || false)
}

pub(super) fn validate_zip_with_cancel(
    zip_path: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<(), String> {
    let check_cancelled = || {
        if cancelled() {
            Err("download.cancelled".to_string())
        } else {
            Ok(())
        }
    };
    check_cancelled()?;
    log::debug!("开始校验 ZIP：文件={}", zip_path.display());
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &e.to_string())])
    })?;
    let mut buffer = [0u8; 64 * 1024];
    for index in 0..archive.len() {
        check_cancelled()?;
        let mut entry = archive.by_index(index).map_err(|e| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &e.to_string())],
            )
        })?;
        if !entry.is_dir() {
            // Read through EOF to check CRC; an entry may itself be several GB.
            loop {
                check_cancelled()?;
                match entry.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(_) => (),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                    Err(error) => {
                        return Err(crate::locale_tag::with_params(
                            "archive.zipValidationFailed",
                            &[("reason", &error.to_string())],
                        ));
                    }
                }
            }
        }
    }
    check_cancelled()?;
    log::info!(
        "ZIP 内容校验通过：文件={}，条目数={}",
        zip_path.display(),
        archive.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests;
