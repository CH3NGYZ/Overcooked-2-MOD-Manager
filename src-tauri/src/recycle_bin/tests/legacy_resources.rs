use super::{Workspace, plugin_blob};
use crate::{mods::plugin_target::DIYCHEF_GUID, recycle_bin::*};
use std::fs;

const CHEF: &str = "BepInEx/plugins/old/Chef.dll";
const MODEL: &str = "BepInEx/plugins/old/Resources/Fox/Body.obj";
const HAT: &str = "BepInEx/plugins/old/Resources/HATS/Cap/m_Cap.obj";
const PEER: &str = "BepInEx/plugins/Peer/Other.dll";

fn legacy(workspace: &Workspace, disabled: bool) -> String {
    let actual = format!("{CHEF}{}", if disabled { ".disabled" } else { "" });
    workspace.write(&actual, &plugin_blob(DIYCHEF_GUID, "DIYChef", "1.9"));
    workspace.write(MODEL, b"user model");
    workspace.write(HAT, b"user hat");
    actual
}

#[test]
fn automatic_purge_keeps_user_models_in_the_original_restorable_journal() {
    for mode in ["package", "selected", "partial", "unmanaged"] {
        for disabled in [false, true] {
            let workspace = Workspace::new();
            let actual = legacy(&workspace, disabled);
            let original = if mode == "unmanaged" {
                None
            } else {
                workspace.write(PEER, b"unrelated peer");
                workspace.manifest("package", &[CHEF, MODEL, PEER]);
                Some(fs::read(workspace.manifests.join("package.toml")).unwrap())
            };
            match mode {
                "package" => purge_package(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    "package",
                    None,
                )
                .unwrap(),
                "unmanaged" => purge_unmanaged(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    CHEF,
                )
                .unwrap(),
                _ => purge_selected_mod(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    "package",
                    CHEF,
                    mode == "selected",
                )
                .unwrap(),
            }
            assert!(!workspace.game.join(&actual).exists());
            assert!(!workspace.game.join(MODEL).exists());
            assert!(!workspace.game.join(HAT).exists());
            if mode == "partial" {
                assert_eq!(workspace.active("package").unwrap().files, vec![PEER]);
                assert_eq!(
                    fs::read(workspace.game.join(PEER)).unwrap(),
                    b"unrelated peer"
                );
            }
            let entries = list(&workspace.recycle).unwrap();
            assert_eq!(entries.len(), 1, "{mode}, disabled={disabled}");
            let payload = workspace.recycle.join(&entries[0].id).join("files");
            assert_eq!(fs::read(payload.join(MODEL)).unwrap(), b"user model");
            assert_eq!(fs::read(payload.join(HAT)).unwrap(), b"user hat");
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entries[0].id,
            )
            .unwrap();
            assert_eq!(fs::read(workspace.game.join(MODEL)).unwrap(), b"user model");
            assert_eq!(fs::read(workspace.game.join(HAT)).unwrap(), b"user hat");
            assert!(workspace.game.join(&actual).is_file());
            if let Some(original) = original {
                assert_eq!(
                    fs::read(workspace.manifests.join("package.toml")).unwrap(),
                    original
                );
            }
        }
    }
}

#[test]
fn unknown_versions_modern_chefs_and_unrelated_mods_keep_existing_purge_behavior() {
    for (guid, version) in [
        (DIYCHEF_GUID, "2.0"),
        (DIYCHEF_GUID, "2.2"),
        (DIYCHEF_GUID, "unknown"),
        ("fixture.other", "1.9"),
    ] {
        let workspace = Workspace::new();
        workspace.write(CHEF, &plugin_blob(guid, "fixture", version));
        workspace.write(MODEL, b"tracked model");
        workspace.write(HAT, b"unregistered file");
        workspace.manifest("package", &[CHEF, MODEL]);
        purge_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap();
        assert!(list(&workspace.recycle).unwrap().is_empty());
        assert!(!workspace.game.join(CHEF).exists());
        assert!(!workspace.game.join(MODEL).exists());
        assert_eq!(
            fs::read(workspace.game.join(HAT)).unwrap(),
            b"unregistered file"
        );
    }
    let workspace = Workspace::new();
    workspace.write(CHEF, &plugin_blob(DIYCHEF_GUID, "DIYChef", "1.9"));
    purge_unmanaged(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        CHEF,
    )
    .unwrap();
    assert!(
        list(&workspace.recycle).unwrap().is_empty(),
        "a DLL without models needs no retention"
    );
}

#[test]
fn shared_plugin_or_map_resources_are_rejected_before_any_move() {
    for condition in [
        "peer_manifest",
        "other_plugin",
        "registered_map",
        "flat_root",
    ] {
        let workspace = Workspace::new();
        let actual = legacy(&workspace, false);
        let (dll, model) = if condition == "flat_root" {
            let dll = "BepInEx/plugins/Chef.dll";
            let model = "BepInEx/plugins/Resources/Fox/Body.obj";
            workspace.write(dll, &plugin_blob(DIYCHEF_GUID, "DIYChef", "1.9"));
            workspace.write(model, b"user model");
            (dll, model)
        } else {
            (actual.as_str(), MODEL)
        };
        workspace.manifest("package", &[dll]);
        if condition == "peer_manifest" {
            workspace.manifest("peer", &[MODEL]);
        } else if condition == "other_plugin" || condition == "registered_map" {
            let peer = "BepInEx/plugins/old/MapPlugin.dll.disabled";
            workspace.write(peer, &plugin_blob("fixture.other", "Other", "1.0"));
            if condition == "registered_map" {
                fs::write(
                    workspace
                        .manifests
                        .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                    serde_json::to_vec(&serde_json::json!({"version": 1, "entries": [{
                        "dll": "BepInEx/plugins/old/MapPlugin.dll",
                        "owner": {"name": "Owned map", "author": "author"}
                    }]}))
                    .unwrap(),
                )
                .unwrap();
            }
        }
        assert_eq!(
            purge_selected_mod(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                dll,
                true
            )
            .unwrap_err(),
            "recycle.fileSharedByOtherPackage"
        );
        assert!(workspace.game.join(dll).is_file(), "{condition}");
        assert_eq!(fs::read(workspace.game.join(model)).unwrap(), b"user model");
        assert!(list(&workspace.recycle).unwrap().is_empty());
        assert!(workspace.active("package").is_some());
    }
}

#[test]
fn legacy_model_moves_roll_back_with_the_existing_file_and_manifest_transaction() {
    let workspace = Workspace::new();
    legacy(&workspace, false);
    let manifest = workspace.manifest("package", &[CHEF]);
    let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
    let mut moved = 0;
    let error = stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "chef".into(),
        Some(&manifest),
        Some(&original),
        vec![CHEF.into()],
        vec![CHEF.into()],
        None,
        &mut || {
            moved += 1;
            if moved == 2 {
                Err("injected model move failure".into())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error, "injected model move failure");
    assert_eq!(moved, 2);
    assert!(workspace.game.join(CHEF).is_file());
    assert_eq!(fs::read(workspace.game.join(MODEL)).unwrap(), b"user model");
    assert_eq!(fs::read(workspace.game.join(HAT)).unwrap(), b"user hat");
    assert_eq!(
        fs::read(workspace.manifests.join("package.toml")).unwrap(),
        original
    );
    assert!(list(&workspace.recycle).unwrap().is_empty());
}

#[test]
fn imported_models_and_explicit_empty_do_not_retain_records() {
    for imported in [false, true] {
        let workspace = Workspace::new();
        legacy(&workspace, false);
        let entry = recycle_unmanaged(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            CHEF,
        )
        .unwrap();
        if imported {
            set_diychef_resources_imported(&workspace.recycle, &entry.id, true).unwrap();
            purge_entry(&workspace.recycle, &entry.id).unwrap();
        } else {
            assert_eq!(delete_all(&workspace.recycle).unwrap(), 1);
        }
        assert!(list(&workspace.recycle).unwrap().is_empty());
        assert!(!workspace.recycle.join(entry.id).exists());
    }
}
