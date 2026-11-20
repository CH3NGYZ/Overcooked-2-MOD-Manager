use super::{LogKind, unavailable};
use chrono::{NaiveDateTime, Utc};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub(super) fn plain(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink()
}

pub(super) fn ensure_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if plain(&metadata) && metadata.is_dir() {
        Ok(())
    } else {
        Err(unavailable())
    }
}

pub(super) fn open_log(path: &Path, writable: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    open_plain_file(path, options)
}

pub(super) fn open_plain_file(path: &Path, mut options: OpenOptions) -> io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !plain(&metadata) || !metadata.is_file() {
        return Err(unavailable());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use winapi::um::winbase::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Darwin <sys/fcntl.h>: do not follow a replaced final path component.
        options.custom_flags(0x0100);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !plain(&metadata)
        || !metadata.is_file()
        || !crate::mod_changes::transaction::has_single_link(&file)
    {
        return Err(unavailable());
    }
    Ok(file)
}

pub(super) fn create_log(directory: &Path, kind: LogKind) -> io::Result<(PathBuf, File)> {
    ensure_directory(directory)?;
    let path = directory.join(format!(
        "{}-{}-{}.log",
        kind.as_str(),
        Utc::now().format("%Y-%m-%d_%H-%M-%S-%9f"),
        uuid::Uuid::new_v4().simple(),
    ));
    let file = OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(&path)?;
    Ok((path, file))
}

fn history_date(name: &str, kind: LogKind, metadata: &Metadata) -> Option<NaiveDateTime> {
    let stem = name.strip_prefix(kind.as_str())?;
    if stem == ".log" {
        // Older versions wrote to a fixed filename until their next rotation.
        return metadata
            .modified()
            .ok()
            .map(|time| chrono::DateTime::<Utc>::from(time).naive_utc());
    }
    if kind == LogKind::Rust && stem.starts_with('_') {
        let date = stem[1..]
            .strip_suffix(".log")
            .or_else(|| stem[1..].strip_suffix(".log.bak"))?;
        return NaiveDateTime::parse_from_str(date, "%Y-%m-%d_%H-%M-%S").ok();
    }
    let (date, nonce) = stem
        .strip_prefix('-')?
        .strip_suffix(".log")?
        .rsplit_once('-')?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    NaiveDateTime::parse_from_str(date, "%Y-%m-%d_%H-%M-%S-%9f").ok()
}

pub(super) fn sorted_logs(
    directory: &Path,
    kind: LogKind,
    active: &Path,
) -> io::Result<Vec<PathBuf>> {
    ensure_directory(directory)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(kind.as_str()) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if plain(&metadata)
            && metadata.is_file()
            && let Some(date) = history_date(name, kind, &metadata)
            && open_log(&entry.path(), false).is_ok()
        {
            let path = entry.path();
            files.push((path == active, date, path));
        }
    }
    files.sort_unstable_by(|left, right| right.cmp(left));
    Ok(files.into_iter().map(|(_, _, path)| path).collect())
}
