use super::*;
use crate::mod_changes::guard;

#[tokio::test]
async fn source_switch_requires_recovery_confirmation_for_a_missing_old_companion() {
    let test = setup_mirrors();
    test.change_old_manifest();
    let state = &test.fixture.state;
    let transaction_root = state.config.mod_change_transactions_dir();
    let data_root = transaction_root.parent().unwrap();
    let staged = data_root.join("interrupted-companion.txt");
    write(&staged, b"retained companion");
    let transaction = prepare(
        &transaction_root,
        &TransactionRoots {
            game_root: test.fixture.game.clone(),
            data_root: data_root.into(),
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Game,
                relative: OLD_RESOURCE.into(),
            },
            source: Some(StagedFile::capture(staged).unwrap()),
            expected: Fingerprint::Missing,
            require_single_link: false,
        })],
    )
    .unwrap();
    let id = transaction.id().to_owned();
    let directory = transaction_root.join(&id);
    drop(transaction);
    let entry = guard::list(state)
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap();
    assert!(entry.scope_known);
    guard::decide(state, &id, &entry.revision, guard::Decision::Pause).unwrap();

    let before = test.fingerprints();
    let retained = Fingerprint::capture(&directory).unwrap();
    let offer = test
        .prepare(None)
        .await
        .unwrap()
        .manifest_overwrite
        .unwrap();
    assert!(offer.source_switch.is_some());
    let intent = ManifestOverwriteIntent {
        repository_id: offer.repository_id,
        fingerprint: offer.fingerprint,
    };
    let plan = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    let [conflict] = plan.recovery_conflicts.as_slice() else {
        panic!("expected one recovery conflict for the absent old companion");
    };
    let evidence = serde_json::to_value(conflict).unwrap();
    assert_eq!(evidence["transaction_id"], id);
    let expected = crate::display_path::display_path(&test.fixture.game.join(OLD_RESOURCE));
    assert_eq!(evidence["paths"], serde_json::json!([expected]));

    let mut session = take(&test.fixture, &plan);
    assert_eq!(
        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err(),
        "modChanges.recoveryRequired"
    );
    assert_eq!(test.fingerprints(), before);
    assert_eq!(Fingerprint::capture(&directory).unwrap(), retained);

    let approval = plan.recovery_approval.as_deref().unwrap();
    execution::commit_with_recovery(state, &mut session, Some(approval), |_| {}).unwrap();
    test.assert_new_install();
    assert!(!test.fixture.game.join(OLD_RESOURCE).exists());
    assert_eq!(Fingerprint::capture(&directory).unwrap(), retained);
}
