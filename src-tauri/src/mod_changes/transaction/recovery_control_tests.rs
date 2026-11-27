use super::{durable::JOURNAL_FILE, tests::Workspace, types::JOURNAL_VERSION, *};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(windows)]
mod documents_alias;

fn get(workspace: &Workspace) -> RecoveryEntry {
    let mut entries = list_recoveries(&workspace.journal_root, Some(&workspace.roots)).unwrap();
    assert_eq!(entries.len(), 1);
    entries.pop().unwrap()
}

fn interrupted(workspace: &Workspace) -> PathBuf {
    workspace.write("a.dll", b"old");
    let mut pending = workspace.prepare(vec![workspace.file("a.dll", Some(b"new"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    pending.directory
}

fn pause(workspace: &Workspace, entry: &RecoveryEntry) {
    pause_recovery(
        &workspace.journal_root,
        &entry.id,
        &entry.revision,
        Some(&workspace.roots),
    )
    .unwrap();
}

fn abandon(workspace: &Workspace, entry: &RecoveryEntry, high_risk: bool) -> Result<(), String> {
    abandon_recovery(
        &workspace.journal_root,
        &entry.id,
        &entry.revision,
        Some(&workspace.roots),
        high_risk,
    )
}

fn retry(workspace: &Workspace, entry: &RecoveryEntry) -> Result<RecoveryReport, String> {
    retry_recovery(
        &workspace.journal_root,
        &workspace.roots,
        &entry.id,
        &entry.revision,
    )
}

#[test]
fn application_and_latest_recovery_errors_stay_separate() {
    let workspace = Workspace::new();
    workspace.write("a.dll", b"old");
    let pending = workspace.prepare(vec![workspace.file("a.dll", Some(b"new"))]);
    let directory = pending.directory.clone();
    let backup = directory.join("before/0");
    let application = "modChanges.targetChanged";
    let result = pending.apply_checked(
        || false,
        || Ok(()),
        || {
            fs::remove_file(&backup).unwrap();
            Err(application.into())
        },
        |_| {},
    );
    assert!(needs_recovery(&result.unwrap_err()));
    let entry = get(&workspace);
    assert_eq!(entry.application_error.as_deref(), Some(application));
    assert!(entry.recovery_error.unwrap().starts_with(check::BACKUP));
    assert!(entry.legacy_error.is_none());

    workspace.write("a.dll", b"unrelated edit");
    let report = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
    assert_eq!(report.blocked, [entry.id]);
    let entry = get(&workspace);
    assert_eq!(entry.application_error.as_deref(), Some(application));
    assert!(entry.recovery_error.unwrap().starts_with(check::TARGET));
    assert_eq!(workspace.read("a.dll"), b"unrelated edit");
}

#[test]
fn paused_retry_is_persistent_across_reads_and_only_success_cleans() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let initial = get(&workspace);
    pause(&workspace, &initial);
    let paused = get(&workspace);
    assert_eq!(paused.state, RecoveryDecisionState::Paused);
    assert_ne!(paused.revision, initial.revision);
    assert!(retry(&workspace, &initial).is_err());
    assert_eq!(workspace.read("a.dll"), b"new");
    assert!(has_pending(&workspace.journal_root).unwrap());

    let before = fs::read(directory.join(JOURNAL_FILE)).unwrap();
    let report = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
    assert_eq!(report.deferred, [paused.id.clone()]);
    assert!(report.recovered.is_empty());
    assert_eq!(fs::read(directory.join(JOURNAL_FILE)).unwrap(), before);

    let backup = directory.join("before/0");
    let saved = directory.join("saved-backup");
    fs::rename(&backup, &saved).unwrap();
    assert_eq!(
        retry(&workspace, &paused).unwrap().blocked,
        [paused.id.clone()]
    );
    let failed = get(&workspace);
    assert_eq!(failed.state, RecoveryDecisionState::Paused);
    assert!(
        failed
            .recovery_error
            .unwrap_or_default()
            .starts_with(check::BACKUP)
    );
    let report = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
    assert_eq!(report.deferred, [paused.id.clone()]);
    assert_eq!(workspace.read("a.dll"), b"new");
    fs::rename(&saved, &backup).unwrap();
    assert_eq!(
        retry(&workspace, &get(&workspace)).unwrap().recovered,
        [paused.id]
    );
    assert_eq!(workspace.read("a.dll"), b"old");
    assert!(!directory.exists());
    assert!(!has_retained_material(&workspace.journal_root).unwrap());
}

#[test]
fn failed_final_cleanup_preserves_pause_and_never_replaces_an_existing_directory() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    pause(&workspace, &get(&workspace));
    let paused = get(&workspace);
    let collision =
        workspace
            .journal_root
            .join(format!("{}{}", recovery::CLEANED_PREFIX, paused.id));
    fs::create_dir(&collision).unwrap();
    let marker = collision.join("external");
    fs::write(&marker, b"user-owned").unwrap();

    assert_eq!(
        retry(&workspace, &paused).unwrap().blocked,
        [paused.id.clone()]
    );
    assert_eq!(workspace.read("a.dll"), b"old");
    assert_eq!(fs::read(&marker).unwrap(), b"user-owned");
    assert!(directory.join(JOURNAL_FILE).is_file());
    let pending = list_recoveries(&workspace.journal_root, Some(&workspace.roots))
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == paused.id)
        .unwrap();
    assert_eq!(pending.state, RecoveryDecisionState::Paused);
    assert!(pending.recovery_error.is_some());
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .deferred,
        [paused.id.clone()]
    );

    fs::remove_file(&marker).unwrap();
    fs::remove_dir(&collision).unwrap();
    assert_eq!(retry(&workspace, &pending).unwrap().cleaned, [paused.id]);
    assert!(!directory.exists());
}

#[test]
fn finished_cleaned_directory_never_retries_when_only_control_files_remain() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    pause(&workspace, &get(&workspace));
    let paused = get(&workspace);
    let cleaned = workspace
        .journal_root
        .join(format!("{}{}", recovery::CLEANED_PREFIX, paused.id));
    fs::create_dir(&cleaned).unwrap();
    assert_eq!(retry(&workspace, &paused).unwrap().blocked, [paused.id]);
    fs::remove_dir(&cleaned).unwrap();
    fs::rename(directory, &cleaned).unwrap();

    assert!(!has_pending(&workspace.journal_root).unwrap());
    assert!(!has_retained_material(&workspace.journal_root).unwrap());
    assert_eq!(workspace.read("a.dll"), b"old");
    assert!(
        list_recoveries(&workspace.journal_root, None)
            .unwrap()
            .is_empty()
    );
    fs::write(cleaned.join("unrecognized"), b"user file").unwrap();
    assert!(has_pending(&workspace.journal_root).unwrap());
    assert!(has_retained_material(&workspace.journal_root).unwrap());
}

#[test]
fn nonempty_cleaned_named_directory_with_damaged_journal_keeps_gate_closed() {
    let workspace = Workspace::new();
    let directory = workspace.journal_root.join(format!(
        "{}{}",
        recovery::CLEANED_PREFIX,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join(JOURNAL_FILE), b"damaged journal").unwrap();
    fs::write(directory.join("backup"), b"retained material").unwrap();

    assert!(has_pending(&workspace.journal_root).unwrap());
    let entry = get(&workspace);
    assert!(!entry.scope_known && !entry.can_decide);
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .blocked,
        [entry.id]
    );
    assert_eq!(
        fs::read(directory.join("backup")).unwrap(),
        b"retained material"
    );
}

#[test]
fn abandon_keeps_journal_original_backup_stage_and_live_file() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let entry = get(&workspace);
    assert!(entry.scope_known);
    for (kind, path) in [
        (RecoveryPathKind::Journal, directory.clone()),
        (RecoveryPathKind::Backup, directory.join("before/0")),
        (RecoveryPathKind::Staging, directory.join("after/0")),
        (
            RecoveryPathKind::Target,
            workspace.roots.game_root.join("a.dll"),
        ),
    ] {
        assert!(entry.paths.contains(&RecoveryPath { kind, path }));
    }
    let samples = [JOURNAL_FILE, "before/0", "after/0"]
        .map(|relative| (relative, fs::read(directory.join(relative)).unwrap()));
    abandon(&workspace, &entry, false).unwrap();
    let archived = get(&workspace);
    assert_eq!(archived.state, RecoveryDecisionState::Abandoned);
    assert!(archived.archived);
    assert!(archived.scope_known);
    assert!(!archived.can_decide);
    assert!(!archived.unknown_scope_acknowledged);
    assert!(!has_pending(&workspace.journal_root).unwrap());
    assert!(has_retained_material(&workspace.journal_root).unwrap());
    assert!(retry(&workspace, &archived).is_err());
    assert!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .recovered
            .is_empty()
    );
    for (relative, bytes) in samples {
        assert_eq!(fs::read(directory.join(relative)).unwrap(), bytes);
    }
    assert_eq!(workspace.read("a.dll"), b"new");
}

#[test]
fn malformed_journal_requires_acknowledgment_bound_to_exact_bytes() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let journal = directory.join(JOURNAL_FILE);
    let broken = b"a truncated record";
    fs::write(&journal, broken).unwrap();
    let initial = get(&workspace);
    assert!(!initial.scope_known);
    assert!(initial.can_decide);
    assert_eq!(
        initial.paths,
        [RecoveryPath {
            kind: RecoveryPathKind::Journal,
            path: directory.clone(),
        }]
    );
    pause(&workspace, &initial);
    let paused = get(&workspace);
    assert!(has_pending(&workspace.journal_root).unwrap());
    assert_eq!(
        abandon(&workspace, &paused, false).unwrap_err(),
        "modChanges.recoveryHighRiskRequired"
    );
    assert_eq!(fs::read(&journal).unwrap(), broken);
    abandon(&workspace, &paused, true).unwrap();
    assert_eq!(fs::read(&journal).unwrap(), broken);
    assert!(get(&workspace).unknown_scope_acknowledged);
    assert!(!has_pending(&workspace.journal_root).unwrap());

    fs::write(&journal, b"different broken record").unwrap();
    assert!(has_pending(&workspace.journal_root).unwrap());
    let changed = get(&workspace);
    assert_eq!(changed.state, RecoveryDecisionState::Abandoned);
    assert!(!changed.archived && changed.can_decide);
    assert!(!changed.unknown_scope_acknowledged);
    assert!(abandon(&workspace, &paused, true).is_err());
    assert!(retry(&workspace, &changed).is_err());
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .blocked,
        [changed.id.clone()]
    );
    assert_eq!(
        abandon(&workspace, &changed, false).unwrap_err(),
        "modChanges.recoveryHighRiskRequired"
    );
    abandon(&workspace, &changed, true).unwrap();
    assert!(get(&workspace).archived);
    assert!(!has_pending(&workspace.journal_root).unwrap());
    assert_eq!(fs::read(&journal).unwrap(), b"different broken record");
    assert_eq!(fs::read(directory.join("before/0")).unwrap(), b"old");
    assert_eq!(workspace.read("a.dll"), b"new");
}

#[test]
fn missing_journal_with_material_requires_high_risk_and_restored_journal_blocks_again() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let path = directory.join(JOURNAL_FILE);
    let original = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(has_pending(&workspace.journal_root).unwrap());
    let entry = get(&workspace);
    assert!(!entry.scope_known && entry.can_decide);
    assert_eq!(
        abandon(&workspace, &entry, false).unwrap_err(),
        "modChanges.recoveryHighRiskRequired"
    );
    abandon(&workspace, &entry, true).unwrap();
    assert!(!has_pending(&workspace.journal_root).unwrap());
    assert!(get(&workspace).unknown_scope_acknowledged);
    assert_eq!(fs::read(directory.join("before/0")).unwrap(), b"old");

    fs::write(&path, &original).unwrap();
    assert!(has_pending(&workspace.journal_root).unwrap());
    let changed = get(&workspace);
    assert!(!changed.archived && !changed.unknown_scope_acknowledged && changed.scope_known);
    assert!(changed.can_decide);
    assert!(retry(&workspace, &changed).is_err());
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .blocked,
        [changed.id.clone()]
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(workspace.read("a.dll"), b"new");
    abandon(&workspace, &changed, false).unwrap();
    let renewed = get(&workspace);
    assert!(renewed.archived && renewed.scope_known && !renewed.unknown_scope_acknowledged);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn empty_directory_is_not_pending_but_unnameable_material_is_blocked() {
    let workspace = Workspace::new();
    let empty = workspace
        .journal_root
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&empty).unwrap();
    assert!(!has_pending(&workspace.journal_root).unwrap());
    let unsafe_directory = workspace.journal_root.join("not-a-transaction-id");
    fs::create_dir(&unsafe_directory).unwrap();
    fs::write(unsafe_directory.join("backup"), b"material").unwrap();
    let entry = get(&workspace);
    assert!(!entry.scope_known && !entry.can_decide);
    assert!(has_pending(&workspace.journal_root).unwrap());
    assert!(abandon(&workspace, &entry, true).is_err());
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .blocked,
        [entry.id]
    );
    assert_eq!(
        fs::read(unsafe_directory.join("backup")).unwrap(),
        b"material"
    );
}

#[test]
fn stored_root_review_succeeds_but_retry_rejects_different_selected_game() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let before = fs::read(directory.join(JOURNAL_FILE)).unwrap();
    let entry = list_recoveries(&workspace.journal_root, None)
        .unwrap()
        .pop()
        .unwrap();
    assert!(entry.scope_known);
    let mut other_roots = workspace.roots.clone();
    other_roots.game_root = workspace.roots.game_root.join("other");
    fs::create_dir(&other_roots.game_root).unwrap();
    assert!(
        retry_recovery(
            &workspace.journal_root,
            &other_roots,
            &entry.id,
            &entry.revision
        )
        .is_err()
    );
    assert_eq!(fs::read(directory.join(JOURNAL_FILE)).unwrap(), before);
    assert_eq!(workspace.read("a.dll"), b"new");
    assert_eq!(retry(&workspace, &entry).unwrap().recovered, [entry.id]);
}

#[test]
fn different_selected_game_requires_high_risk_and_never_recovers_wrong_root() {
    let workspace = Workspace::new();
    workspace.write("levels/pear/info", b"old map");
    let replacement = workspace.stage.join("pear");
    fs::create_dir(&replacement).unwrap();
    fs::write(replacement.join("info"), b"new map").unwrap();
    let manifest = workspace.roots.data_root.join("manifests/pear.toml");
    fs::create_dir(manifest.parent().unwrap()).unwrap();
    fs::write(&manifest, b"old manifest").unwrap();
    let new_manifest = workspace.stage.join("pear.toml");
    fs::write(&new_manifest, b"new manifest").unwrap();
    let mut pending = workspace.prepare(vec![
        Mutation::Tree(TreeChange {
            target: super::tests::game("levels/pear"),
            source: Some(StagedTree::capture(replacement).unwrap()),
            expected: Fingerprint::capture(&workspace.roots.game_root.join("levels/pear")).unwrap(),
        }),
        Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Data,
                relative: "manifests/pear.toml".into(),
            },
            source: Some(StagedFile::capture(new_manifest).unwrap()),
            expected: Fingerprint::capture(&manifest).unwrap(),
            require_single_link: false,
        }),
    ]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    pending.apply_record(1, || {}).unwrap();
    let backup = workspace
        .roots
        .game_root
        .join(format!("levels/.oc2-batch-{}-0.tree-old", pending.id()));
    let directory = pending.directory;
    let mut other = workspace.roots.clone();
    other.game_root = workspace.roots.game_root.join("other");
    fs::create_dir(&other.game_root).unwrap();
    let journal = fs::read(directory.join(JOURNAL_FILE)).unwrap();
    let query = || {
        list_recoveries(&workspace.journal_root, Some(&other))
            .unwrap()
            .pop()
            .unwrap()
    };
    let mismatch = query();
    assert!(!mismatch.scope_known && mismatch.can_decide);
    for (kind, path) in [
        (RecoveryPathKind::Target, manifest.clone()),
        (RecoveryPathKind::Backup, backup.clone()),
        (RecoveryPathKind::Staging, directory.join("after/0")),
        (RecoveryPathKind::Journal, directory.clone()),
    ] {
        assert!(mismatch.paths.contains(&RecoveryPath { kind, path }));
    }
    assert!(
        retry_recovery(
            &workspace.journal_root,
            &other,
            &mismatch.id,
            &mismatch.revision
        )
        .is_err()
    );
    assert_eq!(
        recover_all(&workspace.journal_root, &other)
            .unwrap()
            .blocked,
        [mismatch.id.clone()]
    );
    let abandon = |high_risk| {
        abandon_recovery(
            &workspace.journal_root,
            &mismatch.id,
            &mismatch.revision,
            Some(&other),
            high_risk,
        )
    };
    assert_eq!(
        abandon(false).unwrap_err(),
        "modChanges.recoveryHighRiskRequired"
    );
    abandon(true).unwrap();
    let archived = query();
    assert!(archived.unknown_scope_acknowledged && !archived.scope_known && !archived.can_decide);
    assert_eq!(archived.paths, mismatch.paths);
    assert!(
        recover_all(&workspace.journal_root, &other)
            .unwrap()
            .blocked
            .is_empty()
    );
    assert_eq!(workspace.read("levels/pear/info"), b"new map");
    assert_eq!(fs::read(&manifest).unwrap(), b"new manifest");
    assert_eq!(fs::read(directory.join(JOURNAL_FILE)).unwrap(), journal);
    assert_eq!(
        fs::read(directory.join("before/1")).unwrap(),
        b"old manifest"
    );
    assert_eq!(fs::read(backup.join("info")).unwrap(), b"old map");
}

#[test]
fn archived_transaction_does_not_block_recovery_after_game_root_changes() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let entry = get(&workspace);
    abandon(&workspace, &entry, false).unwrap();
    let before = fs::read(directory.join(JOURNAL_FILE)).unwrap();
    let mut other_roots = workspace.roots.clone();
    other_roots.game_root = workspace.roots.game_root.join("other");
    fs::create_dir(&other_roots.game_root).unwrap();

    let archived = list_recoveries(&workspace.journal_root, Some(&other_roots))
        .unwrap()
        .pop()
        .unwrap();
    assert!(archived.scope_known && !archived.can_decide);
    let report = recover_all(&workspace.journal_root, &other_roots).unwrap();
    assert!(report.blocked.is_empty());
    assert!(report.recovered.is_empty());
    assert!(report.cleaned.is_empty());
    assert_eq!(fs::read(directory.join(JOURNAL_FILE)).unwrap(), before);
    assert_eq!(fs::read(directory.join("before/0")).unwrap(), b"old");
    assert_eq!(workspace.read("a.dll"), b"new");
}

#[test]
fn legacy_v2_error_is_not_reclassified_and_validation_does_not_upgrade_v2() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    let mut journal = durable::read_journal(&directory).unwrap();
    assert_eq!(journal.version, JOURNAL_VERSION);
    journal.version = 2;
    journal.legacy_error = Some("modChanges.verifyInstallBackup".into());
    durable::write_journal(&directory, &journal).unwrap();
    let entry = get(&workspace);
    assert_eq!(
        entry.legacy_error.as_deref(),
        Some("modChanges.verifyInstallBackup")
    );
    assert!(entry.application_error.is_none() && entry.recovery_error.is_none());
    assert!(entry.scope_known);
    workspace.write("a.dll", b"external edit");
    assert_eq!(
        retry(&workspace, &entry).unwrap().blocked,
        [entry.id.clone()]
    );
    assert_eq!(durable::read_journal(&directory).unwrap().version, 2);
    let failed = get(&workspace);
    assert_eq!(failed.legacy_error, entry.legacy_error);
    assert!(failed.application_error.is_none());
    assert!(failed.recovery_error.is_some());
    assert_eq!(workspace.read("a.dll"), b"external edit");
}

#[test]
fn unsafe_and_malformed_decisions_fail_closed_without_rollback() {
    let workspace = Workspace::new();
    let directory = interrupted(&workspace);
    pause(&workspace, &get(&workspace));
    let path = directory.join("recovery-decision.json");
    fs::write(&path, b"not a readable decision").unwrap();
    let entry = get(&workspace);
    assert!(!entry.scope_known && !entry.can_decide);
    assert!(abandon(&workspace, &entry, true).is_err());
    assert!(retry(&workspace, &entry).is_err());
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .blocked,
        [entry.id]
    );
    assert_eq!(workspace.read("a.dll"), b"new");
    assert_eq!(fs::read(path).unwrap(), b"not a readable decision");

    assert!(abandon_recovery(Path::new("missing"), "../escape", "", None, true).is_err());
}
