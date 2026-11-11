//! Metadata-only invalidation. Never reads resource contents to detect changes.
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub(crate) fn directory(path: &Path) -> Result<String, String> {
    let root = path
        .canonicalize()
        .map_err(|_| "diychef.previewUnavailable")?;
    let mut rows = vec![];
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).map_err(|_| "diychef.previewUnavailable")? {
            let entry = entry.map_err(|_| "diychef.previewUnavailable")?;
            let path = entry.path();
            let info =
                std::fs::symlink_metadata(&path).map_err(|_| "diychef.previewUnavailable")?;
            // Reparse points and links can otherwise escape the chef's resource boundary.
            if is_link(&info) {
                return Err("diychef.previewUnsafeResource".into());
            }
            if info.is_dir() {
                pending.push(path);
                continue;
            }
            if !info.is_file() {
                continue;
            }
            if rows.len() >= 16_384 {
                return Err("diychef.previewLimit".into());
            }
            let name = path
                .strip_prefix(&root)
                .map_err(|_| "diychef.previewUnsafeResource")?;
            rows.push((name.to_string_lossy().replace('\\', "/"), stamp(&info)?));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(format!("{}|{:?}", root.display(), rows))
}

pub(crate) fn files(paths: &[PathBuf]) -> Result<String, String> {
    let mut rows = vec![];
    for path in paths {
        let info = path.metadata().map_err(|_| "diychef.previewUnavailable")?;
        rows.push((path.to_string_lossy().into_owned(), stamp(&info)?));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(format!("{rows:?}"))
}

fn stamp(info: &std::fs::Metadata) -> Result<(u64, u128), String> {
    let modified = info
        .modified()
        .map_err(|_| "diychef.previewUnavailable")?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "diychef.previewUnavailable")?;
    Ok((info.len(), modified.as_nanos()))
}
fn is_link(info: &std::fs::Metadata) -> bool {
    if info.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if info.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_added_renamed_and_nested_files_without_reading_contents() {
        let dir = std::env::temp_dir().join(format!("chef-stamp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("Head.obj"), "abc").unwrap();
        let first = directory(&dir).unwrap();
        assert_eq!(first, directory(&dir).unwrap());
        std::fs::write(dir.join("nested/extra.bin"), "x").unwrap();
        let extra = directory(&dir).unwrap();
        assert_ne!(first, extra);
        std::fs::rename(dir.join("Head.obj"), dir.join("Head1.obj")).unwrap();
        assert_ne!(extra, directory(&dir).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn size_mtime_and_deletion_invalidate_but_metadata_restoration_keeps_identity() {
        let dir =
            std::env::temp_dir().join(format!("chef-stamp-boundaries-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Head.obj");
        std::fs::write(&path, b"abc").unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let modified = file.metadata().unwrap().modified().unwrap();
        let before = directory(&dir).unwrap();
        file.set_len(4).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let resized = directory(&dir).unwrap();
        assert_ne!(before, resized, "size alone invalidates");
        file.set_len(3).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(
            before,
            directory(&dir).unwrap(),
            "same metadata identity is restored"
        );
        file.set_times(
            std::fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(60)),
        )
        .unwrap();
        let retimed = directory(&dir).unwrap();
        assert_ne!(before, retimed, "mtime alone invalidates");
        drop(file);
        std::fs::remove_file(&path).unwrap();
        assert_ne!(retimed, directory(&dir).unwrap(), "deletion invalidates");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
