use crate::archive;
use crate::recycle_bin::errors::INVALID_RECORD;
use crate::recycle_bin::records::list;
use crate::recycle_bin::records::load_record;
use crate::recycle_bin::records::save_record;
use crate::recycle_bin::restore::restore;
use crate::recycle_bin::selection::recycle_mod_dll;
use crate::recycle_bin::selection::recycle_package;
use std::fs;

use super::{A, B, DATA, Workspace, duplicate_plugin};

#[test]
fn duplicate_recycle_updates_all_owners_and_restores_their_manifests() {
    let workspace = Workspace::new();
    workspace.write(A, b"redundant plugin");
    workspace.write(B, b"effective plugin");
    workspace.write(DATA, b"keep unrelated data");
    let primary = workspace.manifest("package", &[A, DATA]);
    let peer = workspace.manifest("peer", &[A, B]);
    let historical = workspace.manifest("historical", &[A]);
    let installed = [duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "2.0.0")];
    let entry = recycle_mod_dll(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        A,
        &installed,
    )
    .unwrap();
    assert_eq!(entry.files, vec![A]);
    assert!(!workspace.game.join(A).exists());
    assert_eq!(
        fs::read(workspace.game.join(B)).unwrap(),
        b"effective plugin"
    );
    assert_eq!(
        fs::read(workspace.game.join(DATA)).unwrap(),
        b"keep unrelated data"
    );
    assert_eq!(workspace.active("package").unwrap().files, vec![DATA]);
    assert_eq!(workspace.active("peer").unwrap().files, vec![B]);
    assert!(workspace.active("historical").is_none());
    let record = load_record(&workspace.recycle.join(&entry.id), &entry.id).unwrap();
    assert_eq!(record.related_manifests.len(), 2);
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    assert_eq!(
        fs::read(workspace.game.join(A)).unwrap(),
        b"redundant plugin"
    );
    for original in [primary, peer, historical] {
        let restored = workspace.active(&original.repository_id).unwrap();
        assert_eq!(
            toml::to_string(&restored).unwrap(),
            toml::to_string(&original).unwrap()
        );
    }
}

#[test]
fn shared_dll_requires_existing_enabled_equal_or_newer_guid_copy() {
    for condition in [
        "different_guid",
        "empty_guid",
        "disabled",
        "older",
        "unknown_version",
        "warning",
        "same_path",
        "missing_file",
        "missing_target",
    ] {
        let workspace = Workspace::new();
        workspace.write(A, b"shared plugin");
        if condition != "missing_file" {
            workspace.write(B, b"other plugin");
        }
        workspace.manifest("package", &[A]);
        workspace.manifest("peer", &[A, DATA]);
        let mut target = duplicate_plugin(A, "2.0.0");
        let mut peer = duplicate_plugin(B, "2.0.0");
        match condition {
            "different_guid" => peer.guid = Some("another.plugin".into()),
            "empty_guid" => target.guid = None,
            "disabled" => peer.enabled = false,
            "older" => peer.version = Some("1.0.0".into()),
            "unknown_version" => peer.version = None,
            "warning" => peer.load_warning = Some("load warning".into()),
            "same_path" => peer.dll_file = format!("{A}.disabled"),
            _ => {}
        }
        let installed = if condition == "missing_target" {
            vec![peer]
        } else {
            vec![target, peer]
        };
        let result = recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &installed,
        );
        assert_eq!(
            result.unwrap_err(),
            "recycle.fileSharedByOtherPackage",
            "{condition}"
        );
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"shared plugin");
        assert_eq!(workspace.active("package").unwrap().files, vec![A]);
        assert_eq!(workspace.active("peer").unwrap().files, vec![A, DATA]);
        assert!(list(&workspace.recycle).unwrap().is_empty());
    }
}

#[test]
fn duplicate_replacement_must_cover_all_selected_path_variants() {
    let workspace = Workspace::new();
    workspace.write(A, b"enabled version 3");
    workspace.write(&format!("{A}.disabled"), b"disabled version 1");
    workspace.write(B, b"other version 2");
    workspace.manifest("package", &[A]);
    workspace.manifest("peer", &[A, B]);
    let mut older = duplicate_plugin(A, "1.0.0");
    older.enabled = false;
    let installed = [
        older,
        duplicate_plugin(A, "3.0.0"),
        duplicate_plugin(B, "2.0.0"),
    ];
    assert_eq!(
        recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &installed
        )
        .unwrap_err(),
        "recycle.fileSharedByOtherPackage"
    );
    assert_eq!(
        fs::read(workspace.game.join(A)).unwrap(),
        b"enabled version 3"
    );
    assert_eq!(
        fs::read(workspace.game.join(format!("{A}.disabled"))).unwrap(),
        b"disabled version 1"
    );
}

#[test]
fn disabled_duplicate_preserves_variant_and_canonical_peer_reference() {
    let workspace = Workspace::new();
    let disabled = format!("{A}.disabled");
    workspace.write(&disabled, b"disabled duplicate");
    workspace.write(B, b"effective plugin");
    workspace.manifest("package", &[A]);
    let peer_path = "BepInEx\\plugins\\Bundle\\A.DLL.disabled";
    workspace.manifest("peer", &[peer_path, B]);
    let mut target = duplicate_plugin(A, "1.0.0");
    target.enabled = false;
    let entry = recycle_mod_dll(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        A,
        &[target, duplicate_plugin(B, "1.0.0")],
    )
    .unwrap();
    assert_eq!(entry.files, vec![disabled.clone()]);
    assert_eq!(workspace.active("peer").unwrap().files, vec![B]);
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    assert!(!workspace.game.join(A).exists());
    assert_eq!(
        fs::read(workspace.game.join(disabled)).unwrap(),
        b"disabled duplicate"
    );
    assert_eq!(workspace.active("peer").unwrap().files, vec![peer_path, B]);
}

#[test]
fn duplicate_restore_refuses_updated_peer_before_moving_payload() {
    let workspace = Workspace::new();
    workspace.write(A, b"duplicate");
    workspace.write(B, b"effective");
    workspace.manifest("package", &[A]);
    workspace.manifest("peer", &[A, B]);
    let entry = recycle_mod_dll(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        A,
        &[duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "1.0.0")],
    )
    .unwrap();
    let mut updated = workspace.active("peer").unwrap();
    updated.release_tag = "new release".into();
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
    assert!(!workspace.game.join(A).exists());
    assert!(workspace.active("package").is_none());
    assert_eq!(workspace.active("peer").unwrap().release_tag, "new release");
    assert_eq!(
        fs::read(workspace.recycle.join(&entry.id).join("files").join(A)).unwrap(),
        b"duplicate"
    );
}

#[test]
fn duplicate_restore_preserves_later_independent_peer_removal() {
    let workspace = Workspace::new();
    workspace.write(A, b"duplicate");
    workspace.write(B, b"effective");
    workspace.write(DATA, b"data");
    workspace.manifest("package", &[A]);
    workspace.manifest("peer", &[A, B, DATA]);
    let first = recycle_mod_dll(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        A,
        &[duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "1.0.0")],
    )
    .unwrap();
    let second = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "peer",
        Some(B),
    )
    .unwrap();
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &first.id,
    )
    .unwrap();
    assert_eq!(workspace.active("peer").unwrap().files, vec![A, DATA]);
    assert!(!workspace.game.join(B).exists());
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &second.id,
    )
    .unwrap();
    assert_eq!(workspace.active("peer").unwrap().files, vec![A, B, DATA]);
}

#[test]
fn duplicate_restore_rejects_unrelated_manifest_snapshot_entries() {
    let workspace = Workspace::new();
    workspace.write(A, b"duplicate");
    workspace.write(B, b"effective");
    workspace.manifest("package", &[A]);
    workspace.manifest("peer", &[A, B]);
    let entry = recycle_mod_dll(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        A,
        &[duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "1.0.0")],
    )
    .unwrap();
    let directory = workspace.recycle.join(&entry.id);
    let mut record = load_record(&directory, &entry.id).unwrap();
    record.related_manifests[0].removed_files.push(B.into());
    save_record(&directory, &record).unwrap();
    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id
        )
        .unwrap_err(),
        INVALID_RECORD
    );
    assert!(!workspace.game.join(A).exists());
    assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"effective");
}
