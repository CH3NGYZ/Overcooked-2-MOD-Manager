use super::*;

const FASTINIT_GUID: &str = "oc2.diylevel.fastinit";

fn siblings(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn single_dll_uninstall_still_asks_before_deleting_the_whole_package() {
    // 点单个 DLL、包内还有别人、没有同 GUID 副本 → 需要二次确认
    assert!(needs_package_confirmation(
        &siblings(&["A", "B"]),
        false,
        false
    ));
}

#[test]
fn package_level_uninstall_never_asks_again() {
    // 包标题的「删除」/ 冲突卸载：已经表达过整包语义，不能重复确认
    assert!(!needs_package_confirmation(
        &siblings(&["A", "B"]),
        false,
        true
    ));
}

#[test]
fn other_cases_skip_the_confirmation() {
    // 包内没有其他 MOD：删的就是单个文件
    assert!(!needs_package_confirmation(&siblings(&[]), false, false));
    // 存在同 GUID 的启用副本：走单 DLL 回收分支，不是整包删除
    assert!(!needs_package_confirmation(
        &siblings(&["A", "B"]),
        true,
        false
    ));
}

#[test]
fn confirmed_package_uninstall_overrides_duplicate_guid_fast_path() {
    // 冲突弹窗已确认整包后，即使存在另一个同 GUID 副本，也必须走整包回收。
    assert!(!should_remove_only_selected_dll(true, true));
    // 普通单 DLL 删除仍保留原来的共享副本保护。
    assert!(should_remove_only_selected_dll(true, false));
    assert!(!should_remove_only_selected_dll(false, false));
}

#[test]
fn confirmed_duplicate_cleanup_keeps_current_fastinit_and_other_package_files() {
    let old_guids = [
        FASTINIT_GUID,
        #[cfg(windows)] // Reading PE version resources requires the Windows API.
        "yang.oc2.diylevel.fastinit",
    ];
    for old_guid in old_guids {
        let temp = tempfile::tempdir().unwrap();
        let game_root = temp.path().join("game");
        let old = "BepInEx/plugins/FastInit-old/FastInit.dll";
        let newest = "BepInEx/plugins/FastInit/FastInit.dll";
        let other = "BepInEx/plugins/FastInit-old/Sibling.dll";
        for (file, bytes) in [
            ("BepInEx/core/BepInEx.dll", b"fixture loader".to_vec()),
            (old, fastinit_dll(old_guid, "1.3")),
            (newest, fastinit_dll(FASTINIT_GUID, "1.9.5")),
            (
                other,
                mods::plugin_blob("fixture-sibling", "Sibling", "1.0"),
            ),
        ] {
            let path = game_root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let state = AppState::with_config(crate::config::ConfigService::for_test(
            temp.path().join("data"),
        ));
        let mut config = crate::config::default_config();
        config.game.path = game_root.to_string_lossy().into_owned();
        config.recycle_deleted_mods = true;
        let mut repo = crate::community::repository_from_coordinates(
            &crate::models::Provider::Github,
            "Fixture",
            "FastInit",
            "",
            "",
        )
        .unwrap();
        // Model a configured source, not a community search result (which the
        // installed list deliberately cannot use as a repository binding).
        repo.source_id = None;
        repo.plugin_guid = FASTINIT_GUID.into();
        config.repositories = vec![repo.clone()];
        state.config.save(&config).unwrap();
        let manifests = state.config.manifests_dir();
        for (package, release, files) in [
            ("old", "v1.3", vec![old.into(), other.into()]),
            ("new", "v1.9.5", vec![newest.into()]),
        ] {
            let mut manifest = archive::new_manifest(
                package,
                release,
                Default::default(),
                Default::default(),
                files,
                vec![],
            );
            manifest.repository_binding =
                crate::models::ModRepositoryBinding::from_release(&repo, release);
            archive::write_manifest(&manifests, &manifest).unwrap();
        }

        let installed = mods::scan_mods_with_manifests(&game_root, &config, Some(&manifests));
        let old_item = installed.iter().find(|item| item.dll_file == old).unwrap();
        assert!(old_item.duplicate_cleanup_available, "GUID: {old_guid}");
        assert_eq!(
            old_item.status, "duplicate_lower_version",
            "GUID: {old_guid}"
        );
        assert!(!old_item.update_available);
        let new_bytes = std::fs::read(game_root.join(newest)).unwrap();
        assert_eq!(
            uninstall_local_mod_blocking(&state, newest, false, true).unwrap_err(),
            "modChanges.newerCopyNotFound"
        );
        let alternate = game_root.join(mods::disabled_dll_file(old));
        std::fs::write(&alternate, b"do not remove ambiguous variant").unwrap();
        assert_eq!(
            uninstall_local_mod_blocking(&state, old, false, true).unwrap_err(),
            "modChanges.newerCopyNotFound"
        );
        assert!(game_root.join(old).is_file());
        std::fs::remove_file(alternate).unwrap();

        let result = uninstall_local_mod_blocking(&state, old, false, true).unwrap();
        assert!(result.ok);
        assert!(!result.requires_package_confirmation);
        assert!(!game_root.join(old).exists());
        assert_eq!(std::fs::read(game_root.join(newest)).unwrap(), new_bytes);
        assert!(game_root.join(other).exists());
        assert_eq!(
            archive::read_manifest(&manifests, "old").unwrap().files,
            [other]
        );
        assert_eq!(
            archive::read_manifest(&manifests, "new").unwrap().files,
            [newest]
        );
    }
}

fn fastinit_dll(guid: &str, version: &str) -> Vec<u8> {
    let name = if guid == FASTINIT_GUID {
        "FastInit"
    } else {
        "Legacy FastInit"
    };
    let plugin = mods::plugin_blob(guid, name, version);
    if cfg!(windows) {
        let mut pe = include_bytes!(
            "../../mod_changes/orchestration_tests/duplicate_release_target/fastinit_version.pe"
        )
        .to_vec();
        pe.extend(plugin);
        pe
    } else {
        plugin
    }
}

#[test]
fn duplicate_cleanup_rejects_multiple_manifest_claims_without_mutating_either_package() {
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    let old = "BepInEx/plugins/FastInit-old/FastInit.dll";
    let newest = "BepInEx/plugins/FastInit/FastInit.dll";
    let old_bytes = mods::plugin_blob(FASTINIT_GUID, "FastInit", "1.3");
    let new_bytes = mods::plugin_blob(FASTINIT_GUID, "FastInit", "1.9.5");
    for (file, bytes) in [
        ("BepInEx/core/BepInEx.dll", b"fixture loader".as_slice()),
        (old, old_bytes.as_slice()),
        (newest, new_bytes.as_slice()),
    ] {
        let path = game_root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let state = AppState::with_config(crate::config::ConfigService::for_test(
        temp.path().join("data"),
    ));
    let manifests = state.config.manifests_dir();
    let alias = mods::disabled_dll_file(&old.to_uppercase().replace('/', "\\"));
    let mut originals = Vec::new();
    for (id, file) in [("old", old.to_string()), ("other", alias)] {
        let manifest = archive::new_manifest(
            id,
            "v1.3",
            Default::default(),
            Default::default(),
            vec![file],
            vec![],
        );
        archive::write_manifest(&manifests, &manifest).unwrap();
        let path = manifests.join(format!("{id}.toml"));
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }

    let mut config = crate::config::default_config();
    config.game.path = game_root.to_string_lossy().into_owned();
    for recycle_deleted_mods in [true, false] {
        config.recycle_deleted_mods = recycle_deleted_mods;
        state.config.save(&config).unwrap();
        assert_eq!(
            uninstall_local_mod_blocking(&state, old, false, true).unwrap_err(),
            "modChanges.ambiguousOwnership"
        );
        assert_eq!(std::fs::read(game_root.join(old)).unwrap(), old_bytes);
        assert_eq!(std::fs::read(game_root.join(newest)).unwrap(), new_bytes);
        for (path, bytes) in &originals {
            assert_eq!(&std::fs::read(path).unwrap(), bytes);
        }
    }
}
