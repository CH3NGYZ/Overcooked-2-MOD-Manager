use crate::recycle_bin::delete::delete;
use crate::recycle_bin::errors::UNSAFE_PATH;
use crate::recycle_bin::paths::is_link;
use crate::recycle_bin::restore::restore;
use crate::recycle_bin::selection::recycle_unmanaged;
use std::fs;
use std::path::Path;

use super::{A, Workspace};

#[cfg(any(windows, unix))]
fn directory_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        // Directory junctions do not require symlink privileges on Windows.
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(target.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        if !output.status.success() {
            eprintln!(
                "junction creation failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return output.status.success();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(any(windows, unix))]
fn remove_directory_link(link: &Path) {
    assert!(is_link(&fs::symlink_metadata(link).unwrap()));
    #[cfg(windows)]
    fs::remove_dir(link).unwrap();
    #[cfg(unix)]
    fs::remove_file(link).unwrap();
}

#[test]
#[cfg(any(windows, unix))]
fn junctions_in_source_destination_or_recycle_payload_never_follow_outside_paths() {
    let workspace = Workspace::new();
    let outside = workspace.directory.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("A.dll"), b"outside original").unwrap();
    let linked_source = workspace.game.join("BepInEx/plugins/Linked");
    fs::create_dir_all(linked_source.parent().unwrap()).unwrap();
    assert!(
        directory_link(&linked_source, &outside),
        "test link creation failed"
    );
    assert_eq!(
        recycle_unmanaged(
            &workspace.recycle,
            &workspace.game,
            "BepInEx/plugins/Linked/A.dll"
        )
        .unwrap_err(),
        UNSAFE_PATH
    );
    remove_directory_link(&linked_source);
    workspace.write(A, b"staged original");
    let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
    let directory = workspace.recycle.join(&entry.id);
    let bundle = directory.join("files/BepInEx/plugins/Bundle");
    let preserved = workspace.directory.join("preserved-staged.dll");
    fs::rename(bundle.join("A.dll"), &preserved).unwrap();
    fs::remove_dir(&bundle).unwrap();
    assert!(directory_link(&bundle, &outside));
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        UNSAFE_PATH
    );
    assert_eq!(
        delete(&workspace.recycle, &entry.id).unwrap_err(),
        UNSAFE_PATH
    );
    assert_eq!(
        fs::read(outside.join("A.dll")).unwrap(),
        b"outside original"
    );
    remove_directory_link(&bundle);
    fs::create_dir(&bundle).unwrap();
    fs::rename(&preserved, bundle.join("A.dll")).unwrap();
    let target_bundle = workspace.game.join("BepInEx/plugins/Bundle");
    assert!(directory_link(&target_bundle, &outside));
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        UNSAFE_PATH
    );
    assert_eq!(fs::read(bundle.join("A.dll")).unwrap(), b"staged original");
    assert_eq!(
        fs::read(outside.join("A.dll")).unwrap(),
        b"outside original"
    );
    remove_directory_link(&target_bundle);
    assert_eq!(
        fs::read(outside.join("A.dll")).unwrap(),
        b"outside original"
    );
}
