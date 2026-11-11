//! Persistent rendered PNGs. Keys describe resource metadata and the renderer
//! preset; the frontend never supplies a directory or filename.
use image::ImageReader;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const MAX_PNG: u64 = 4 * 1024 * 1024;
const MAX_CACHE: u64 = 128 * 1024 * 1024;
static ACCESS: Mutex<()> = Mutex::new(());

pub(super) enum CacheDirectory {
    Thumbnails,
    Native,
    Assets,
}

pub(super) fn directory() -> Result<PathBuf, String> {
    fixed_directory(CacheDirectory::Thumbnails)
}

pub(super) fn fixed_directory(kind: CacheDirectory) -> Result<PathBuf, String> {
    let root = crate::user_paths::root()?;
    directory_under(&root, kind)
}

fn directory_under(root: &Path, kind: CacheDirectory) -> Result<PathBuf, String> {
    let leaf = match kind {
        CacheDirectory::Thumbnails => "diychef-thumbnails",
        CacheDirectory::Native => "diychef-native",
        CacheDirectory::Assets => "diychef-assets",
    };
    let directory = root.join("cache").join(leaf);
    // The system's data root may itself be redirected. Disposable child
    // directories may not be links/junctions, including broken links.
    for child in [root.join("cache"), directory.clone()] {
        match fs::symlink_metadata(&child) {
            Ok(info)
                if !info.is_dir() || info.file_type().is_symlink() || is_reparse_point(&info) =>
            {
                return Err("diychef.previewUnsafeResource".into());
            }
            Ok(_) => {
                if !child
                    .canonicalize()
                    .map_err(|_| "diychef.previewUnavailable")?
                    .starts_with(
                        root.canonicalize()
                            .map_err(|_| "diychef.previewUnavailable")?,
                    )
                {
                    return Err("diychef.previewUnsafeResource".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("diychef.previewUnavailable".into()),
        }
    }
    Ok(directory)
}

pub(super) fn lock_access() -> Result<MutexGuard<'static, ()>, String> {
    ACCESS
        .lock()
        .map_err(|_| "diychef.previewUnavailable".into())
}

pub(super) fn regular_file(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() || is_reparse_point(&info) => {
            Err("diychef.previewUnsafeResource".into())
        }
        Ok(info) if info.is_file() => Ok(Some(info)),
        Ok(_) => Err("diychef.previewUnsafeResource".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("diychef.previewUnavailable".into()),
    }
}

pub(super) fn write_atomic(
    directory: &Path,
    file: &Path,
    write: impl FnOnce(&mut fs::File) -> std::io::Result<()>,
) -> Result<(), String> {
    if file.parent() != Some(directory) {
        return Err("diychef.previewUnsafeResource".into());
    }
    fs::create_dir_all(directory).map_err(|_| "diychef.previewUnavailable")?;
    let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        write(&mut output)?;
        output.sync_all()?;
        fs::rename(&temporary, file)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("diychef.previewUnavailable".into());
    }
    Ok(())
}

pub(crate) fn access(
    key: &str,
    png: Option<&[u8]>,
    partial: bool,
    epoch: u64,
) -> Result<Vec<u8>, String> {
    access_at(&directory()?, key, png, partial, epoch)
}

pub(super) fn access_at(
    directory: &Path,
    key: &str,
    png: Option<&[u8]>,
    partial: bool,
    epoch: u64,
) -> Result<Vec<u8>, String> {
    let _guard = lock_access()?;
    super::preview::require_epoch(epoch)?;
    access_in(directory, key, png, partial)
}

pub(super) fn clear_in(directory: &Path) -> Result<usize, String> {
    let _guard = lock_access()?;
    let info = match fs::symlink_metadata(directory) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err("diychef.previewUnavailable".into()),
    };
    if !info.is_dir() || info.file_type().is_symlink() || is_reparse_point(&info) {
        return Err("diychef.previewUnsafeResource".into());
    }
    let mut removed = 0;
    for entry in fs::read_dir(directory).map_err(|_| "diychef.previewUnavailable")? {
        let entry = entry.map_err(|_| "diychef.previewUnavailable")?;
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = path
            .extension()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let record = is_record(&path);
        let temporary = extension == "tmp" && uuid::Uuid::parse_str(stem).is_ok();
        let file_type = entry
            .file_type()
            .map_err(|_| "diychef.previewUnavailable")?;
        if (record || temporary) && file_type.is_file() {
            fs::remove_file(path).map_err(|_| "diychef.previewUnavailable")?;
            removed += usize::from(record);
        }
    }
    Ok(removed)
}

fn is_record(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("bin")
        && path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| {
                stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
}

fn is_reparse_point(info: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return info.file_attributes() & 0x400 != 0;
    }
    #[cfg(not(windows))]
    {
        let _ = info;
        false
    }
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
    let existing = regular_file(&file)?;
    if let Some(bytes) = png {
        validate_png(bytes)?;
        fs::create_dir_all(directory).map_err(|_| "diychef.previewUnavailable")?;
        if existing.is_some() {
            return Ok(vec![]);
        }
        write_atomic(directory, &file, |output| {
            output.write_all(&[u8::from(partial)])?;
            output.write_all(bytes)
        })?;
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

pub(super) fn prune(directory: &Path, keep: &Path, budget: u64) -> Result<(), String> {
    let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = vec![];
    for entry in fs::read_dir(directory).map_err(|_| "diychef.previewUnavailable")? {
        let entry = entry.map_err(|_| "diychef.previewUnavailable")?;
        let path = entry.path();
        if !is_record(&path) {
            continue;
        }
        if let Ok(Some(info)) = regular_file(&path) {
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
