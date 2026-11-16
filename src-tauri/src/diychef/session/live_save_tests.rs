use super::tests::Workspace;
use super::*;
use crate::config::ConfigService;
use std::fs;

#[test]
fn live_save_target_roundtrips_config_and_rejects_replaced_dll_or_stale_revision() {
    let workspace = Workspace::new();
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load_or_default();
    config.game.path = workspace.0.join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    let mod_root = root.join("BepInEx/plugins/DIYChef");
    let dll = mod_root.join("DIYChef.dll");
    fs::write(
        &dll,
        crate::mods::plugin_blob(target::GUID, "DIYChef", "2.4.0"),
    )
    .unwrap();
    let installed = crate::mods::scan_mods_with_manifests(&root, &config, None);
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), installed)
        .unwrap();
    let opened = open(&state, generation, None, |_| 0)
        .unwrap()
        .snapshot
        .unwrap();
    assert!(opened.target.supports_live_save);
    let initialized = initialize(&state, &opened.session_id, &opened.source.revision).unwrap();
    assert_eq!(initialized.source.state, "present");

    let mut draft = baseline_draft(&get(&state, &opened.session_id).unwrap());
    draft.lobby_switch = false;
    let saved = save(&state, &opened.session_id, &draft).unwrap();
    assert!(!saved.selection.lobby_switch);
    let expected = fs::read(mod_root.join("prefer.txt")).unwrap();
    let mut draft = baseline_draft(&get(&state, &opened.session_id).unwrap());
    draft.lobby_switch = true;
    // The live-save flag belongs to the scanned DLL, not the old open session.
    fs::write(
        &dll,
        crate::mods::plugin_blob(target::GUID, "DIYChef", "2.5.0"),
    )
    .unwrap();
    assert_eq!(
        save(&state, &opened.session_id, &draft).unwrap_err(),
        "diychef.targetChanged"
    );
    assert_eq!(
        initialize(&state, &opened.session_id, &saved.source.revision).unwrap_err(),
        "diychef.targetChanged"
    );
    assert_eq!(fs::read(mod_root.join("prefer.txt")).unwrap(), expected);

    fs::write(
        &dll,
        crate::mods::plugin_blob(target::GUID, "DIYChef", "2.4.0"),
    )
    .unwrap();
    fs::write(mod_root.join("prefer.txt"), b"Chef_Black_cat HAT=None\r\n").unwrap();
    assert_eq!(
        save(&state, &opened.session_id, &draft).unwrap_err(),
        "diychef.conflict"
    );
    assert_eq!(
        fs::read(mod_root.join("prefer.txt")).unwrap(),
        b"Chef_Black_cat HAT=None\r\n"
    );
}
