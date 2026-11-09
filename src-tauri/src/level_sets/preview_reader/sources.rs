//! Optional local resource providers; no plugin is a required preview dependency.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
pub(crate) fn common_sources(game: &Path, local: &Path) -> Vec<PathBuf> {
    let mut directories = vec![local.to_path_buf()];
    let plugins = match crate::game::safe_game_path(game, "BepInEx/plugins") {
        Ok(path) => path,
        Err(_) => return vec![],
    };
    if let Ok(base) = plugins.canonicalize() {
        directories.push(base.clone());
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten().take(1024) {
                if let Ok(path) = entry.path().canonicalize() {
                    if path.starts_with(&base) && path.is_dir() {
                        directories.push(path);
                    }
                }
            }
        }
    }
    let mut paths = BTreeSet::new();
    for directory in directories {
        let Ok(base) = directory.canonicalize() else {
            continue;
        };
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten().take(4096) {
                if !entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("common")
                {
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_and_macos_resources_and_optional_plugin_bundles() {
        let root = std::env::temp_dir().join(format!(
            "oc2-preview-paths-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let local = root.join("BepInEx/plugins/OC2DIYLevel/levels/renamed-set");
        std::fs::create_dir_all(&local).unwrap();
        for rel in [
            "Overcooked2_Data/StreamingAssets/Windows/bundle18",
            "Overcooked2.app/Contents/Resources/Data/StreamingAssets/OSX/bundle18",
            "Overcooked2.app/Contents/Resources/Data/StreamingAssets/StandaloneOSXUniversal/bundle99",
            "BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW3",
        ] {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"fixture").unwrap();
        }
        assert_eq!(game_sources(&root).len(), 3);
        assert_eq!(
            common_sources(&root, &local),
            vec![
                root.join("BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW3")
                    .canonicalize()
                    .unwrap()
            ]
        );
        std::fs::remove_file(root.join("BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW3"))
            .unwrap();
        assert!(common_sources(&root, &local).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
