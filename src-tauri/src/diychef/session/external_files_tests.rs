use super::tests::Workspace;
use super::*;
use crate::config::ConfigService;
use std::fs;
use std::{sync::mpsc, time::Duration};

#[test]
fn real_resource_content_notification_rotates_session_and_rechecks_preview_without_catalog_change()
{
    let _cache = super::super::preview::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let workspace = Workspace::new();
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.0.join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    let mod_root = root.join("BepInEx/plugins/DIYChef");
    let resource = mod_root.join("Resources/Fox");
    fs::create_dir_all(&resource).unwrap();
    fs::write(resource.join("INFO"), "ID=7\n").unwrap();
    fs::write(resource.join("t_Head.png"), b"first texture").unwrap();
    fs::write(resource.join("m_Head.txt"), b"fixture material").unwrap();
    let prefer = mod_root.join("prefer.txt");
    fs::write(&prefer, b"Chef_Black_cat\nFox HAT=None\n").unwrap();
    let installed = serde_json::from_value(serde_json::json!({
        "name":"DIYChef", "guid":target::GUID, "version":"2.2.0", "dll_file":"BepInEx/plugins/DIYChef/DIYChef.dll",
        "enabled":true, "version_source":"plugin", "update_available":false, "duplicate_guid":false, "status":"unknown"
    })).unwrap();
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), vec![installed])
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let subscribe = |selected: Option<(&Path, &ChefTarget)>| {
        let (root, target) = selected.unwrap();
        let tx = tx.clone();
        state.diychef_watch.watch(root, target, move |event| {
            tx.send(event).unwrap();
        })
    };
    let first = open(&state, generation, None, &subscribe)
        .unwrap()
        .snapshot
        .unwrap();
    let fingerprint = |session: &str| {
        super::super::preview::fingerprint(&state, session, "custom:Fox", "hat:None").unwrap()
    };
    let old_fingerprint = fingerprint(&first.session_id);
    fs::write(
        resource.join("t_Head.png"),
        b"replacement texture with another size",
    )
    .unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    let changed = open(&state, generation, None, &subscribe)
        .unwrap()
        .snapshot
        .unwrap();
    assert_eq!(first.catalog_revision, changed.catalog_revision);
    assert_eq!(first.source.revision, changed.source.revision);
    assert_ne!(first.session_id, changed.session_id);
    assert_ne!(old_fingerprint, fingerprint(&changed.session_id));
    // A later prefer-only save echo must keep this preview session alive.
    fs::write(&prefer, fs::read(&prefer).unwrap()).unwrap();
    assert!(
        !rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    let echo = open(&state, generation, None, &subscribe)
        .unwrap()
        .snapshot
        .unwrap();
    assert_eq!(changed.session_id, echo.session_id);
    state.diychef_watch.stop();
}

#[test]
fn reopening_after_sidecar_save_imports_new_resources_and_external_txt_without_losing_disabled_hats()
 {
    let workspace = Workspace::new();
    let state = AppState::with_config(ConfigService::for_test(workspace.0.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.0.join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let root = game_root(&state).unwrap();
    let mod_root = root.join("BepInEx/plugins/DIYChef");
    let installed = serde_json::from_value(serde_json::json!({
        "name":"DIYChef", "guid":target::GUID, "version":"2.2.0", "dll_file":"BepInEx/plugins/DIYChef/DIYChef.dll",
        "enabled":true, "version_source":"plugin", "update_available":false, "duplicate_guid":false, "status":"unknown"
    })).unwrap();
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), vec![installed])
        .unwrap();
    // The production hook is invoked before catalog and TXT are read.
    let opened = open(&state, generation, None, |target| {
        let (path, target) = target.unwrap();
        assert_eq!(path, mod_root);
        assert_eq!(target.generation, generation);
        fs::write(path.join("prefer.txt"), "Chef_Black_cat HAT=Santa\n").unwrap();
        0
    })
    .unwrap()
    .snapshot
    .unwrap();
    assert_eq!(opened.selection.entries.len(), 1);
    for _ in 0..24 {
        let unchanged = open(&state, generation, None, |_| 0)
            .unwrap()
            .snapshot
            .unwrap();
        assert_eq!(unchanged.session_id, opened.session_id);
    }
    assert_eq!(state.diychef_sessions.lock().unwrap().values.len(), 1);
    assert!(get(&state, &opened.session_id).is_ok());
    let mut draft = baseline_draft(&get(&state, &opened.session_id).unwrap());
    draft.hat_bindings.insert(
        "native:Chef_Beard".into(),
        HatReference::Known {
            hat_id: "hat:None".into(),
        },
    );
    // Use the production TXT+sidecar transaction. The fixture DLL deliberately
    // has no PE metadata, so do not pretend a full plugin save was exercised.
    let stored = storage::write(
        &root,
        &mod_root,
        &opened.target.id,
        &state.config.config_path().with_file_name("diychef"),
        &opened.source.revision,
        b"Chef_Black_cat HAT=None\n",
        &draft.hat_bindings,
        false,
    )
    .unwrap();
    assert!(stored.bindings_path.is_file());
    let saved = open(&state, generation, None, |_| 0)
        .unwrap()
        .snapshot
        .unwrap();
    assert!(
        get(&state, &saved.session_id)
            .unwrap()
            .stored
            .bindings_path
            .is_file()
    );
    let chef = mod_root.join("Resources/NewFox");
    fs::create_dir_all(&chef).unwrap();
    fs::write(chef.join("INFO"), "ID=7\n").unwrap();
    fs::write(chef.join("t_Head.png"), "fixture texture").unwrap();
    fs::write(chef.join("m_Head.txt"), "fixture material").unwrap();
    let external = b"NewFox HAT=None\nChef_Black_cat HAT=Santa\n";
    fs::write(mod_root.join("prefer.txt"), external).unwrap();
    let reopened = open(&state, generation, None, |_| 0)
        .unwrap()
        .snapshot
        .unwrap();
    assert_ne!(reopened.source.revision, saved.source.revision);
    assert_ne!(reopened.catalog_revision, saved.catalog_revision);
    assert_ne!(reopened.session_id, saved.session_id);
    assert!(
        matches!(&reopened.selection.entries[0].chef_ref, ChefReference::Known { chef_id } if chef_id == "custom:NewFox")
    );
    assert_eq!(
        reopened.selection.hat_bindings["native:Chef_Beard"],
        HatReference::Known {
            hat_id: "hat:None".into()
        }
    );
    assert_eq!(
        reopened.selection.hat_bindings["custom:NewFox"],
        HatReference::Known {
            hat_id: "hat:None".into()
        }
    );
    assert!(
        !reopened
            .diagnostics
            .iter()
            .any(|problem| problem.save_blocking)
    );
    assert_eq!(
        fs::read(mod_root.join("prefer.txt")).unwrap(),
        external,
        "reopening does not rewrite external input"
    );
    assert_eq!(
        storage::write(
            &root,
            &mod_root,
            &saved.target.id,
            &state.config.config_path().with_file_name("diychef"),
            &saved.source.revision,
            b"Chef_Black_cat\n",
            &draft.hat_bindings,
            false
        )
        .err()
        .unwrap(),
        "diychef.conflict"
    );
}
