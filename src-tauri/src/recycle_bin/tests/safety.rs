use crate::archive;
use crate::recycle_bin::delete::delete;
use crate::recycle_bin::errors::CONFLICT;
use crate::recycle_bin::errors::INCOMPLETE;
use crate::recycle_bin::errors::INVALID_RECORD;
use crate::recycle_bin::records::list;
use crate::recycle_bin::records::load_record;
use crate::recycle_bin::records::save_record;
use crate::recycle_bin::restore::restore;
use crate::recycle_bin::selection::recycle_package;
use crate::recycle_bin::selection::recycle_unmanaged;
use crate::recycle_bin::staging::stage;
use std::fs;

use super::{A, B, Workspace};

/// 没有 `package_id` 却带着清单快照的记录是坏记录（台账不一致），
/// 不能静默跳过清单合并把文件搬回去。
#[test]
fn restore_rejects_record_with_snapshot_but_no_package_id() {
    let workspace = Workspace::new();
    workspace.write(A, b"plugin");
    let entry = stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "Loose plugin".into(),
        None,
        None,
        Vec::new(),
        vec![A.into()],
        None,
        &mut || Ok(()),
    )
    .unwrap();
    let directory = workspace.recycle.join(&entry.id);
    fs::write(directory.join("manifest.toml"), b"snapshot").unwrap();
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap_err(),
        INVALID_RECORD
    );
    assert!(!workspace.game.join(A).exists());
}

#[test]
fn restoring_disabled_dll_refuses_enabled_variant_and_keeps_both_copies() {
    let workspace = Workspace::new();
    workspace.write(&format!("{A}.disabled"), b"original");
    let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
    workspace.write(A, b"new install");
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        CONFLICT
    );
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"new install");
    assert_eq!(
        fs::read(
            workspace
                .recycle
                .join(&entry.id)
                .join("files")
                .join(format!("{A}.disabled"))
        )
        .unwrap(),
        b"original"
    );
    assert_eq!(list(&workspace.recycle).unwrap().len(), 1);
}

#[test]
fn restore_refuses_changed_game_directory_and_reinstalled_package() {
    let workspace = Workspace::new();
    workspace.write(A, b"original");
    workspace.manifest("package", &[A]);
    let entry = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        None,
    )
    .unwrap();
    let other_game = workspace.directory.join("other-game");
    fs::create_dir_all(&other_game).unwrap();
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &other_game,
            &entry.id
        )
        .unwrap_err(),
        "recycle.wrongGameDirectory"
    );
    let mut updated = workspace.manifest("package", &[B]);
    updated.release_tag = "v2".into();
    archive::write_manifest(&workspace.manifests, &updated).unwrap();
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        "recycle.packageUpdated"
    );
    assert_eq!(workspace.active("package").unwrap().release_tag, "v2");
    assert!(workspace.recycle.join(entry.id).exists());
}

#[test]
fn forged_ids_paths_and_corrupted_payload_cannot_escape_or_destroy_data() {
    let workspace = Workspace::new();
    workspace.write(A, b"original");
    assert!(recycle_unmanaged(&workspace.recycle, &workspace.game, "../outside.dll").is_err());
    assert!(
        recycle_unmanaged(
            &workspace.recycle,
            &workspace.game,
            "BepInEx/plugins/a.dll:stream"
        )
        .is_err()
    );
    assert!(delete(&workspace.recycle, "../game").is_err());
    assert!(
        recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "../../outside",
            None
        )
        .is_err()
    );
    let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
    let directory = workspace.recycle.join(&entry.id);
    let original_record = load_record(&directory, &entry.id).unwrap();
    let mut forged = original_record.clone();
    forged.files[0].relative = "../../outside.dll".into();
    save_record(&directory, &forged).unwrap();
    assert!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .is_err()
    );
    assert!(delete(&workspace.recycle, &entry.id).is_err());
    assert_eq!(
        fs::read(directory.join("files").join(A)).unwrap(),
        b"original"
    );
    save_record(&directory, &original_record).unwrap();
    fs::write(directory.join("files").join(A), b"changed staged file").unwrap();
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        INCOMPLETE
    );
    assert!(directory.exists());
    assert!(!workspace.game.join(A).exists());
}

#[test]
fn undeclared_staged_files_are_preserved_when_restore_metadata_is_incomplete() {
    let workspace = Workspace::new();
    workspace.write(A, b"original");
    let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
    let directory = workspace.recycle.join(&entry.id);
    let unknown = directory.join("files/omitted-from-record.dll");
    fs::write(&unknown, b"must not disappear during restore").unwrap();
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        INCOMPLETE
    );
    assert_eq!(
        fs::read(&unknown).unwrap(),
        b"must not disappear during restore"
    );
    assert_eq!(
        fs::read(directory.join("files").join(A)).unwrap(),
        b"original"
    );
    assert!(!workspace.game.join(A).exists());
    // Explicit permanent deletion still owns the entire UUID directory.
    delete(&workspace.recycle, &entry.id).unwrap();
    assert!(!directory.exists());
}

#[test]
fn manager_inside_plugins_is_rejected_before_any_file_moves() {
    let workspace = Workspace::new();
    workspace.write(A, b"original");
    let recycle = workspace.game.join("BepInEx/plugins/manager/recycle-bin");
    assert_eq!(
        recycle_unmanaged(&recycle, &workspace.game, A).unwrap_err(),
        "recycle.insideModsDirectory"
    );
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"original");
    assert!(!recycle.exists());
}
