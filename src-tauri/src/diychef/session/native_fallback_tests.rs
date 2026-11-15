use super::tests::Workspace;
use super::*;
use crate::{config::ConfigService, models::InstalledMod};
use std::fs;

#[test]
fn imported_custom_only_selection_is_readable_but_save_and_txt_generation_are_hard_blocked() {
    let workspace = Workspace::new();
    let game = workspace.0.join("game");
    let directory = game.join("BepInEx/plugins/DIYChef");
    let custom = directory.join("Resources/Fox");
    fs::create_dir_all(&custom).unwrap();
    fs::write(
        directory.join("DIYChef.dll"),
        crate::mods::plugin_blob(target::GUID, "DIYChef", "2.2.0"),
    )
    .unwrap();
    fs::write(custom.join("INFO"), b"ID=7\n").unwrap();
    fs::write(custom.join("t_Head.png"), b"metadata-only").unwrap();
    fs::write(custom.join("m_Head.txt"), b"metadata-only").unwrap();
    let original = b"Fox HAT=None\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n";
    fs::write(directory.join("prefer.txt"), original).unwrap();
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = game.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    let installed: InstalledMod = serde_json::from_value(serde_json::json!({
        "name": "DIYChef", "guid": target::GUID, "version": "2.2.0", "dll_file": "BepInEx/plugins/DIYChef/DIYChef.dll",
        "enabled": true, "version_source": "BepInPlugin", "update_available": false, "duplicate_guid": false, "status": "unknown"
    })).unwrap();
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), vec![installed])
        .unwrap();
    let opened = open(&state, generation, None, |_| 0)
        .unwrap()
        .snapshot
        .unwrap();
    assert!(opened.target.supports_custom_default);
    assert!(opened.diagnostics.iter().any(|problem| problem.code
        == validation::NATIVE_FALLBACK_REQUIRED
        && problem.save_blocking));
    let session = get(&state, &opened.session_id).unwrap();
    let draft = baseline_draft(&session);
    assert!(
        validate(&state, &opened.session_id, &draft)
            .unwrap()
            .save_blocking
    );
    assert_eq!(
        prefer::plan(&session.stored.document, &draft, &session.catalog).unwrap_err(),
        validation::NATIVE_FALLBACK_REQUIRED
    );
    assert_eq!(
        save(&state, &opened.session_id, &draft).unwrap_err(),
        validation::NATIVE_FALLBACK_REQUIRED
    );
    assert_eq!(fs::read(directory.join("prefer.txt")).unwrap(), original);
    assert!(
        !state
            .config
            .config_path()
            .with_file_name("diychef")
            .exists(),
        "a rejected save cannot create manager bindings"
    );
    let mut repaired = draft.clone();
    repaired.entries.push(ChefEntry {
        entry_id: "native-fallback".into(),
        row_id: None,
        chef_ref: ChefReference::Known {
            chef_id: "native:Chef_Black_cat".into(),
        },
        hat_ref: HatReference::Known {
            hat_id: "hat:None".into(),
        },
    });
    let checked = validate(&state, &opened.session_id, &repaired).unwrap();
    assert!(!checked.save_blocking);
    repaired.acknowledged_change_ids = checked
        .changes
        .iter()
        .map(|change| change.id.clone())
        .collect();
    let saved = save(&state, &opened.session_id, &repaired).unwrap();
    assert!(
        matches!(&saved.selection.entries[0].chef_ref, ChefReference::Known { chef_id } if chef_id == "custom:Fox")
    );
    assert!(saved.selection.lobby_switch);
    assert!(
        String::from_utf8_lossy(&fs::read(directory.join("prefer.txt")).unwrap())
            .starts_with("Fox HAT=None\r\n")
    );
}
