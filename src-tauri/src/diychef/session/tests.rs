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
        stored: storage::read(&root, &mod_root, "default-target").unwrap(),
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
        &missing.source.revision,
        prefer::DEFAULT_CONTENT,
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
        session.stored = storage::read(&root, &mod_root, &session.target.id).unwrap();
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
    assert_eq!(opened.target.compatibility, "fastinit_2_2");
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
    draft.entries[0].hat_ref = HatReference::Known {
        hat_id: "hat:None".into(),
    };
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
    let mut edited = baseline_draft(&get(&state, &opened.session_id).unwrap());
    edited.lobby_switch = false;
    let external = b"Chef_Beard HAT=Fancy\n";
    fs::write(destination.join("prefer.txt"), external).unwrap();
    assert_eq!(
        save(&state, &opened.session_id, &edited).err().unwrap(),
        "diychef.conflict"
    );
    assert_eq!(fs::read(destination.join("prefer.txt")).unwrap(), external);
    println!(
        "E1 real DLL scanned; init + ordered Fox/Hardhat/None/lobby save + reopen + external conflict passed ({} bytes saved)",
        expected.len()
    );
}
