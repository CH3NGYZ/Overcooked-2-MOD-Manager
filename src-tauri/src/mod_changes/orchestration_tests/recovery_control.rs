use super::super::{execution, guard};
use super::*;
use std::path::PathBuf;

const REQUIRED: &str = "modChanges.recoveryRequired";
const CHANGED: &str = "modChanges.recoveryConflictChanged";
const TARGET: &str = "BepInEx/plugins/one/one.dll";
const OTHER: &str = "BepInEx/plugins/two/two.dll";

fn recovery_fixture() -> Fixture {
    let fixture = fixture(vec![
        repository("one", "fixture-one", &["1.0.0", "2.0.0", "3.0.0"], vec![]),
        repository("two", "fixture-two", &["1.0.0"], vec![]),
    ]);
    install(&fixture, "one", "fixture-one", "1.0.0", true);
    fixture
}

fn fingerprint(path: &Path) -> Fingerprint {
    Fingerprint::capture(path).unwrap()
}

fn interrupt(fixture: &Fixture) -> PathBuf {
    let transaction_root = fixture.state.config.mod_change_transactions_dir();
    let data_root = transaction_root.parent().unwrap();
    let source = data_root.join("interrupted-replacement.dll");
    write(&source, &mods::plugin_blob("fixture-one", "one", "2.0.0"));
    let pending = prepare(
        &transaction_root,
        &TransactionRoots {
            game_root: fixture.game.clone(),
            data_root: data_root.to_path_buf(),
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Game,
                relative: TARGET.into(),
            },
            source: Some(StagedFile::capture(source).unwrap()),
            expected: fingerprint(&fixture.game.join(TARGET)),
            require_single_link: false,
        })],
    )
    .unwrap();
    let directory = transaction_root.join(pending.id());
    let failure = pending
        .apply_checked(
            || false,
            || Ok(()),
            || {
                fs::rename(
                    directory.join("before/0"),
                    directory.join("preserved-original"),
                )
                .unwrap();
                Err("modChanges.targetChanged".into())
            },
            |_| {},
        )
        .unwrap_err();
    assert!(
        super::super::transaction::needs_recovery(&failure),
        "{failure}"
    );
    directory
}

fn recovery(state: &AppState, directory: &Path) -> RecoveryEntry {
    let id = directory.file_name().unwrap().to_str().unwrap();
    guard::list(state)
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap()
}

fn decide(state: &AppState, directory: &Path, decision: guard::Decision) -> Result<(), String> {
    let entry = recovery(state, directory);
    guard::decide(state, &entry.id, &entry.revision, decision).map(|_| ())
}

fn plan(fixture: &Fixture, repo: &str, tag: &str) -> PlanSession {
    let selection = planning::create(&fixture.state, request(repo, Some(tag), true)).unwrap();
    let mut session = take(fixture, &selection);
    verify_selected(fixture, &mut session);
    preparation::finish(&fixture.state, &mut session).unwrap();
    session.plan.prepared = true;
    session
}

fn conflict_token(session: &PlanSession) -> String {
    assert!(!session.plan.recovery_conflicts.is_empty());
    session.plan.recovery_approval.clone().unwrap()
}

#[test]
fn paused_transaction_survives_new_state_and_only_retry_restores_and_cleans() {
    let fixture = recovery_fixture();
    let original = fingerprint(&fixture.game.join(TARGET));
    let directory = interrupt(&fixture);
    let interrupted = fingerprint(&fixture.game.join(TARGET));
    assert_ne!(original, interrupted);
    decide(&fixture.state, &directory, guard::Decision::Pause).unwrap();
    fs::rename(
        directory.join("preserved-original"),
        directory.join("before/0"),
    )
    .unwrap();
    let retained = fingerprint(&directory);

    let restarted = AppState::with_config(fixture.state.config.clone());
    {
        let _operation = restarted.operation.lock().unwrap();
        assert_eq!(guard::ensure_recovered(&restarted).unwrap_err(), REQUIRED);
        assert!(!guard::ensure_plannable(&restarted).unwrap());
    }
    assert_eq!(
        recovery(&restarted, &directory).state,
        RecoveryDecisionState::Paused
    );
    assert_eq!(fingerprint(&fixture.game.join(TARGET)), interrupted);
    assert_eq!(fingerprint(&directory), retained);
    decide(&restarted, &directory, guard::Decision::Retry).unwrap();
    assert_eq!(fingerprint(&fixture.game.join(TARGET)), original);
    assert!(!directory.exists());
    assert!(guard::list(&restarted).unwrap().is_empty());
}

#[test]
fn paused_transaction_allows_replanned_sibling_install_and_retains_material() {
    let fixture = recovery_fixture();
    let directory = interrupt(&fixture);
    decide(&fixture.state, &directory, guard::Decision::Pause).unwrap();
    let retained = fingerprint(&directory);
    let interrupted = fingerprint(&fixture.game.join(TARGET));
    let mut session = plan(&fixture, "two", "v1.0.0");
    execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {}).unwrap();
    assert_eq!(
        fs::read(fixture.game.join(OTHER)).unwrap(),
        mods::plugin_blob("fixture-two", "two", "1.0.0")
    );
    assert_eq!(fingerprint(&fixture.game.join(TARGET)), interrupted);
    assert_eq!(fingerprint(&directory), retained);
}

#[test]
fn pausing_after_prepare_requires_a_new_plan_even_for_a_disjoint_install() {
    let fixture = recovery_fixture();
    let mut old = plan(&fixture, "two", "v1.0.0");
    let directory = interrupt(&fixture);
    decide(&fixture.state, &directory, guard::Decision::Pause).unwrap();
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut old, None, |_| {}).unwrap_err(),
        CHANGED
    );
    assert!(!fixture.game.join(OTHER).exists());
    let mut fresh = plan(&fixture, "two", "v1.0.0");
    execution::commit_with_recovery(&fixture.state, &mut fresh, None, |_| {}).unwrap();
    assert!(fixture.game.join(OTHER).is_file());
}

#[test]
fn overlapping_install_requires_approval_for_the_current_plan_and_recovery_revision() {
    let fixture = recovery_fixture();
    let directory = interrupt(&fixture);
    decide(&fixture.state, &directory, guard::Decision::Pause).unwrap();
    fs::rename(
        directory.join("preserved-original"),
        directory.join("before/0"),
    )
    .unwrap();
    let journal = directory.join("journal.json");
    let interrupted = fingerprint(&fixture.game.join(TARGET));
    let mut first = plan(&fixture, "one", "v3.0.0");
    let first_token = conflict_token(&first);
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut first, None, |_| {}).unwrap_err(),
        REQUIRED
    );
    let mut other = plan(&fixture, "one", "v3.0.0");
    let other_token = conflict_token(&other);
    assert_ne!(first_token, other_token);
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut other, Some(&first_token), |_| {})
            .unwrap_err(),
        CHANGED
    );

    let mut edited = fs::read(&journal).unwrap();
    edited.push(b'\n');
    fs::write(&journal, edited).unwrap();
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut other, Some(&other_token), |_| {})
            .unwrap_err(),
        CHANGED
    );
    let mut refreshed = plan(&fixture, "one", "v3.0.0");
    let refreshed_token = conflict_token(&refreshed);
    let saved = ["journal.json", "before/0", "after/0"]
        .map(|relative| (relative, fingerprint(&directory.join(relative))));
    decide(
        &fixture.state,
        &directory,
        guard::Decision::Abandon { high_risk: false },
    )
    .unwrap();
    assert_eq!(
        execution::commit_with_recovery(
            &fixture.state,
            &mut refreshed,
            Some(&refreshed_token),
            |_| {}
        )
        .unwrap_err(),
        CHANGED
    );
    assert_eq!(fingerprint(&fixture.game.join(TARGET)), interrupted);
    let mut archived = plan(&fixture, "one", "v3.0.0");
    let archived_token = conflict_token(&archived);
    execution::commit_with_recovery(&fixture.state, &mut archived, Some(&archived_token), |_| {})
        .unwrap();
    assert_eq!(
        fs::read(fixture.game.join(TARGET)).unwrap(),
        mods::plugin_blob("fixture-one", "one", "3.0.0")
    );
    for (relative, before) in saved {
        assert_eq!(fingerprint(&directory.join(relative)), before);
    }
}

#[test]
fn overlapping_manifest_requires_confirmation_even_without_overlapping_game_files() {
    let fixture = recovery_fixture();
    let transaction_root = fixture.state.config.mod_change_transactions_dir();
    let data_root = transaction_root.parent().unwrap();
    let manifest = fixture.state.config.manifests_dir().join("one.toml");
    let relative = manifest
        .strip_prefix(data_root)
        .unwrap()
        .to_str()
        .unwrap()
        .replace('\\', "/");
    let original = fingerprint(&manifest);
    let pending = prepare(
        &transaction_root,
        &TransactionRoots {
            game_root: fixture.game.clone(),
            data_root: data_root.to_path_buf(),
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Data,
                relative,
            },
            source: None,
            expected: original.clone(),
            require_single_link: false,
        })],
    )
    .unwrap();
    let directory = transaction_root.join(pending.id());
    drop(pending);
    decide(&fixture.state, &directory, guard::Decision::Pause).unwrap();
    let retained = fingerprint(&directory);
    let mut session = plan(&fixture, "one", "v3.0.0");
    let token = conflict_token(&session);
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {}).unwrap_err(),
        REQUIRED
    );
    assert_eq!(fingerprint(&manifest), original);
    execution::commit_with_recovery(&fixture.state, &mut session, Some(&token), |_| {}).unwrap();
    assert_ne!(fingerprint(&manifest), original);
    assert_eq!(fingerprint(&directory), retained);
}

#[test]
fn new_dll_install_checks_the_other_state_even_when_it_is_missing() {
    let fixture = recovery_fixture();
    let transaction_root = fixture.state.config.mod_change_transactions_dir();
    let data_root = transaction_root.parent().unwrap();
    let disabled = mods::disabled_dll_file(OTHER);
    let source = data_root.join("previous-disabled.dll");
    write(&source, &mods::plugin_blob("fixture-two", "two", "1.0.0"));
    let pending = prepare(
        &transaction_root,
        &TransactionRoots {
            game_root: fixture.game.clone(),
            data_root: data_root.to_path_buf(),
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Game,
                relative: disabled.clone(),
            },
            source: Some(StagedFile::capture(source).unwrap()),
            expected: Fingerprint::Missing,
            require_single_link: false,
        })],
    )
    .unwrap();
    let directory = transaction_root.join(pending.id());
    drop(pending);
    decide(&fixture.state, &directory, guard::Decision::Pause).unwrap();
    let retained = fingerprint(&directory);

    let mut session = plan(&fixture, "two", "v1.0.0");
    let approval = conflict_token(&session);
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {}).unwrap_err(),
        REQUIRED
    );
    assert!(!fixture.game.join(OTHER).exists());
    execution::commit_with_recovery(&fixture.state, &mut session, Some(&approval), |_| {}).unwrap();
    assert!(fixture.game.join(OTHER).is_file());
    assert!(!fixture.game.join(disabled).exists());
    assert_eq!(fingerprint(&directory), retained);
}

fn damaged(fixture: &Fixture) -> PathBuf {
    let directory = fixture
        .state
        .config
        .mod_change_transactions_dir()
        .join(uuid::Uuid::new_v4().to_string());
    write(&directory.join("journal.json"), b"malformed journal");
    write(&directory.join("before/0"), b"retained unknown backup");
    write(&directory.join("after/0"), b"retained unknown stage");
    directory
}

#[test]
fn unknown_range_pause_blocks_writes_and_high_risk_exempts_only_reviewed_transaction() {
    let fixture = recovery_fixture();
    let mut session = plan(&fixture, "two", "v1.0.0");
    let first = damaged(&fixture);
    let second = damaged(&fixture);
    let first_files = ["journal.json", "before/0", "after/0"]
        .map(|relative| (relative, fingerprint(&first.join(relative))));
    let second_backup = fingerprint(&second.join("before/0"));

    decide(&fixture.state, &first, guard::Decision::Pause).unwrap();
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {}).unwrap_err(),
        REQUIRED
    );
    assert!(!fixture.game.join(OTHER).exists());
    assert_eq!(
        decide(
            &fixture.state,
            &first,
            guard::Decision::Abandon { high_risk: false }
        )
        .unwrap_err(),
        "modChanges.recoveryHighRiskRequired"
    );
    decide(
        &fixture.state,
        &first,
        guard::Decision::Abandon { high_risk: true },
    )
    .unwrap();
    assert!(recovery(&fixture.state, &first).unknown_scope_acknowledged);
    assert!(!recovery(&fixture.state, &second).unknown_scope_acknowledged);
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {}).unwrap_err(),
        REQUIRED
    );
    assert!(!fixture.game.join(OTHER).exists());
    decide(
        &fixture.state,
        &second,
        guard::Decision::Abandon { high_risk: true },
    )
    .unwrap();
    let mut allowed = plan(&fixture, "two", "v1.0.0");
    let game_running = &fixture.state.game_running_for_test;
    game_running.store(true, std::sync::atomic::Ordering::Relaxed);
    let running_write = execution::commit_with_recovery(&fixture.state, &mut allowed, None, |_| {});
    game_running.store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(running_write.unwrap_err(), "game.running");
    assert!(!fixture.game.join(OTHER).exists());
    execution::commit_with_recovery(&fixture.state, &mut allowed, None, |_| {}).unwrap();
    assert!(fixture.game.join(OTHER).is_file());
    for (relative, before) in first_files {
        assert_eq!(fingerprint(&first.join(relative)), before);
    }
    assert_eq!(fingerprint(&second.join("before/0")), second_backup);
}

#[test]
fn high_risk_abandon_does_not_permit_prepared_writes_into_retained_material() {
    let fixture = recovery_fixture();
    let directory = damaged(&fixture);
    decide(
        &fixture.state,
        &directory,
        guard::Decision::Abandon { high_risk: true },
    )
    .unwrap();
    let retained = fingerprint(&directory);
    let mut session = plan(&fixture, "two", "v1.0.0");
    let transaction_root = fixture.state.config.mod_change_transactions_dir();
    let data_root = transaction_root.parent().unwrap();
    let backup = directory.join("before/0");
    session.mutations.as_mut().unwrap().push(
        super::super::application_progress::OwnedMutation::for_node(
            Mutation::File(FileChange {
                target: TransactionPath {
                    root: RootKind::Data,
                    relative: backup
                        .strip_prefix(data_root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace('\\', "/"),
                },
                source: None,
                expected: fingerprint(&backup),
                require_single_link: false,
            }),
            "mod:two",
        ),
    );
    assert_eq!(
        execution::commit_with_recovery(&fixture.state, &mut session, None, |_| {}).unwrap_err(),
        "modChanges.recoveryMaterialProtected"
    );
    assert_eq!(fingerprint(&directory), retained);
    assert!(!fixture.game.join(OTHER).exists());
}
