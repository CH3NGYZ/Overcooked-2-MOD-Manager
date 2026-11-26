use super::super::super::{execution, guard, transaction};
use super::*;

fn setup_paused(target: &str) -> (Fixture, PathBuf) {
    let fixture = setup();
    fs::rename(
        fixture.game.join(BUNDLED),
        fixture.game.join(mods::disabled_dll_file(BUNDLED)),
    )
    .unwrap();
    let transaction_root = fixture.state.config.mod_change_transactions_dir();
    let data_root = transaction_root.parent().unwrap();
    let payload = data_root.join("paused-map-plugin.dll");
    write(
        &payload,
        &mods::plugin_blob("com.example.map-recovery", "PausedMapPlugin", "2.0.0"),
    );
    let pending = transaction::prepare(
        &transaction_root,
        &TransactionRoots {
            game_root: fixture.game.clone(),
            data_root: data_root.to_path_buf(),
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Game,
                relative: target.into(),
            },
            source: Some(StagedFile::capture(payload).unwrap()),
            expected: Fingerprint::capture(&fixture.game.join(target)).unwrap(),
            require_single_link: false,
        })],
    )
    .unwrap();
    let id = pending.id().to_owned();
    drop(pending);
    let entry = guard::list(&fixture.state)
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap();
    guard::decide(&fixture.state, &id, &entry.revision, guard::Decision::Pause).unwrap();
    (fixture, transaction_root.join(id))
}

async fn prepare_map_disable(fixture: &Fixture, map_id: &str) -> ChangePlan {
    let mut request = toggle(false);
    request.roots[0].intent.node_id = map_id.into();
    let plan = planning::create(&fixture.state, request).unwrap();
    let prepared = prepare(fixture, &plan).await;
    let [action] = prepared.result.actions.as_slice() else {
        panic!("expected only the map toggle: {:?}", prepared.result);
    };
    assert_eq!(action.node_id, map_id);
    assert_eq!(action.kind, ActionKind::Disable);
    prepared
}

#[tokio::test]
async fn disabling_a_map_checks_paused_recovery_for_both_paths_of_its_already_disabled_dll() {
    let disabled = mods::disabled_dll_file(BUNDLED);
    for target in [BUNDLED, disabled.as_str()] {
        let (fixture, retained) = setup_paused(target);
        let before = snapshot(&fixture);
        let disabled_dll = fixture.game.join(&disabled);
        let dll_before = fs::read(&disabled_dll).unwrap();
        let journal_before = Fingerprint::capture(&retained).unwrap();

        let first = prepare_map_disable(&fixture, MAP_ID).await;
        let conflicts = serde_json::to_value(&first.recovery_conflicts).unwrap();
        assert_eq!(
            conflicts,
            serde_json::json!([{
                "transaction_id": retained.file_name().unwrap().to_str().unwrap(),
                "paths": [crate::display_path::display_path(&fixture.game.join(target))],
            }]),
        );
        let old_approval = first.recovery_approval.clone().unwrap();
        let mut session = take(&fixture, &first);
        assert_eq!(
            execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {})
                .unwrap_err(),
            "modChanges.recoveryRequired"
        );
        assert_eq!(snapshot(&fixture), before);

        let fresh = prepare_map_disable(&fixture, MAP_ID).await;
        let approval = fresh.recovery_approval.clone().unwrap();
        assert_ne!(old_approval, approval);
        let mut session = take(&fixture, &fresh);
        assert_eq!(
            execution::commit_with_recovery(
                &fixture.state,
                &mut session,
                Some(&old_approval),
                |_| {}
            )
            .unwrap_err(),
            "modChanges.recoveryConflictChanged"
        );
        assert_eq!(snapshot(&fixture), before);

        execution::commit_with_recovery(&fixture.state, &mut session, Some(&approval), |_| {})
            .unwrap();
        assert!(!fixture.game.join(MAP_PATH).exists());
        assert_eq!(
            fs::read(
                level_sets::disabled_root(&fixture.game)
                    .join(MAP)
                    .join("user-level")
            )
            .unwrap(),
            MAP_CONTENT,
        );
        assert!(!fixture.game.join(BUNDLED).exists());
        assert_eq!(fs::read(disabled_dll).unwrap(), dll_before);
        assert_eq!(
            Fingerprint::capture(&fixture.state.config.manifests_dir()).unwrap(),
            before[1]
        );
        assert_eq!(Fingerprint::capture(&retained).unwrap(), journal_before);
    }
}

#[tokio::test]
async fn disabling_an_unrelated_pure_map_does_not_require_the_other_maps_dll_recovery_approval() {
    const PURE_MAP: &str = "Pure";
    let disabled = mods::disabled_dll_file(BUNDLED);
    let (fixture, retained) = setup_paused(&disabled);
    let pure = fixture.game.join("BepInEx/plugins/DIYLevel/levels/Pure");
    write_map(&pure, PURE_MAP);
    let before = snapshot(&fixture);
    let retained_before = Fingerprint::capture(&retained).unwrap();
    let map_before = Fingerprint::capture(&fixture.game.join(MAP_PATH)).unwrap();
    let dll_before = fs::read(fixture.game.join(&disabled)).unwrap();

    let plan = prepare_map_disable(&fixture, "map:Pure").await;
    assert!(plan.recovery_conflicts.is_empty());
    assert!(plan.recovery_approval.is_none());
    assert_eq!(snapshot(&fixture), before);
    execution::commit_with_recovery(&fixture.state, &mut take(&fixture, &plan), None, |_| {})
        .unwrap();

    assert!(!pure.exists());
    assert_eq!(
        fs::read(
            level_sets::disabled_root(&fixture.game)
                .join(PURE_MAP)
                .join("user-level")
        )
        .unwrap(),
        MAP_CONTENT,
    );
    assert_eq!(
        Fingerprint::capture(&fixture.game.join(MAP_PATH)).unwrap(),
        map_before
    );
    assert!(!fixture.game.join(BUNDLED).exists());
    assert_eq!(fs::read(fixture.game.join(disabled)).unwrap(), dll_before);
    assert_eq!(
        Fingerprint::capture(&fixture.state.config.manifests_dir()).unwrap(),
        before[1]
    );
    assert_eq!(Fingerprint::capture(&retained).unwrap(), retained_before);
}
