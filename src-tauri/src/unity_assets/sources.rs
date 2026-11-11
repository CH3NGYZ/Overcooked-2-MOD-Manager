//! Platform resource discovery shared by map and chef preview providers.
use std::{
    collections::BTreeSet,
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
                    if let Ok(path) = entry.path().canonicalize() {
                        if path.starts_with(&root) && path.is_dir() && paths.insert(path.clone()) {
                            queue.push((path, depth + 1));
                        }
                    }
                }
            }
        }
    }
    paths.into_iter().collect()
}
pub(crate) fn game_sources(game: &Path) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for base in game_resource_roots(game) {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten().take(4096) {
                if let Ok(path) = entry.path().canonicalize() {
                    if path.starts_with(&base) && path.is_file() {
                        paths.insert(path);
                    }
                }
            }
        }
    }
    paths.into_iter().collect()
}
