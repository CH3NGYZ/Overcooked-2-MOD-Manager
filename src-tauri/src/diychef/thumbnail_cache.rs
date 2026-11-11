//! Persistent rendered PNGs. Keys describe resource metadata and the renderer
//! preset; the frontend never supplies a directory or filename.
use image::ImageReader;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_PNG: u64 = 4 * 1024 * 1024;
const MAX_CACHE: u64 = 128 * 1024 * 1024;
static ACCESS: Mutex<()> = Mutex::new(());

pub(crate) fn access(key: &str, png: Option<&[u8]>, partial: bool) -> Result<Vec<u8>, String> {
    let directory = crate::user_paths::root()?.join("cache/diychef-thumbnails");
    let _guard = ACCESS.lock().map_err(|_| "diychef.previewUnavailable")?;
    access_in(&directory, key, png, partial)
}

fn access_in(
    directory: &Path,
    key: &str,
    png: Option<&[u8]>,
    partial: bool,
) -> Result<Vec<u8>, String> {
    // Restrict even the unhashed logical key: no paths, user text or unbounded
    // IPC input can become an on-disk cache entry.
    if key.is_empty()
        || key.len() > 160
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
    {
        return Err("diychef.previewUnsafeResource".into());
    }
    let file = directory.join(format!("{:x}.bin", Sha256::digest(key.as_bytes())));
    if let Some(bytes) = png {
        validate_png(bytes)?;
        fs::create_dir_all(directory).map_err(|_| "diychef.previewUnavailable")?;
        if file.is_file() {
            return Ok(vec![]);
        }
        let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            output.write_all(&[u8::from(partial)])?;
            output.write_all(bytes)?;
            output.sync_all()?;
            fs::rename(&temporary, &file)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("diychef.previewUnavailable".into());
        }
        prune(directory, &file, MAX_CACHE)?;
        return Ok(vec![]);
    }
    let Ok(input) = fs::File::open(&file) else {
        return Ok(vec![]);
    };
    let mut bytes = vec![];
    input
        .take(MAX_PNG + 2)
        .read_to_end(&mut bytes)
        .map_err(|_| "diychef.previewUnavailable")?;
    if bytes.first().is_none_or(|state| *state > 1)
        || validate_png(bytes.get(1..).unwrap_or_default()).is_err()
    {
        // Corrupt cache is disposable and must not prevent a fresh render.
        let _ = fs::remove_file(&file);
        return Ok(vec![]);
    }
    let _ = OpenOptions::new()
        .write(true)
        .open(&file)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()));
    Ok(bytes)
}

fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_PNG || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("diychef.previewLimit".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(640);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| "diychef.previewUnavailable")?;
    if (decoded.width(), decoded.height()) != (512, 640) {
        return Err("diychef.previewLimit".into());
    }
    Ok(())
}

fn prune(directory: &Path, keep: &Path, budget: u64) -> Result<(), String> {
    let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = vec![];
    for entry in fs::read_dir(directory).map_err(|_| "diychef.previewUnavailable")? {
        let entry = entry.map_err(|_| "diychef.previewUnavailable")?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("bin") {
            continue;
        }
        let info = entry.metadata().map_err(|_| "diychef.previewUnavailable")?;
        if info.is_file() {
            entries.push((
                path,
                info.len(),
                info.modified().unwrap_or(std::time::UNIX_EPOCH),
            ));
        }
    }
    let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
    entries.sort_by_key(|entry| entry.2);
    for (path, size, _) in entries {
        if total <= budget {
            break;
        }
        if path != keep && fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "thumbnail_cache/tests.rs"]
mod tests;
