use super::*;
use crate::{config::ConfigService, models::InstalledMod};
use std::fs;

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("oc2-chef-session-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(p.join("game/BepInEx/plugins/DIYChef")).unwrap();
        fs::create_dir_all(p.join("game/Overcooked2.app/Contents/MacOS")).unwrap();
        fs::write(p.join("game/Overcooked2.exe"), b"fixture").unwrap();
        fs::write(
            p.join("game/BepInEx/plugins/DIYChef/DIYChef.dll"),
            b"fixture",
        )
        .unwrap();
        Self(p)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let p = self.0.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(p.starts_with(&temp) && p != temp);
        fs::remove_dir_all(p).unwrap();
    }
}
#[test]
fn editor_requires_completed_backend_generation_and_invalidates_existing_sessions() {
    let workspace = Workspace::new();
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.0.join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    let installed: InstalledMod = serde_json::from_value(serde_json::json!({
        "name": "DIYChefFastInit", "guid": target::GUID, "version": "2.2.0",
        "dll_file": "BepInEx/plugins/DIYChef/DIYChef.dll", "enabled": true,
        "version_source": "plugin", "update_available": false,
        "duplicate_guid": false, "status": "unknown"
    }))
    .unwrap();
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    assert_eq!(
        open(&state, generation, None).err().unwrap(),
        "diychef.staleScan"
    );
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), vec![installed])
        .unwrap();
    let opened = open(&state, generation, None).unwrap().snapshot.unwrap();
    assert_eq!(opened.source.state, "missing");
    assert_eq!(opened.selection.entries.len(), 1);
    assert!(opened.selection.lobby_switch);
    assert!(
        !workspace
            .0
            .join("game/BepInEx/plugins/DIYChef/prefer.txt")
            .exists(),
        "opening never initializes on disk"
    );
    let session = get(&state, &opened.session_id).unwrap();
    let mut draft = baseline_draft(&session);
    assert_eq!(
        save(&state, &opened.session_id, &draft).err().unwrap(),
        "diychef.conflict",
        "save cannot replace the separate missing-file initialization baseline"
    );
    draft.target_generation += 1;
    assert_eq!(
        validate(&state, &opened.session_id, &draft).err().unwrap(),
        "diychef.conflict"
    );
    let chef = opened
        .chefs
        .iter()
        .find(|chef| chef.source_kind == "native")
        .unwrap();
    let hat = opened
        .hats
        .iter()
        .find(|hat| hat.config_value == "None")
        .unwrap();
    let preview = || resolve_preview_context(&state, &opened.session_id, &chef.id, &hat.id);
    assert_eq!(preview().unwrap().game_root, root);
    require_session(&state, &opened.session_id).unwrap();
    let mut changed = state.config.load().unwrap();
    changed.game.path = workspace
        .0
        .join("different-game")
        .to_string_lossy()
        .into_owned();
    state.config.save(&changed).unwrap();
    assert_eq!(preview().unwrap_err(), "diychef.targetChanged");
    state.config.save(&config).unwrap();
    let dll = workspace.0.join("game/BepInEx/plugins/DIYChef/DIYChef.dll");
    let moved = dll.with_extension("dll.disabled");
    fs::rename(&dll, &moved).unwrap();
    assert!(
        preview().is_err(),
        "an unchanged scan cannot authorize a missing/disabled target DLL"
    );
    fs::rename(&moved, &dll).unwrap();
    assert!(preview().is_ok());
    state.invalidate_local_mod_snapshot().unwrap();
    assert_eq!(preview().unwrap_err(), "diychef.staleScan");
    assert_eq!(
        require_session(&state, &opened.session_id).unwrap_err(),
        "diychef.staleScan"
    );
    assert_eq!(
        get(&state, &opened.session_id).err().unwrap(),
        "diychef.staleScan"
    );
    assert_eq!(
        open(&state, generation, None).err().unwrap(),
        "diychef.staleScan"
    );
}

#[test]
fn missing_and_initialized_defaults_enable_lobby_switch_without_changing_present_files() {
    let workspace = Workspace::new();
    let root = workspace.0.join("game").canonicalize().unwrap();
    let mod_root = root.join("BepInEx/plugins/DIYChef");
    let mut session = Session {
        id: "default-session".into(),
        game_root: root.clone(),
        game_settings: GameConfig {
            path: root.to_string_lossy().into_owned(),
            runtime: GameRuntime::Native,
        },
        mod_root: mod_root.clone(),
        target: ChefTarget {
            id: "default-target".into(),
            guid: target::GUID.into(),
            version: Some("2.2.0".into()),
            display_path: "fixture".into(),
            enabled: true,
            generation: 1,
            compatibility: "fastinit_2_2".into(),
            supports_custom_default: true,
        },
        catalog: catalog::read(&root, &mod_root).unwrap(),
        stored: storage::read(
            &root,
            &mod_root,
            "default-target",
            &workspace.0.join("manager/diychef"),
        )
        .unwrap(),
        bindings_dir: workspace.0.join("manager/diychef"),
    };
    let missing = view(&session).unwrap();
    assert_eq!(missing.source.state, "missing");
    assert!(missing.selection.lobby_switch);
    assert_eq!(missing.selection.entries.len(), 1);
    assert_eq!(missing.selection.entries[0].row_id, None);
    assert!(matches!(
        &missing.selection.entries[0].chef_ref,
        ChefReference::Known { chef_id } if chef_id == "native:Chef_Black_cat"
    ));
    let path = mod_root.join("prefer.txt");
    assert!(!path.exists(), "view never writes the missing default");
    session.stored = storage::write(
        &root,
        &mod_root,
        &session.target.id,
        &session.bindings_dir,
        &missing.source.revision,
        &mutations::initialization_plan(&session).unwrap().0,
        &missing.selection.hat_bindings,
        true,
    )
    .unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"Chef_Black_cat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n"
    );
    assert_eq!(prefer::DEFAULT_CONTENT.len(), 40);
    let initialized = view(&session).unwrap();
    assert_eq!(initialized.source.state, "present");
    assert_eq!(initialized.source.encoding, "utf8");
    assert!(initialized.selection.lobby_switch);
    let saved_hats = std::collections::BTreeMap::from([
        (
            "native:Chef_Black_cat".into(),
            HatReference::Known {
                hat_id: "hat:None".into(),
            },
        ),
        (
            "native:Chef_Beard".into(),
            HatReference::Known {
                hat_id: "hat:None".into(),
            },
        ),
    ]);
    session.stored = storage::write(
        &root,
        &mod_root,
        &session.target.id,
        &session.bindings_dir,
        &session.stored.source.revision,
        prefer::DEFAULT_CONTENT,
        &saved_hats,
        false,
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
    session.stored =
        storage::read(&root, &mod_root, &session.target.id, &session.bindings_dir).unwrap();
    let missing_with_hats = view(&session).unwrap();
    assert_eq!(missing_with_hats.selection.hat_bindings, saved_hats);
    let (bytes, bindings) = mutations::initialization_plan(&session).unwrap();
    assert_eq!(
        bytes,
        b"Chef_Black_cat HAT=None\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n"
    );
    session.stored = storage::write(
        &root,
        &mod_root,
        &session.target.id,
        &session.bindings_dir,
        &session.stored.source.revision,
        &bytes,
        &bindings,
        true,
    )
    .unwrap();
    assert_eq!(view(&session).unwrap().selection.hat_bindings, saved_hats);
    for (original, lobby_switch) in [
        (b"".as_slice(), false),
        (b"Chef_Black_cat\r\n", false),
        (b"Chef_Black_cat\nLOBBYSWITCHCHEF=FALSE\n", false),
        (b"LOBBYSWITCHCHEF=TRUE\r\nChef_Black_cat\r\n", true),
        (
            b"LOBBYSWITCHCHEF=TRUE\nChef_Black_cat\nLOBBYSWITCHCHEF=TRUE",
            true,
        ),
    ] {
        fs::write(&path, original).unwrap();
        session.stored =
            storage::read(&root, &mod_root, &session.target.id, &session.bindings_dir).unwrap();
        let present = view(&session).unwrap();
        assert_eq!(present.source.state, "present");
        assert_eq!(present.selection.lobby_switch, lobby_switch);
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "view preserves present bytes"
        );
    }
}

#[test]
fn ambiguous_picker_exposes_only_targets_that_can_be_selected() {
    let workspace = Workspace::new();
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.0.join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    let installed: Vec<InstalledMod> = [
        ("latest-a", Some("2.2.0")),
        ("latest-b", Some("2.2.0")),
        ("older", Some("2.1.0")),
    ]
    .into_iter()
    .map(|(id, version)| {
        serde_json::from_value(serde_json::json!({
            "name": "DIYChefFastInit", "guid": target::GUID, "version": version,
            "dll_file": id, "enabled": true,
            "version_source": "plugin", "update_available": false,
            "duplicate_guid": false, "status": "unknown"
        }))
        .unwrap()
    })
    .collect();
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), installed.clone())
        .unwrap();
    let opened = open(&state, generation, None).unwrap();
    assert_eq!(opened.status, "select_target");
    assert!(opened.snapshot.is_none());
    assert_eq!(
        opened
            .candidates
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["latest-a", "latest-b"]
    );
    assert_eq!(
        open(&state, generation, Some("older")).err().unwrap(),
        "diychef.targetChanged"
    );
    let mut unknown = installed;
    unknown[0].version = None;
    let next = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(next, Some(&root), unknown)
        .unwrap();
    let reopened = open(&state, next, None).unwrap();
    assert_eq!(reopened.status, "select_target");
    assert_eq!(reopened.candidates.len(), 3);
}

/// Service-layer integration: real plugin bytes are scanned, but the executable
/// is only a directory-discovery fixture. No game process or real prefer is used.
#[test]
#[ignore = "requires installed real DIYChef DLL/resources; writes only UUID temp game directory"]
fn real_dll_session_initialize_edit_save_readback_and_external_conflict() {
    let source = std::env::var_os("OC2_DIYCHEF_FIXTURE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYChef-FastInit")
    });
    assert!(
        source.join("DIYChefFastInit.dll").is_file(),
        "set OC2_DIYCHEF_FIXTURE_DIR to the installed real plugin directory"
    );
    let workspace = Workspace::new();
    let destination = workspace.0.join("game/BepInEx/plugins/DIYChef");
    fs::copy(
        source.join("DIYChefFastInit.dll"),
        destination.join("DIYChef.dll"),
    )
    .unwrap();
    for relative in [
        "Resources/00-Fox/INFO",
        "Resources/00-Fox/t_Head.png",
        "Resources/00-Fox/m_Head.txt",
        "Resources/HATS/Hardhat/t_Hardhat.png",
        "Resources/HATS/Hardhat/m_Hardhat.txt",
    ] {
        let to = destination.join(relative);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(source.join(relative), &to).unwrap();
    }
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.0.join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    // Use the production scanner rather than constructing InstalledMod metadata.
    let installed =
        crate::mods::scan_mods_with_manifests(&root, &config, Some(&state.config.manifests_dir()));
    let matched = installed
        .iter()
        .find(|m| m.guid.as_deref() == Some(target::GUID))
        .expect("real DLL must be identified by metadata");
    assert_eq!(matched.version.as_deref(), Some("2.2.0"));
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), installed)
        .unwrap();
    let opened = open(&state, generation, None).unwrap().snapshot.unwrap();
    assert_eq!(opened.target.compatibility, "editable");
    assert_eq!(opened.source.state, "missing");
    assert!(opened.selection.lobby_switch);
    assert!(!destination.join("prefer.txt").exists());
    let initialized = initialize(&state, &opened.session_id, &opened.source.revision).unwrap();
    assert_eq!(
        fs::read(destination.join("prefer.txt")).unwrap(),
        b"Chef_Black_cat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n"
    );
    assert_eq!(initialized.source.state, "present");
    assert!(initialized.selection.lobby_switch);
    let session = get(&state, &opened.session_id).unwrap();
    let mut draft = baseline_draft(&session);
    draft.hat_bindings.insert(
        "native:Chef_Black_cat".into(),
        HatReference::Known {
            hat_id: "hat:None".into(),
        },
    );
    draft.hat_bindings.insert(
        "custom:00-Fox".into(),
        HatReference::Known {
            hat_id: "hat:Hardhat".into(),
        },
    );
    draft.entries.insert(
        0,
        ChefEntry {
            entry_id: "added:fox".into(),
            row_id: None,
            chef_ref: ChefReference::Known {
                chef_id: "custom:00-Fox".into(),
            },
            hat_ref: HatReference::Known {
                hat_id: "hat:Hardhat".into(),
            },
        },
    );
    draft.lobby_switch = true;
    assert!(opened.target.supports_custom_default);
    assert!(
        !validate(&state, &opened.session_id, &draft)
            .unwrap()
            .save_blocking
    );
    let mut custom_only = draft.clone();
    custom_only.entries.truncate(1);
    assert!(
        !validate(&state, &opened.session_id, &custom_only)
            .unwrap()
            .save_blocking
    );
    assert_eq!(
        fs::read(destination.join("prefer.txt")).unwrap(),
        b"Chef_Black_cat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n",
        "validation alone never writes to disk"
    );
    draft.entries.swap(0, 1);
    let validated = validate(&state, &opened.session_id, &draft).unwrap();
    assert!(!validated.save_blocking, "{:?}", validated.diagnostics);
    draft.acknowledged_change_ids = validated
        .changes
        .into_iter()
        .map(|change| change.id)
        .collect();
    let saved = save(&state, &opened.session_id, &draft).unwrap();
    let expected = b"Chef_Black_cat HAT=None\r\n00-Fox HAT=Hardhat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n";
    assert_eq!(fs::read(destination.join("prefer.txt")).unwrap(), expected);
    assert_eq!(saved.selection.entries.len(), 2);
    assert!(saved.selection.lobby_switch);
    assert!(
        matches!(&saved.selection.entries[1].chef_ref, ChefReference::Known {chef_id} if chef_id == "custom:00-Fox")
    );
    assert!(
        matches!(&saved.selection.entries[1].hat_ref, HatReference::Known {hat_id} if hat_id == "hat:Hardhat")
    );
    let reopened = open(&state, generation, None).unwrap().snapshot.unwrap();
    assert_eq!(reopened.source.revision, saved.source.revision);
    assert!(reopened.selection.lobby_switch);
    assert!(
        matches!(&reopened.selection.entries[0].chef_ref, ChefReference::Known {chef_id} if chef_id == "native:Chef_Black_cat")
    );
    // Only the disabled chef's map changes; a new manager instance must recover
    // it independently of the unchanged TXT entries and stale entry hat_ref.
    let mut disabled = baseline_draft(&get(&state, &reopened.session_id).unwrap());
    disabled.entries.retain(|e| matches!(&e.chef_ref, ChefReference::Known { chef_id } if chef_id == "native:Chef_Black_cat"));
    let disabled_saved = save(&state, &reopened.session_id, &disabled).unwrap();
    let mut map_only = baseline_draft(&get(&state, &disabled_saved.session_id).unwrap());
    map_only.hat_bindings.insert(
        "custom:00-Fox".into(),
        HatReference::Known {
            hat_id: "hat:None".into(),
        },
    );
    save(&state, &disabled_saved.session_id, &map_only).unwrap();
    let disabled_bytes = fs::read(destination.join("prefer.txt")).unwrap();
    assert!(!String::from_utf8_lossy(&disabled_bytes).contains("00-Fox"));
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let installed = crate::mods::scan_mods_with_manifests(
        &root,
        &state.config.load().unwrap(),
        Some(&state.config.manifests_dir()),
    );
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), installed)
        .unwrap();
    let restored = open(&state, generation, None).unwrap().snapshot.unwrap();
    assert!(
        matches!(restored.selection.hat_bindings.get("custom:00-Fox"), Some(HatReference::Known { hat_id }) if hat_id == "hat:None")
    );
    assert_eq!(restored.selection.entries.len(), 1);
    let mut enabled = baseline_draft(&get(&state, &restored.session_id).unwrap());
    enabled.entries.push(ChefEntry {
        entry_id: "restored:fox".into(),
        row_id: None,
        chef_ref: ChefReference::Known {
            chef_id: "custom:00-Fox".into(),
        },
        hat_ref: HatReference::Known {
            hat_id: "hat:Hardhat".into(),
        },
    });
    let enabled_saved = save(&state, &restored.session_id, &enabled).unwrap();
    assert!(
        String::from_utf8_lossy(&fs::read(destination.join("prefer.txt")).unwrap())
            .contains("00-Fox HAT=None")
    );
    assert!(
        get(&state, &enabled_saved.session_id)
            .unwrap()
            .stored
            .bindings_path
            .starts_with(
                state
                    .config
                    .config_path()
                    .parent()
                    .unwrap()
                    .canonicalize()
                    .unwrap()
            )
    );
    // Deleting TXT must not let automatic initialization erase saved None hats.
    fs::remove_file(destination.join("prefer.txt")).unwrap();
    let missing_again = open(&state, generation, None).unwrap().snapshot.unwrap();
    let reinitialized = initialize(
        &state,
        &missing_again.session_id,
        &missing_again.source.revision,
    )
    .unwrap();
    assert!(
        String::from_utf8_lossy(&fs::read(destination.join("prefer.txt")).unwrap())
            .starts_with("Chef_Black_cat HAT=None\r\n")
    );
    for id in ["native:Chef_Black_cat", "custom:00-Fox"] {
        assert!(
            matches!(reinitialized.selection.hat_bindings.get(id), Some(HatReference::Known { hat_id }) if hat_id == "hat:None")
        );
    }
    let historical = b"00-Fox HAT=Hardhat\r\nChef_Black_cat HAT=None\r\n";
    fs::write(destination.join("prefer.txt"), historical).unwrap();
    let old = open(&state, generation, None).unwrap().snapshot.unwrap();
    assert!(!old.selection.lobby_switch);
    assert!(
        matches!(&old.selection.entries[0].chef_ref, ChefReference::Known {chef_id} if chef_id == "custom:00-Fox")
    );
    assert!(!old.diagnostics.iter().any(|problem| problem.code
        == validation::NATIVE_DEFAULT_REQUIRED
        && problem.save_blocking));
    assert_eq!(
        fs::read(destination.join("prefer.txt")).unwrap(),
        historical
    );
    let historical_draft = baseline_draft(&get(&state, &old.session_id).unwrap());
    let historical_saved = save(&state, &old.session_id, &historical_draft).unwrap();
    assert!(
        matches!(&historical_saved.selection.entries[0].chef_ref, ChefReference::Known { chef_id } if chef_id == "custom:00-Fox")
    );
    assert_eq!(
        fs::read(destination.join("prefer.txt")).unwrap(),
        historical,
        "reading or saving an unchanged custom default preserves it byte-for-byte"
    );
    let mut edited = baseline_draft(&get(&state, &old.session_id).unwrap());
    edited.lobby_switch = false;
    let external = b"Chef_Beard HAT=Fancy\n";
    fs::write(destination.join("prefer.txt"), external).unwrap();
    assert_eq!(
        save(&state, &old.session_id, &edited).err().unwrap(),
        "diychef.conflict"
    );
    assert_eq!(fs::read(destination.join("prefer.txt")).unwrap(), external);
    println!(
        "E1 real DLL scanned; init + ordered hats + disabled/map-only save + manager restart + enable + deleted TXT reinit + external conflict passed ({} bytes saved)",
        expected.len()
    );
}
