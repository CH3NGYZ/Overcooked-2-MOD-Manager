use super::*;
use std::fs;

fn workspace() -> (PathBuf, PathBuf) {
    let game = std::env::temp_dir().join(format!("oc2-chef-delete-{}", uuid::Uuid::new_v4()));
    let mod_root = game.join("BepInEx/plugins/DIYChef");
    fs::create_dir_all(mod_root.join("Resources")).unwrap();
    (game, mod_root)
}

fn add_custom(resources: &Path, name: &str, id: u8) {
    let directory = resources.join(name);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("INFO"), format!("ID={id}\n")).unwrap();
    fs::write(directory.join("t_Head.png"), b"metadata").unwrap();
    fs::write(directory.join("m_Head.txt"), b"metadata").unwrap();
}

#[test]
fn custom_resource_path_accepts_only_direct_custom_resources_child() {
    let (game, mod_root) = workspace();
    let actual = custom_resource_path(&game, &mod_root, "custom", "00-Fox").unwrap();
    assert!(actual.ends_with(Path::new("Resources/00-Fox")));
    assert!(
        actual
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case("Resources"))
    );
    assert_eq!(
        custom_resource_path(&game, &mod_root, "native", "Chef_Black_cat").unwrap_err(),
        "diychef.invalidResource"
    );
    assert_eq!(
        custom_resource_path(&game, &mod_root, "custom", "HATS").unwrap_err(),
        "diychef.invalidResource"
    );
    assert!(custom_resource_path(&game, &mod_root, "custom", "../outside").is_err());
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn deletion_accepts_enabled_or_disabled_custom_chef_but_requires_saved_config() {
    assert!(check_delete_allowed("custom", true).is_ok());
    assert_eq!(
        check_delete_allowed("custom", false).unwrap_err(),
        "diychef.invalidResource"
    );
    assert_eq!(
        check_delete_allowed("native", true).unwrap_err(),
        "diychef.invalidResource"
    );
}

#[test]
fn deleting_enabled_custom_chef_removes_its_txt_row_and_binding_but_keeps_native_fallback() {
    let (game, mod_root) = workspace();
    let resources = mod_root.join("Resources");
    add_custom(&resources, "Fox", 7);
    fs::write(
        mod_root.join("prefer.txt"),
        b"Fox HAT=Santa\r\nChef_Black_cat HAT=None\r\n",
    )
    .unwrap();
    let catalog = catalog::read(&game, &mod_root).unwrap();
    let custom_id = catalog
        .chefs
        .iter()
        .find(|chef| chef.config_name == "Fox")
        .unwrap()
        .id
        .clone();
    let native_id = "native:Chef_Black_cat";
    let hat_bindings = std::collections::BTreeMap::from([
        (
            custom_id.clone(),
            HatReference::Known {
                hat_id: "hat:Santa".into(),
            },
        ),
        (
            native_id.into(),
            HatReference::Known {
                hat_id: "hat:None".into(),
            },
        ),
    ]);
    let bindings_dir = game.join("manager/diychef");
    storage::write(
        &game,
        &mod_root,
        "target",
        &bindings_dir,
        &storage::read(&game, &mod_root, "target", &bindings_dir)
            .unwrap()
            .source
            .revision,
        b"Fox HAT=Santa\r\nChef_Black_cat HAT=None\r\n",
        &hat_bindings,
        false,
    )
    .unwrap();
    let stored = storage::read(&game, &mod_root, "target", &bindings_dir).unwrap();
    let session = Session {
        id: "session".into(),
        game_root: game.clone(),
        game_settings: GameConfig {
            path: game.to_string_lossy().into_owned(),
            runtime: crate::models::GameRuntime::Native,
        },
        mod_root: mod_root.clone(),
        target: ChefTarget {
            id: "target".into(),
            guid: "test".into(),
            version: Some("2.2.0".into()),
            display_path: "fixture".into(),
            enabled: true,
            generation: 1,
            compatibility: "fastinit_2_2".into(),
            supports_custom_default: true,
            supports_live_save: false,
        },
        catalog,
        resources_revision: 0,
        stored,
        bindings_dir,
    };

    let (draft, bytes) = deletion_plan(&session, &custom_id).unwrap();
    assert_eq!(draft.entries.len(), 1);
    assert!(draft.hat_bindings.contains_key(native_id));
    assert!(!draft.hat_bindings.contains_key(&custom_id));
    assert_eq!(bytes, b"Chef_Black_cat HAT=None\r\n");
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn rollback_after_delete_restores_original_prefer_and_complete_hat_bindings() {
    let (game, mod_root) = workspace();
    add_custom(&mod_root.join("Resources"), "Fox", 7);
    let bindings_dir = game.join("manager/diychef");
    let original_bindings = std::collections::BTreeMap::from([
        (
            "custom:Fox".into(),
            HatReference::Known {
                hat_id: "hat:Santa".into(),
            },
        ),
        (
            "native:Chef_Black_cat".into(),
            HatReference::Known {
                hat_id: "hat:None".into(),
            },
        ),
    ]);
    let original = storage::write(
        &game,
        &mod_root,
        "target",
        &bindings_dir,
        &storage::read(&game, &mod_root, "target", &bindings_dir)
            .unwrap()
            .source
            .revision,
        b"Fox HAT=Santa\r\nChef_Black_cat HAT=None\r\n",
        &original_bindings,
        true,
    )
    .unwrap();
    let deleted = storage::write(
        &game,
        &mod_root,
        "target",
        &bindings_dir,
        &original.source.revision,
        b"Chef_Black_cat HAT=None\r\n",
        &std::collections::BTreeMap::from([(
            "native:Chef_Black_cat".into(),
            HatReference::Known {
                hat_id: "hat:None".into(),
            },
        )]),
        false,
    )
    .unwrap();

    restore_deleted_config(
        &Session {
            id: "session".into(),
            game_root: game.clone(),
            game_settings: GameConfig {
                path: game.to_string_lossy().into_owned(),
                runtime: crate::models::GameRuntime::Native,
            },
            mod_root: mod_root.clone(),
            target: ChefTarget {
                id: "target".into(),
                guid: "test".into(),
                version: Some("2.2.0".into()),
                display_path: "fixture".into(),
                enabled: true,
                generation: 1,
                compatibility: "fastinit_2_2".into(),
                supports_custom_default: true,
                supports_live_save: false,
            },
            catalog: catalog::read(&game, &mod_root).unwrap(),
            resources_revision: 0,
            stored: original.clone(),
            bindings_dir: bindings_dir.clone(),
        },
        &deleted.source.revision,
        &original,
    )
    .unwrap();

    let restored = storage::read(&game, &mod_root, "target", &bindings_dir).unwrap();
    assert_eq!(
        restored.document.text(),
        "Fox HAT=Santa\r\nChef_Black_cat HAT=None\r\n"
    );
    assert_eq!(restored.hat_bindings, original_bindings);
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn deleted_resource_directory_recycles_files_and_updates_owned_manifests() {
    let (game, mod_root) = workspace();
    let resource = mod_root.join("Resources/Fox");
    add_custom(&mod_root.join("Resources"), "Fox", 7);
    let manifests = game.join("manager/manifests");
    fs::create_dir_all(&manifests).unwrap();
    let file = "BepInEx/plugins/DIYChef/Resources/Fox/INFO";
    crate::archive::write_manifest(
        &manifests,
        &crate::archive::InstallManifest {
            repository_id: "chef-package".into(),
            release_tag: "v1".into(),
            repository_binding: None,
            package_name_i18n: Default::default(),
            note_i18n: Default::default(),
            group_i18n: Default::default(),
            prerelease: false,
            relations: vec![],
            installed_at: "2026-11-16T00:00:00Z".into(),
            files: vec![file.into()],
        },
    )
    .unwrap();
    let recycle = game.join("manager/recycle-bin");

    let deleted =
        recycle_chef_resource(&recycle, &manifests, &game, &mod_root, &resource, "Fox").unwrap();
    let DeletedResource::Recycled(id) = deleted else {
        panic!("nonempty resource tree must use the recycle journal");
    };

    assert!(!resource.exists());
    let active_path = manifests.join("chef-package.toml");
    assert!(!active_path.exists());
    crate::recycle_bin::restore(&recycle, &manifests, &game, &id).unwrap();
    assert_eq!(fs::read(resource.join("INFO")).unwrap(), b"ID=7\n");
    let active: crate::archive::InstallManifest =
        toml::from_str(&fs::read_to_string(&active_path).unwrap()).unwrap();
    assert_eq!(active.files, [file]);
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn empty_resource_tree_uses_adjacent_staging_and_can_be_restored() {
    let (game, mod_root) = workspace();
    let resource = mod_root.join("Resources/Empty");
    fs::create_dir_all(&resource).unwrap();
    let deleted = recycle_chef_resource(
        &game.join("manager/recycle-bin"),
        &game.join("manager/manifests"),
        &game,
        &mod_root,
        &resource,
        "Empty",
    )
    .unwrap();
    let DeletedResource::EmptyTree(staged) = deleted else {
        panic!("empty resource tree must use adjacent staging");
    };
    assert!(!resource.exists());
    restore_resource(
        &game.join("manager/recycle-bin"),
        &game.join("manager/manifests"),
        &game,
        &resource,
        &DeletedResource::EmptyTree(staged),
    )
    .unwrap();
    assert!(resource.is_dir());
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn failed_resource_restore_rolls_the_deleted_prefer_and_bindings_back() {
    let (game, mod_root) = workspace();
    let resources = mod_root.join("Resources");
    add_custom(&resources, "Fox", 7);
    let bindings_dir = game.join("manager/diychef");
    let original_bindings = std::collections::BTreeMap::from([(
        "custom:Fox".into(),
        HatReference::Known {
            hat_id: "hat:Santa".into(),
        },
    )]);
    let original = storage::write(
        &game,
        &mod_root,
        "target",
        &bindings_dir,
        &storage::read(&game, &mod_root, "target", &bindings_dir)
            .unwrap()
            .source
            .revision,
        b"Fox HAT=Santa\nChef_Black_cat HAT=None\n",
        &original_bindings,
        true,
    )
    .unwrap();
    let deleted = storage::write(
        &game,
        &mod_root,
        "target",
        &bindings_dir,
        &original.source.revision,
        b"Chef_Black_cat HAT=None\n",
        &std::collections::BTreeMap::new(),
        false,
    )
    .unwrap();
    let session = Session {
        id: "session".into(),
        game_root: game.clone(),
        game_settings: GameConfig {
            path: game.to_string_lossy().into_owned(),
            runtime: crate::models::GameRuntime::Native,
        },
        mod_root: mod_root.clone(),
        target: ChefTarget {
            id: "target".into(),
            guid: "test".into(),
            version: Some("2.2.0".into()),
            display_path: "fixture".into(),
            enabled: true,
            generation: 1,
            compatibility: "fastinit_2_2".into(),
            supports_custom_default: true,
            supports_live_save: false,
        },
        catalog: catalog::read(&game, &mod_root).unwrap(),
        resources_revision: 0,
        stored: deleted.clone(),
        bindings_dir: bindings_dir.clone(),
    };
    let state = AppState::with_config(crate::config::ConfigService::for_test(game.join("manager")));
    let recycle_root = state.config.mod_recycle_bin_dir();
    let resource = resources.join("Fox");
    let deleted_resource = recycle_chef_resource(
        &recycle_root,
        &state.config.manifests_dir(),
        &game,
        &mod_root,
        &resource,
        "Fox",
    )
    .unwrap();
    fs::create_dir_all(&resource).unwrap();
    fs::write(resource.join("INFO"), b"conflict").unwrap();

    assert_eq!(
        restore_deleted_resource(
            &state,
            &session,
            &resource,
            &deleted_resource,
            &deleted.source.revision,
            &original,
        )
        .unwrap_err(),
        "diychef.writtenReadFailed"
    );
    let current = storage::read(&game, &mod_root, "target", &bindings_dir).unwrap();
    assert_eq!(current.document.text(), "Chef_Black_cat HAT=None\n");
    assert!(current.hat_bindings.is_empty());
    assert_eq!(fs::read(resource.join("INFO")).unwrap(), b"conflict");
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn session_commit_recovers_a_poisoned_session_mutex() {
    let (game, mod_root) = workspace();
    let state = AppState::with_config(crate::config::ConfigService::for_test(game.join("manager")));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _sessions = state.diychef_sessions.lock().unwrap();
        panic!("poison fixture");
    }));
    let stored = storage::read(&game, &mod_root, "target", &game.join("bindings")).unwrap();
    let session = Session {
        id: "session".into(),
        game_root: game.clone(),
        game_settings: GameConfig {
            path: game.to_string_lossy().into_owned(),
            runtime: crate::models::GameRuntime::Native,
        },
        mod_root,
        target: ChefTarget {
            id: "target".into(),
            guid: "test".into(),
            version: Some("2.2.0".into()),
            display_path: "fixture".into(),
            enabled: true,
            generation: 1,
            compatibility: "fastinit_2_2".into(),
            supports_custom_default: true,
            supports_live_save: false,
        },
        catalog: catalog::Catalog {
            chefs: vec![],
            hats: vec![],
            revision: String::new(),
        },
        resources_revision: 0,
        stored,
        bindings_dir: game.join("bindings"),
    };

    commit_deleted_session(&state, "session", session);
    assert!(
        state
            .diychef_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values
            .contains_key("session")
    );
    fs::remove_dir_all(game).unwrap();
}

#[test]
fn deletion_is_blocked_when_it_removes_native_fallback_or_legacy_first_row() {
    let (game, mod_root) = workspace();
    add_custom(&mod_root.join("Resources"), "Fox", 7);
    add_custom(&mod_root.join("Resources"), "Wolf", 8);
    let catalog = catalog::read(&game, &mod_root).unwrap();
    let custom_id = catalog
        .chefs
        .iter()
        .find(|chef| chef.config_name == "Fox")
        .unwrap()
        .id
        .clone();
    let bindings_dir = game.join("manager/diychef");
    let target = ChefTarget {
        id: "target".into(),
        guid: "test".into(),
        version: Some("2.2.0".into()),
        display_path: "fixture".into(),
        enabled: true,
        generation: 1,
        compatibility: "fastinit_2_2".into(),
        supports_custom_default: true,
        supports_live_save: false,
    };
    let mut session = Session {
        id: "session".into(),
        game_root: game.clone(),
        game_settings: GameConfig {
            path: game.to_string_lossy().into_owned(),
            runtime: crate::models::GameRuntime::Native,
        },
        mod_root: mod_root.clone(),
        target,
        catalog,
        resources_revision: 0,
        stored: storage::read(&game, &mod_root, "target", &bindings_dir).unwrap(),
        bindings_dir,
    };
    let custom_only = b"Fox HAT=None\n";
    session.stored = storage::write(
        &game,
        &mod_root,
        "target",
        &session.bindings_dir,
        &session.stored.source.revision,
        custom_only,
        &std::collections::BTreeMap::new(),
        true,
    )
    .unwrap();
    assert_eq!(
        deletion_plan(&session, &custom_id).unwrap_err(),
        validation::NATIVE_FALLBACK_REQUIRED
    );

    session.target.supports_custom_default = false;
    let legacy = storage::write(
        &game,
        &mod_root,
        "target",
        &session.bindings_dir,
        &session.stored.source.revision,
        b"Wolf HAT=None\nFox HAT=None\nChef_Black_cat HAT=None\n",
        &std::collections::BTreeMap::new(),
        false,
    )
    .unwrap();
    session.stored = legacy;
    assert_eq!(
        deletion_plan(&session, &custom_id).unwrap_err(),
        validation::NATIVE_DEFAULT_REQUIRED
    );
    fs::remove_dir_all(game).unwrap();
}
