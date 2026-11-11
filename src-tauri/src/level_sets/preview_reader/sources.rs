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

/// Cross-set custom sprites can point into another installed set's info CAB.
/// Only index bundle directories; decoding remains reference driven.
pub(crate) fn sibling_info_sources(roots: &crate::level_sets::LevelSetRoots) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    let enabled = crate::level_sets::paths::levels_root(roots).ok();
    let disabled = roots
        .safe_path(&crate::level_sets::paths::disabled_root(roots.game_root()))
        .ok();
    for root in enabled.into_iter().chain(disabled) {
        let Ok(base) = root.canonicalize() else {
            continue;
        };
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten().take(1024) {
                let Ok(dir) = entry.path().canonicalize() else {
                    continue;
                };
                if dir.starts_with(&base) && dir.is_dir() {
                    if let Some(info) = crate::level_sets::scan::locate_info_file(&dir) {
                        let Ok(info) = info.canonicalize() else {
                            continue;
                        };
                        if info.starts_with(&base) {
                            paths.insert(info);
                        }
                    }
                }
            }
        }
    }
    paths.into_iter().collect()
}

pub(crate) use crate::unity_assets::{game_resource_roots, game_sources};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sibling_info_sources_follow_scan_rules_in_enabled_and_disabled_roots() {
        let root = std::env::temp_dir().join(format!(
            "oc2-sibling-info-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let roots = crate::level_sets::LevelSetRoots::new(
            root.clone(),
            Some(root.join("BepInEx/plugins/renamed-diy-level-v3")),
        );
        let enabled = crate::level_sets::paths::levels_root(&roots).unwrap();
        let disabled = crate::level_sets::paths::disabled_root(&root);
        for path in [
            enabled.join("one/info_custom"),
            disabled.join("two/info"),
            enabled.join("ambiguous/info_a"),
            enabled.join("ambiguous/info_b"),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"fixture").unwrap();
        }
        let found = sibling_info_sources(&roots);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&enabled.join("one/info_custom").canonicalize().unwrap()));
        assert!(found.contains(&disabled.join("two/info").canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }
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
