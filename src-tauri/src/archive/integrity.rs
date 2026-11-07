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
        let expected = expected
            .strip_prefix("sha256:")
            .unwrap_or(expected)
            .trim()
            .to_ascii_lowercase();
        let actual = sha256_file(path)?;
        if expected != actual {
            log::error!("SHA-256 校验失败：文件={}", path.display());
            return Err(crate::locale_tag::with_params(
                "download.checksumMismatch",
                &[
                    ("expected", &expected.to_string()),
                    ("actual", &actual.to_string()),
                ],
            ));
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

pub(super) fn validate_download_size(actual: u64, expected: Option<u64>) -> Result<(), String> {
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
        let expected = expected
            .strip_prefix("sha256:")
            .unwrap_or(expected)
            .trim()
            .to_ascii_lowercase();
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != expected {
            return Err("download.checksumFailed".into());
        }
    }
    Ok(())
}

/// Read every ZIP entry before changing the game directory. `zip` validates
/// each entry's compressed stream and CRC while it is read, so this catches a
/// truncated or otherwise damaged download before installation starts.
pub fn validate_zip(zip_path: &Path) -> Result<(), String> {
    log::debug!("开始校验 ZIP：文件={}", zip_path.display());
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &e.to_string())])
    })?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &e.to_string())],
            )
        })?;
        if !entry.is_dir() {
            io::copy(&mut entry, &mut io::sink()).map_err(|e| {
                crate::locale_tag::with_params(
                    "archive.zipValidationFailed",
                    &[("reason", &e.to_string())],
                )
            })?;
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
mod tests;
