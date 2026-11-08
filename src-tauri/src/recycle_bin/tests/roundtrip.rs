use crate::archive;
use crate::archive::InstallManifest;
use crate::mods;
use crate::recycle_bin::delete::delete;
use crate::recycle_bin::records::list;
use crate::recycle_bin::restore::restore;
use crate::recycle_bin::selection::recycle_package;
use crate::recycle_bin::selection::recycle_unmanaged;
use crate::recycle_bin::staging::stage;
use std::fs;

use super::{A, B, DATA, Workspace};

#[test]
fn repository_binding_survives_package_and_child_restore_and_rejects_changed_owners() {
    for whole_package in [true, false] {
        for disabled in [false, true] {
            let workspace = Workspace::new();
            let actual = if disabled {
                format!("{A}.disabled")
            } else {
                A.into()
            };
            workspace.write(&actual, b"bound plugin");
            workspace.write(B, b"sibling plugin");
            let mut original = workspace.manifest("package", &[A, B]);
            original.repository_binding = Some(crate::models::ModRepositoryBinding {
                provider: crate::models::Provider::Github,
                owner: "Owner".into(),
                repository: "Mod".into(),
                tag: original.release_tag.clone(),
                channel: crate::models::Channel::Stable,
            });
            archive::write_manifest(&workspace.manifests, &original).unwrap();
            let entry = recycle_package(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                (!whole_package).then_some(A),
            )
            .unwrap();
            let snapshot: InstallManifest = toml::from_str(
                &fs::read_to_string(workspace.recycle.join(&entry.id).join("manifest.toml"))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(snapshot.repository_binding, original.repository_binding);
            if !whole_package {
                let mut changed = workspace.active("package").unwrap();
                changed.repository_binding.as_mut().unwrap().owner = "OtherOwner".into();
                archive::write_manifest(&workspace.manifests, &changed).unwrap();
                assert!(
                    restore(
                        &workspace.recycle,
                        &workspace.manifests,
                        &workspace.game,
                        &entry.id,
                    )
                    .unwrap_err()
                    .contains("recycle.packageUpdated")
                );
                assert!(!workspace.game.join(&actual).exists());
                assert!(workspace.recycle.join(&entry.id).exists());
                changed.repository_binding = original.repository_binding.clone();
                archive::write_manifest(&workspace.manifests, &changed).unwrap();
            }
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id,
            )
            .unwrap();
            assert_eq!(
                workspace.active("package").unwrap().repository_binding,
                original.repository_binding
            );
            assert_eq!(
                fs::read(workspace.game.join(&actual)).unwrap(),
                b"bound plugin"
            );
            assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"sibling plugin");
            assert!(list(&workspace.recycle).unwrap().is_empty());
        }
    }
}

#[test]
fn hiding_unidentified_plugins_keeps_all_package_files_recoverable() {
    for hide_unidentified in [true, false] {
        let workspace = Workspace::new();
        let mut plugin = b"BepInPlugin".to_vec();
        plugin.extend_from_slice(&[1, 0]);
        for value in ["com.example.recognized", "Recognized MOD", "1.0.0"] {
            plugin.push(value.len() as u8);
            plugin.extend_from_slice(value.as_bytes());
        }
        workspace.write(A, &plugin);
        workspace.write(
            &format!("{B}.disabled"),
            b"dependency without a BepInEx GUID",
        );
        workspace.write(DATA, b"settings");
        workspace.manifest("package", &[A, B, DATA]);
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "ui_zoom": 1.5,
            "hide_unidentified_mods": hide_unidentified,
            "game": {"path": ""},
            "network": {
                "timeout_seconds": 5,
                "download_threads": 1,
                "max_retry_count": 3
            },
            "launch": {
                "ask_user_to_start_from_steam": false,
                "auto_restore_winhttp": false
            },
            "auth": {
                "github_token": "",
                "gitee_token": "",
                "official_api_only": true
            },
            "repositories": []
        }))
        .unwrap();
        let installed =
            mods::scan_mods_with_manifests(&workspace.game, &config, Some(&workspace.manifests));
        assert_eq!(installed.len(), 2);
        assert_eq!(
            installed.iter().filter(|item| item.guid.is_some()).count(),
            1
        );
        assert!(installed.iter().all(
            |item| item.package_id.as_deref() == Some("package") && item.package_mod_count == 2
        ));

        let entry = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap();
        assert_eq!(entry.file_count, 3);
        assert_eq!(
            entry.files,
            vec![A.to_owned(), format!("{B}.disabled"), DATA.to_owned()]
        );
        assert!(workspace.active("package").is_none());
        for file in &entry.files {
            assert!(!workspace.game.join(file).exists());
        }
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), plugin);
        assert_eq!(
            fs::read(workspace.game.join(format!("{B}.disabled"))).unwrap(),
            b"dependency without a BepInEx GUID"
        );
        assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"settings");
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
    }
}

/*
 * 卸载 BepInEx 框架：它的 ZIP 不只解压出 `BepInEx/`，还有 `winhttp.dll`、
 * `doorstop_config.ini`、`run_bepinex.sh` 这些**直接落在游戏根目录**的文件。
 * 清单记的是这些游戏相对路径，所以按 package 回收必须把它们一起带走，
 * 否则用户以为卸干净了，实际 doorstop 还在接管启动。
 */
#[test]
fn framework_manifest_recycles_root_level_files_too() {
    let workspace = Workspace::new();
    const FRAMEWORK_FILES: &[&str] = &[
        "BepInEx/core/BepInEx.dll",
        "BepInEx/config/BepInEx.cfg",
        "winhttp.dll",
        "doorstop_config.ini",
        "run_bepinex.sh",
    ];
    for file in FRAMEWORK_FILES {
        workspace.write(file, b"framework");
    }
    workspace.manifest("bepinex-framework", FRAMEWORK_FILES);

    let entry = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "bepinex-framework",
        None,
    )
    .unwrap();

    assert_eq!(entry.file_count, FRAMEWORK_FILES.len());
    for file in FRAMEWORK_FILES {
        assert!(!workspace.game.join(file).exists(), "{file} 应已移走");
    }
    // 回收记录里也要包含根目录文件，恢复时才能原样还原
    for file in ["winhttp.dll", "doorstop_config.ini", "run_bepinex.sh"] {
        assert!(
            entry.files.iter().any(|item| item == file),
            "{file} 应记录在案"
        );
    }

    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    for file in FRAMEWORK_FILES {
        assert!(workspace.game.join(file).exists(), "{file} 应已恢复");
    }
}

#[test]
fn whole_package_preserves_disabled_paths_nondefault_target_and_raw_manifest() {
    let workspace = Workspace::new();
    workspace.write(A, b"first DLL");
    workspace.write(&format!("{B}.disabled"), b"disabled DLL");
    workspace.write(DATA, b"settings");
    workspace.write("CustomInstallTarget/data.txt", b"non-default target");
    let original = workspace.manifest("package", &[A, B, DATA, "CustomInstallTarget/data.txt"]);
    let bytes = fs::read(workspace.manifests.join("package.toml")).unwrap();
    let entry = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        None,
    )
    .unwrap();
    assert_eq!(entry.file_count, 4);
    assert_eq!(entry.name, "Fixture package");
    assert!(entry.files.contains(&format!("{B}.disabled")));
    assert!(!workspace.game.join(A).exists());
    assert!(workspace.active("package").is_none());
    assert_eq!(
        fs::read(workspace.recycle.join(&entry.id).join("manifest.toml")).unwrap(),
        bytes
    );
    assert_eq!(list(&workspace.recycle).unwrap().len(), 1);
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"first DLL");
    assert!(!workspace.game.join(B).exists());
    assert_eq!(
        fs::read(workspace.game.join(format!("{B}.disabled"))).unwrap(),
        b"disabled DLL"
    );
    assert_eq!(
        fs::read(workspace.game.join("CustomInstallTarget/data.txt")).unwrap(),
        b"non-default target"
    );
    assert_eq!(workspace.active("package").unwrap().files, original.files);
    assert!(list(&workspace.recycle).unwrap().is_empty());
}

#[test]
fn partial_restore_merges_only_its_own_files_and_retains_current_metadata() {
    let workspace = Workspace::new();
    workspace.write(A, b"first");
    workspace.write(B, b"second");
    workspace.write(DATA, b"settings");
    workspace.manifest("package", &[A, B, DATA]);
    let first = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        Some(A),
    )
    .unwrap();
    assert_eq!(workspace.active("package").unwrap().files, vec![B, DATA]);
    let second = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        Some(B),
    )
    .unwrap();
    let mut active = workspace.active("package").unwrap();
    active.note_i18n = crate::archive::single_language("edited after removal");
    archive::write_manifest(&workspace.manifests, &active).unwrap();
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &first.id,
    )
    .unwrap();
    let merged = workspace.active("package").unwrap();
    assert_eq!(merged.files, vec![A, DATA]);
    assert_eq!(
        crate::archive::primary_translation(&merged.note_i18n),
        "edited after removal"
    );
    assert!(!workspace.game.join(B).exists());
    assert!(workspace.recycle.join(&second.id).exists());
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &second.id,
    )
    .unwrap();
    assert_eq!(workspace.active("package").unwrap().files, vec![A, B, DATA]);
}

#[test]
fn unmanaged_disabled_dll_can_restore_or_be_permanently_deleted() {
    let workspace = Workspace::new();
    workspace.write(&format!("{A}.disabled"), b"unmanaged");
    let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
    assert_eq!(entry.files, vec![format!("{A}.disabled")]);
    assert!(entry.package_id.is_none());
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    assert_eq!(
        fs::read(workspace.game.join(format!("{A}.disabled"))).unwrap(),
        b"unmanaged"
    );
    let removed = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
    delete(&workspace.recycle, &removed.id).unwrap();
    assert!(!workspace.recycle.join(removed.id).exists());
    assert!(!workspace.game.join(format!("{A}.disabled")).exists());
    assert!(list(&workspace.recycle).unwrap().is_empty());
}

/// 关卡集按整目录回收（`package_id` 为 None、没有清单快照），恢复要把
/// `info` 与场景文件原样搬回去 —— 不能因为它们不是 .dll 就判成坏记录。
#[test]
fn restore_level_set_files_staged_without_package() {
    let workspace = Workspace::new();
    let info = "BepInEx/plugins/OC2DIYLevel/levels/Test Set/info";
    let scene = "BepInEx/plugins/OC2DIYLevel/levels/Test Set/s_test_1";
    let requires = "BepInEx/plugins/OC2DIYLevel/levels/Test Set/requires.txt";
    workspace.write(info, b"info bytes");
    workspace.write(scene, b"scene bytes");
    workspace.write(requires, b"runtime");
    let entry = stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "Test Set".into(),
        None,
        None,
        Vec::new(),
        vec![info.into(), scene.into(), requires.into()],
        None,
        &mut || Ok(()),
    )
    .unwrap();
    assert!(!workspace.game.join(info).exists());
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &entry.id,
    )
    .unwrap();
    assert_eq!(fs::read(workspace.game.join(info)).unwrap(), b"info bytes");
    assert_eq!(
        fs::read(workspace.game.join(scene)).unwrap(),
        b"scene bytes"
    );
    assert_eq!(fs::read(workspace.game.join(requires)).unwrap(), b"runtime");
    assert!(list(&workspace.recycle).unwrap().is_empty());
}
