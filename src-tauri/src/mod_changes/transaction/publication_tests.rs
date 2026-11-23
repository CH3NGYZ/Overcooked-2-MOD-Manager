use super::{engine::RecordProgress, tests::Workspace, *};
use std::cell::Cell;
use std::fs;

#[test]
fn cancellation_after_the_last_live_write_rolls_back_before_the_durable_commit() {
    let workspace = Workspace::new();
    let pending = workspace.prepare(vec![workspace.file("new.dll", Some(b"payload"))]);
    let cancelled = Cell::new(false);
    let result = pending.apply_with_cancel(
        || cancelled.get(),
        |event| {
            if matches!(event, RecordProgress::Processed(0)) {
                assert_eq!(workspace.read("new.dll"), b"payload");
                cancelled.set(true);
            }
        },
    );
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert!(!workspace.roots.game_root.join("new.dll").exists());
    assert!(!has_pending(&workspace.journal_root).unwrap());
}

#[test]
fn collision_with_identical_file_preserves_it_and_rolls_back_earlier_writes() {
    let workspace = Workspace::new();
    workspace.write("prior.dll", b"old");
    let pending = workspace.prepare(vec![
        workspace.file("prior.dll", Some(b"new")),
        workspace.file("replacement.toml", Some(b"proposed manifest")),
    ]);
    let target = workspace.roots.game_root.join("replacement.toml");
    let types::RecordedMutation::File { after, .. } = &pending.journal.records[1].operation else {
        unreachable!();
    };
    let permissions = after.permissions().unwrap().clone();
    let result = pending.apply_with_cancel(
        || false,
        |event| {
            if matches!(event, RecordProgress::Applying(1)) {
                fs::write(&target, b"proposed manifest").unwrap();
                permissions.restore(&target).unwrap();
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(target).unwrap(), b"proposed manifest");
    assert_eq!(workspace.read("prior.dll"), b"old");
    assert!(!has_pending(&workspace.journal_root).unwrap());
}

#[test]
fn exclusive_rename_never_replaces_an_existing_target() {
    let workspace = Workspace::new();
    workspace.write("staged.toml", b"new");
    workspace.write("target.toml", b"external");
    let source = workspace.roots.game_root.join("staged.toml");
    let target = workspace.roots.game_root.join("target.toml");
    assert!(matches!(
        durable::rename_durable(&source, &target, false, check::WRITE),
        Err(durable::RenameFailure::Unchanged(_))
    ));
    assert_eq!(workspace.read("staged.toml"), b"new");
    assert_eq!(workspace.read("target.toml"), b"external");
}

#[test]
fn restart_uses_file_identity_to_recover_the_rename_gap() {
    for replace_with_identical_file in [false, true] {
        let workspace = Workspace::new();
        let mut pending = workspace.prepare(vec![workspace.file("new.dll", Some(b"payload"))]);
        pending.journal.state = JournalState::Applying;
        pending.persist().unwrap();
        pending.apply_record(0, || {}).unwrap();
        pending.journal.records[0].phase = types::RecordPhase::Applying;
        pending.persist().unwrap();
        let id = pending.id().to_owned();
        let target = workspace.roots.game_root.join("new.dll");
        if replace_with_identical_file {
            let external = workspace.roots.game_root.join("external.dll");
            let permissions = Fingerprint::capture(&target).unwrap();
            fs::write(&external, b"payload").unwrap();
            permissions
                .permissions()
                .unwrap()
                .restore(&external)
                .unwrap();
            fs::rename(&target, workspace.roots.game_root.join("away.dll")).unwrap();
            fs::rename(external, &target).unwrap();
            assert_eq!(Fingerprint::capture(&target).unwrap(), permissions);
        }
        drop(pending);

        let report = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
        if replace_with_identical_file {
            assert_eq!(report.blocked, [id.clone()]);
            assert!(workspace.journal_root.join(id).join("after/0").is_file());
            assert_eq!(fs::read(&target).unwrap(), b"payload");
        } else {
            assert_eq!(report.recovered, [id]);
            assert!(!target.exists());
        }
    }
}

#[test]
fn ambiguous_legacy_journal_keeps_live_file_and_recovery_material() {
    let workspace = Workspace::new();
    let mut pending = workspace.prepare(vec![workspace.file("new.dll", Some(b"payload"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    pending.journal.version = 1;
    pending.journal.records[0].publication = None;
    pending.persist().unwrap();
    let id = pending.id().to_owned();
    drop(pending);

    let report = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
    assert_eq!(report.blocked, [id.clone()]);
    assert!(workspace.journal_root.join(id).join("after/0").is_file());
    assert_eq!(workspace.read("new.dll"), b"payload");
}
