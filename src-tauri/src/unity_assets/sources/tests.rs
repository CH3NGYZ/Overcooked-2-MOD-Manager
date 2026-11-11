use super::*;
use std::fs;

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("unity-source-metadata-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(root.starts_with(&temp) && root != temp);
        fs::remove_dir_all(root).unwrap();
    }
}

fn canonical_file_sources(game: &Path) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for base in legacy_resource_roots(game) {
        if let Ok(entries) = fs::read_dir(&base) {
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

fn legacy_resource_roots(game: &Path) -> Vec<PathBuf> {
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
            if let Ok(entries) = fs::read_dir(dir) {
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

#[test]
fn enumerated_metadata_has_the_same_source_paths_and_bounded_depth() {
    let ws = Workspace::new();
    let base = ws.0.join("Overcooked2_Data/StreamingAssets");
    for relative in [
        "bundle0",
        "DLC/bundle1",
        "DLC/Bundle/bundle2",
        "DLC/Bundle/deeper/bundle3",
    ] {
        let path = base.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, relative.as_bytes()).unwrap();
    }
    let rows = game_source_metadata(&ws.0, 10, 4096).unwrap();
    let paths: Vec<_> = rows.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(paths, canonical_file_sources(&ws.0));
    assert_eq!(paths.len(), 3);
    for (path, info) in rows {
        let direct = path.metadata().unwrap();
        assert_eq!(info.len(), direct.len());
        assert_eq!(info.modified().unwrap(), direct.modified().unwrap());
    }
    assert!(game_source_metadata(&ws.0, 2, 4096).is_err());
    assert!(game_source_metadata(&ws.0, 10, 8).is_err());
}

#[cfg(any(unix, windows))]
#[test]
fn linked_directories_preserve_in_boundary_aliases_and_reject_escapes() {
    let ws = Workspace::new();
    let base = ws.0.join("Overcooked2_Data/StreamingAssets");
    fs::create_dir_all(base.join("DLC")).unwrap();
    fs::create_dir_all(ws.0.join("outside")).unwrap();
    fs::write(base.join("DLC/bundle1"), b"inside").unwrap();
    fs::write(ws.0.join("outside/bundle2"), b"outside").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(base.join("DLC"), base.join("alias")).unwrap();
        std::os::unix::fs::symlink(ws.0.join("outside"), base.join("escape")).unwrap();
        std::os::unix::fs::symlink(ws.0.join("outside/bundle2"), base.join("escape-file")).unwrap();
    }
    #[cfg(windows)]
    {
        // Junctions need no administrator/developer-mode symlink privilege.
        for (name, destination) in [
            ("alias", base.join("DLC")),
            ("escape", ws.0.join("outside")),
        ] {
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(base.join(name).to_string_lossy().replace('/', "\\"))
                .arg(destination.to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "{}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
    }
    let actual = game_sources(&ws.0);
    assert_eq!(actual, canonical_file_sources(&ws.0));
    assert_eq!(actual, [base.join("DLC/bundle1").canonicalize().unwrap()]);
    // Remove links themselves before recursive fixture cleanup.
    #[cfg(windows)]
    for name in ["alias", "escape"] {
        fs::remove_dir(base.join(name)).unwrap();
    }
}

#[test]
#[ignore = "requires installed game assets; only enumerates metadata"]
fn installed_game_source_paths_are_identical_to_canonical_enumeration() {
    let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    assert_eq!(game_sources(game), canonical_file_sources(game));
    assert!(!game_sources(game).is_empty());
}
