use crate::recycle_bin::records::list;
use crate::recycle_bin::restore::restore;
use crate::recycle_bin::selection::recycle_package;
use crate::recycle_bin::selection::recycle_selected_mod;
use std::fs;

use super::{A, B, DATA, Workspace};

#[test]
fn empty_missing_or_shared_only_packages_do_not_create_empty_recycle_records() {
    for condition in ["empty", "missing", "shared"] {
        let workspace = Workspace::new();
        workspace.manifest("package", if condition == "empty" { &[] } else { &[A] });
        if condition == "shared" {
            workspace.write(A, b"shared DLL");
            workspace.manifest("peer", &[A]);
        }
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let error = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap_err();
        assert_eq!(error, "recycle.noMovableFiles");
        assert!(list(&workspace.recycle).unwrap().is_empty());
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        if condition == "shared" {
            assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"shared DLL");
            assert!(workspace.active("peer").is_some());
        }
    }
}

#[test]
fn selected_mod_recycles_duplicate_manifest_references_and_restores_them() {
    for disabled in [false, true] {
        for whole_package in [false, true] {
            let workspace = Workspace::new();
            let actual = if disabled {
                format!("{A}.disabled")
            } else {
                A.into()
            };
            workspace.write(&actual, b"selected plugin");
            workspace.write(DATA, b"shared data");
            workspace.write(B, b"unrelated plugin");
            workspace.manifest("package", &[A, DATA]);
            workspace.manifest("manual", &[A]);
            workspace.manifest("peer", &[A, B, DATA]);
            let originals = ["package", "manual", "peer"].map(|id| {
                (
                    id,
                    fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                )
            });
            let entry = recycle_selected_mod(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                A,
                whole_package,
            )
            .unwrap();
            assert_eq!(entry.files, vec![actual.clone()]);
            assert!(!workspace.game.join(&actual).exists());
            assert!(workspace.active("manual").is_none());
            assert_eq!(workspace.active("peer").unwrap().files, vec![B, DATA]);
            if whole_package {
                assert!(workspace.active("package").is_none());
            } else {
                assert_eq!(workspace.active("package").unwrap().files, vec![DATA]);
            }
            assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"shared data");
            assert_eq!(
                fs::read(workspace.game.join(B)).unwrap(),
                b"unrelated plugin"
            );
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id,
            )
            .unwrap();
            assert_eq!(
                fs::read(workspace.game.join(&actual)).unwrap(),
                b"selected plugin"
            );
            for (id, original) in originals {
                assert_eq!(
                    fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                    original
                );
            }
        }
    }
}

#[test]
fn selected_mod_rejects_missing_or_unowned_dll_without_changing_manifests() {
    for selected in [A, B] {
        let workspace = Workspace::new();
        workspace.write(DATA, b"keep data");
        workspace.write(B, b"unowned DLL");
        workspace.manifest("package", &[A, DATA]);
        workspace.manifest("peer", &[A]);
        let result = recycle_selected_mod(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            selected,
            true,
        );
        assert_eq!(
            result.unwrap_err(),
            if selected == A {
                "mods.dllNotFound"
            } else {
                "mods.dllNotInPackage"
            }
        );
        assert_eq!(workspace.active("package").unwrap().files, vec![A, DATA]);
        assert_eq!(workspace.active("peer").unwrap().files, vec![A]);
        assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"keep data");
        assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"unowned DLL");
        assert!(list(&workspace.recycle).unwrap().is_empty());
    }
}

#[test]
fn shared_files_stay_with_other_active_package() {
    let workspace = Workspace::new();
    workspace.write(A, b"owned");
    workspace.write(DATA, b"shared");
    workspace.manifest("package", &[A, DATA]);
    workspace.manifest("peer", &[DATA]);
    let entry = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        None,
    )
    .unwrap();
    assert_eq!(entry.files, vec![A]);
    assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"shared");
    assert!(workspace.active("peer").is_some());
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    assert_eq!(workspace.active("package").unwrap().files, vec![A, DATA]);
}
