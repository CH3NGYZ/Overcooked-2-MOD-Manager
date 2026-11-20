use super::*;

#[test]
fn exact_upgrade_cleans_tied_same_guid_copies() {
    assert_tied_copies_upgrade("BepInEx/plugins/Z-FastInit/FastInit.dll");
}

#[test]
#[cfg(windows)] // The renamed file relies on PE string metadata.
fn exact_upgrade_cleans_a_renamed_copy_tied_in_version() {
    assert_tied_copies_upgrade("BepInEx/plugins/Z-FastInit/FastInit (2).dll");
}

fn assert_tied_copies_upgrade(extra: &str) {
    let f = same_guid_fastinit_fixture();
    fs::remove_file(f.game.join(OLDER)).unwrap();
    retained_repo_copy(&f, "FastInit-legacy-enabled", extra, GUID, "1.9.5");
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.retired_mod_copies[NODE].len(), 2);
    assert!(
        plan.retired_mod_copies[NODE]
            .iter()
            .any(|copy| copy.dll_file == extra)
    );

    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(game_and_manifest_bytes(&f), before);
    session.plan.prepared = true;
    super::super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();

    for path in [extra, DISABLED_OLDER] {
        assert!(!f.game.join(path).exists());
    }
    assert_eq!(
        fs::read(f.game.join(CURRENT)).unwrap(),
        mods::plugin_blob(GUID, FASTINIT, "1.9.6")
    );
    assert!(
        archive::read_manifest(&f.state.config.manifests_dir(), "FastInit-legacy-enabled")
            .is_none()
    );
}
