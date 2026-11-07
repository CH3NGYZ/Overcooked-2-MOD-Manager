use crate::recycle_bin::errors::IO_ERROR;
use crate::recycle_bin::records::RelatedManifest;
use crate::recycle_bin::records::list;
use crate::recycle_bin::restore::restore;
use crate::recycle_bin::staging::stage;
use crate::recycle_bin::staging::stage_with_related_manifests;
use std::fs;

use super::{A, B, DATA, Workspace};

#[test]
fn duplicate_journal_commit_failure_rolls_back_every_manifest() {
    let workspace = Workspace::new();
    workspace.write(A, b"duplicate");
    let primary = workspace.manifest("package", &[A, DATA]);
    workspace.manifest("peer", &[A, B]);
    let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
    let peer_original = fs::read(workspace.manifests.join("peer.toml")).unwrap();
    let related = RelatedManifest {
        package_id: "peer".into(),
        original: String::from_utf8(peer_original.clone()).unwrap(),
        removed_files: vec![A.into()],
    };
    let mut remaining = primary.clone();
    remaining.files.retain(|file| file != A);
    let result = stage_with_related_manifests(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package".into(),
        Some(&primary),
        Some(&original),
        vec![A.into()],
        vec![A.into()],
        Some(&remaining),
        vec![related],
        &mut || {
            let entry = fs::read_dir(&workspace.recycle)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            fs::remove_file(entry.join("record.json")).unwrap();
            fs::create_dir(entry.join("record.json")).unwrap();
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err(), IO_ERROR);
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"duplicate");
    assert_eq!(
        fs::read(workspace.manifests.join("package.toml")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(workspace.manifests.join("peer.toml")).unwrap(),
        peer_original
    );
}

#[test]
fn mid_move_failure_rolls_back_files_and_leaves_original_manifest_untouched() {
    let workspace = Workspace::new();
    workspace.write(A, b"first");
    workspace.write(B, b"second");
    let manifest = workspace.manifest("package", &[A, B]);
    let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
    let error = stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package".into(),
        Some(&manifest),
        Some(&original),
        manifest.files.clone(),
        manifest.files.clone(),
        None,
        &mut || Err("injected failure".into()),
    )
    .unwrap_err();
    assert_eq!(error, "injected failure");
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"first");
    assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"second");
    assert_eq!(
        fs::read(workspace.manifests.join("package.toml")).unwrap(),
        original
    );
    assert!(list(&workspace.recycle).unwrap().is_empty());
}

#[test]
fn journal_commit_failure_restores_files_and_the_removed_active_manifest() {
    let workspace = Workspace::new();
    workspace.write(A, b"first");
    workspace.write(B, b"second");
    let manifest = workspace.manifest("package", &[A, B]);
    let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
    let mut count = 0;
    let result = stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package".into(),
        Some(&manifest),
        Some(&original),
        manifest.files.clone(),
        manifest.files.clone(),
        None,
        &mut || {
            count += 1;
            if count == 2 {
                let entry = fs::read_dir(&workspace.recycle)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::remove_file(entry.join("record.json")).unwrap();
                fs::create_dir(entry.join("record.json")).unwrap();
            }
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err(), IO_ERROR);
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"first");
    assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"second");
    assert_eq!(
        fs::read(workspace.manifests.join("package.toml")).unwrap(),
        original
    );
}

#[test]
fn rollback_conflict_preserves_staged_copy_and_recoverable_record() {
    let workspace = Workspace::new();
    workspace.write(A, b"original");
    let manifest = workspace.manifest("package", &[A]);
    let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
    let result = stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package".into(),
        Some(&manifest),
        Some(&original),
        manifest.files.clone(),
        manifest.files.clone(),
        None,
        &mut || {
            workspace.write(A, b"conflicting new file");
            Err("injected failure".into())
        },
    );
    assert_eq!(result.unwrap_err(), "recycle.partialFailure");
    let entries = list(&workspace.recycle).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        fs::read(workspace.recycle.join(&entries[0].id).join("files").join(A)).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(workspace.game.join(A)).unwrap(),
        b"conflicting new file"
    );
    assert_eq!(
        fs::read(workspace.manifests.join("package.toml")).unwrap(),
        original
    );
    fs::remove_file(workspace.game.join(A)).unwrap();
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entries[0].id,
    )
    .unwrap();
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"original");
}
