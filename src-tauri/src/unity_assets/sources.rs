//! Platform resource discovery shared by map and chef preview providers.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::Metadata,
    path::{Path, PathBuf},
};
/// The IPC already uses the shared normalized game root (including .app selection).
/// Enumerate bounded platform directories, so case-sensitive macOS layouts do not
/// depend on guessing Windows or OSX folder names.
pub(crate) fn game_resource_roots(game: &Path) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for relative in [
        "Overcooked2_Data/StreamingAssets",
        "Overcooked2.app/Contents/Resources/Data/StreamingAssets",
        "Contents/Resources/Data/StreamingAssets",
    ] {
        let Ok(parent) = crate::game::safe_game_path(game, relative) else {
            continue;
        };
        let Ok(root) = parent.canonicalize() else {
            continue;
        };
        paths.insert(root.clone());
        let mut queue = vec![(root.clone(), 0)];
        while let Some((dir, depth)) = queue.pop() {
            if depth >= 2 || paths.len() >= 128 {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten().take(1024) {
                    let Ok(info) = entry.metadata() else { continue };
                    // Windows directory enumeration already supplies metadata.
                    // Files cannot add resource roots; do not open every bundle
                    // just to canonicalize a path that will be discarded.
                    if !info.is_dir() && !is_link(&info) {
                        continue;
                    }
                    let Some((path, info)) = resolved_entry(entry.path(), info, &root) else {
                        continue;
                    };
                    if info.is_dir() && paths.insert(path.clone()) {
                        queue.push((path, depth + 1));
                    }
                }
            }
        }
    }
    paths.into_iter().collect()
}
pub(crate) fn game_sources(game: &Path) -> Vec<PathBuf> {
    game_source_metadata(game, usize::MAX, usize::MAX)
        .unwrap_or_default()
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

/// The source list and its metadata come from the same enumeration. Callers
/// that need invalidation must not rediscover and stat every file afterwards.
pub(crate) fn game_source_metadata(
    game: &Path,
    max_files: usize,
    max_path_bytes: usize,
) -> Result<Vec<(PathBuf, Metadata)>, ()> {
    let mut paths = BTreeMap::new();
    let mut path_bytes = 0usize;
    for base in game_resource_roots(game) {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten().take(4096) {
                let Ok(info) = entry.metadata() else { continue };
                let Some((path, info)) = resolved_entry(entry.path(), info, &base) else {
                    continue;
                };
                if info.is_file() {
                    if !paths.contains_key(&path) {
                        path_bytes =
                            path_bytes.saturating_add(path.as_os_str().as_encoded_bytes().len());
                        if paths.len() >= max_files || path_bytes > max_path_bytes {
                            return Err(());
                        }
                    }
                    paths.insert(path, info);
                }
            }
        }
    }
    Ok(paths.into_iter().collect())
}

fn resolved_entry(path: PathBuf, info: Metadata, boundary: &Path) -> Option<(PathBuf, Metadata)> {
    if !is_link(&info) {
        // The parent is already canonical and names originate in read_dir.
        return Some((path, info));
    }
    let path = path.canonicalize().ok()?;
    if !path.starts_with(boundary) {
        return None;
    }
    let info = path.metadata().ok()?;
    Some((path, info))
}

fn is_link(info: &Metadata) -> bool {
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
mod tests;
