use super::{engine::RecordProgress, tests::Workspace, *};
use crate::file_metadata::FileIdentity;
use std::cell::Cell;
use std::fs;
use std::path::PathBuf;

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
fn replacing_an_approved_target_with_identical_bytes_aborts_and_rolls_back_earlier_writes() {
    for start_record in [false, true] {
        let workspace = Workspace::new();
        workspace.write("prior.dll", b"old");
        workspace.write("manifest.toml", b"old manifest");
        let mut manifest = workspace.file("manifest.toml", Some(b"updated manifest"));
        let Mutation::File(change) = &mut manifest else {
            unreachable!();
        };
        change.require_single_link = true;
        let target = change.target.resolve(&workspace.roots).unwrap();
        let original = FileIdentity::capture(&target).unwrap().unwrap();
        let before = change.expected.clone();
        let guards = vec![(change.target.clone(), original.clone())];
        let pending = prepare_for_operation(
            &workspace.journal_root,
            &workspace.roots,
            vec![workspace.file("prior.dll", Some(b"new")), manifest],
            None,
            guards,
        )
        .unwrap();
        let external = workspace.roots.game_root.join("external.toml");
        fs::write(&external, b"old manifest").unwrap();
        before.permissions().unwrap().restore(&external).unwrap();
        let external_identity = FileIdentity::capture(&external).unwrap().unwrap();
        assert_ne!(original, external_identity);
        let result = pending.apply_with_cancel(
            || false,
            |event| {
                if (!start_record && matches!(event, RecordProgress::Processed(0)))
                    || (start_record && matches!(event, RecordProgress::Applying(1)))
                {
                    assert_eq!(workspace.read("prior.dll"), b"new");
                    fs::rename(&target, workspace.roots.game_root.join("original.toml")).unwrap();
                    fs::rename(&external, &target).unwrap();
                    assert_eq!(Fingerprint::capture(&target).unwrap(), before);
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(
            FileIdentity::capture(&target).unwrap(),
            Some(external_identity)
        );
        assert_eq!(workspace.read("prior.dll"), b"old");
        assert_eq!(workspace.read("manifest.toml"), b"old manifest");
        assert!(!has_pending(&workspace.journal_root).unwrap());
    }
}

fn prepare_guarded_manifest_update(workspace: &Workspace) -> (PreparedTransaction, PathBuf) {
    workspace.write("prior.dll", b"old");
    let target = workspace.roots.data_root.join("manifest.toml");
    fs::write(&target, b"old manifest").unwrap();
    let mut manifest = workspace.file("unused.toml", Some(b"updated manifest"));
    let Mutation::File(change) = &mut manifest else {
        unreachable!();
    };
    change.target = TransactionPath {
        root: RootKind::Data,
        relative: "manifest.toml".into(),
    };
    change.expected = Fingerprint::capture(&target).unwrap();
    change.require_single_link = true;
    let guards = vec![(
        change.target.clone(),
        FileIdentity::capture(&target).unwrap().unwrap(),
    )];
    let pending = prepare_for_operation(
        &workspace.journal_root,
        &workspace.roots,
        vec![
            workspace.file("prior.dll", Some(b"new")),
            workspace.file("new.dll", Some(b"new")),
            manifest,
        ],
        None,
        guards,
    )
    .unwrap();
    (pending, target)
}

#[test]
fn tampering_with_a_guarded_manifest_does_not_leave_partial_game_writes() {
    for tamper in [
        "directory before publication",
        "edited after publication",
        "replaced after publication",
        "hard linked after publication",
    ] {
        let workspace = Workspace::new();
        let (pending, target) = prepare_guarded_manifest_update(&workspace);
        let id = pending.id().to_owned();
        let alias = target.with_extension("alias");
        let result = pending.apply_with_cancel(
            || false,
            |event| match (tamper, event) {
                ("directory before publication", RecordProgress::Applying(2)) => {
                    fs::remove_file(&target).unwrap();
                    fs::create_dir(&target).unwrap();
                }
                ("edited after publication", RecordProgress::Processed(2)) => {
                    assert_eq!(fs::read(&target).unwrap(), b"updated manifest");
                    fs::write(&target, b"external edit").unwrap();
                }
                ("replaced after publication", RecordProgress::Processed(2)) => {
                    let external = target.with_extension("external");
                    let installed = Fingerprint::capture(&target).unwrap();
                    fs::write(&external, b"updated manifest").unwrap();
                    installed.permissions().unwrap().restore(&external).unwrap();
                    fs::remove_file(&target).unwrap();
                    fs::rename(external, &target).unwrap();
                    assert_eq!(Fingerprint::capture(&target).unwrap(), installed);
                }
                ("hard linked after publication", RecordProgress::Processed(2)) => {
                    fs::hard_link(&target, &alias).unwrap();
                }
                _ => (),
            },
        );

        assert!(result.is_err(), "{tamper}");
        assert_eq!(workspace.read("prior.dll"), b"old", "{tamper}");
        assert!(
            !workspace.roots.game_root.join("new.dll").exists(),
            "{tamper}"
        );
        if tamper == "directory before publication" {
            assert!(target.is_dir());
            assert!(!has_pending(&workspace.journal_root).unwrap());
        } else {
            let expected: &[u8] = if tamper == "edited after publication" {
                b"external edit"
            } else {
                b"updated manifest"
            };
            assert_eq!(fs::read(&target).unwrap(), expected);
            assert_eq!(
                fs::read(workspace.journal_root.join(&id).join("before/2")).unwrap(),
                b"old manifest"
            );
            assert!(has_pending(&workspace.journal_root).unwrap());
        }
        if tamper == "hard linked after publication" {
            let blocked = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
            assert_eq!(blocked.blocked, [id.clone()]);
            assert_eq!(fs::read(&alias).unwrap(), b"updated manifest");
            assert_eq!(fs::read(&target).unwrap(), b"updated manifest");

            fs::remove_file(alias).unwrap();
            let recovered = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
            assert_eq!(recovered.recovered, [id]);
            assert_eq!(fs::read(&target).unwrap(), b"old manifest");
            assert_eq!(workspace.read("prior.dll"), b"old");
            assert!(!workspace.roots.game_root.join("new.dll").exists());
            assert!(!has_pending(&workspace.journal_root).unwrap());
        }
    }
}

#[test]
fn restart_resumes_an_interrupted_guarded_manifest_restore() {
    let workspace = Workspace::new();
    let (mut pending, target) = prepare_guarded_manifest_update(&workspace);
    let manifest = 2;
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    for index in 0..=manifest {
        pending.apply_record(index, || {}).unwrap();
    }
    let record = &mut pending.journal.records[manifest];
    let types::RecordedMutation::File { temporary, .. } = &record.operation else {
        unreachable!();
    };
    let temporary = temporary.resolve(&workspace.roots).unwrap();
    let Some(types::FilePublication::Staged(published)) = record.publication.take() else {
        unreachable!();
    };
    fs::write(&temporary, b"partial").unwrap();
    record.publication = Some(types::FilePublication::Restoring {
        published,
        temporary: FileIdentity::capture(&temporary).unwrap().unwrap(),
    });
    record.phase = types::RecordPhase::Reverting;
    pending.journal.state = JournalState::RecoveryRequired;
    pending.persist().unwrap();
    let id = pending.id().to_owned();
    drop(pending);

    assert_eq!(fs::read(&target).unwrap(), b"updated manifest");
    assert_eq!(workspace.read("prior.dll"), b"new");
    assert_eq!(workspace.read("new.dll"), b"new");
    let report = recover_all(&workspace.journal_root, &workspace.roots).unwrap();
    assert_eq!(report.recovered, [id]);
    assert!(report.blocked.is_empty());
    assert_eq!(fs::read(target).unwrap(), b"old manifest");
    assert_eq!(workspace.read("prior.dll"), b"old");
    assert!(!workspace.roots.game_root.join("new.dll").exists());
    assert!(!temporary.exists());
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
