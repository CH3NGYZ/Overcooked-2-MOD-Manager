use super::{tests::*, *};
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

fn change(w: &Workspace, target: &str, source: Option<&str>) -> Mutation {
    Mutation::Tree(TreeChange {
        target: game(target),
        source: source.map(|source| StagedTree::capture(w.stage.join(source)).unwrap()),
        expected: Fingerprint::capture(&w.roots.game_root.join(target)).unwrap(),
    })
}

fn staged(w: &Workspace, root: &str, relative: &str, bytes: &[u8]) {
    let path = w.stage.join(root).join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn tree_changed_after_freeze_is_rejected_before_journal_is_ready() {
    let workspace = Workspace::new();
    workspace.write("maps/one/info", b"old");
    staged(&workspace, "one", "info", b"new");
    let frozen = change(&workspace, "maps/one", Some("one"));
    staged(&workspace, "one", "info", b"edit after freeze");
    let error = match prepare(&workspace.journal_root, &workspace.roots, vec![frozen]) {
        Err(error) => error,
        Ok(_) => panic!("journal accepted a tree edited after freeze"),
    };
    assert!(error.starts_with(check::STAGED), "{error}");
    assert_eq!(workspace.read("maps/one/info"), b"old");
    assert!(!has_pending(&workspace.journal_root).unwrap());
}

#[test]
fn whole_tree_replacement_and_deletion_commit_as_one_batch() {
    let w = Workspace::new();
    w.write("maps/one/info", b"old");
    w.write("maps/one/user.txt", b"old user");
    w.write("maps/two/info", b"delete");
    staged(&w, "one", "info", b"new");
    staged(&w, "one", "nested/plugin.dll.disabled", b"new plugin");
    let old_info = w.roots.game_root.join("maps/one/info");
    let mut permissions = fs::metadata(&old_info).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&old_info, permissions).unwrap();
    let changes = vec![
        change(&w, "maps/one", Some("one")),
        change(&w, "maps/two", None),
    ];
    let prepared = w.prepare(changes);
    assert_eq!(w.read("maps/one/user.txt"), b"old user");
    prepared.apply().unwrap();
    assert_eq!(w.read("maps/one/info"), b"new");
    assert!(fs::metadata(&old_info).unwrap().permissions().readonly());
    assert!(!w.roots.game_root.join("maps/one/user.txt").exists());
    assert!(!w.roots.game_root.join("maps/two").exists());
    assert!(!has_pending(&w.journal_root).unwrap());
    assert_eq!(
        fs::read_dir(w.roots.game_root.join("maps"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn directory_replacement_restart_restores_members_and_permissions() {
    let w = Workspace::new();
    w.write("map/info", b"old");
    w.write("map/nested/user", b"user");
    let user = w.roots.game_root.join("map/nested/user");
    let mut permissions = fs::metadata(&user).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&user, permissions).unwrap();
    let before = Fingerprint::capture(&w.roots.game_root.join("map")).unwrap();
    staged(&w, "map", "info", b"new");
    let mut pending = w.prepare(vec![change(&w, "map", Some("map"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    drop(pending);
    let report = recover_all(&w.journal_root, &w.roots).unwrap();
    assert_eq!(report.recovered.len(), 1);
    assert!(report.blocked.is_empty());
    assert_eq!(
        Fingerprint::capture(&w.roots.game_root.join("map")).unwrap(),
        before
    );
    assert!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .recovered
            .is_empty()
    );
}

#[test]
fn interrupted_after_old_tree_rename_is_recovered() {
    let w = Workspace::new();
    w.write("map/info", b"old");
    staged(&w, "map", "info", b"new");
    let mut pending = w.prepare(vec![change(&w, "map", Some("map"))]);
    pending.journal.state = JournalState::Applying;
    pending.journal.records[0].phase = types::RecordPhase::Applying;
    pending.persist().unwrap();
    let types::RecordedMutation::Tree { target, backup, .. } =
        &pending.journal.records[0].operation
    else {
        panic!()
    };
    durable::rename(
        &target.resolve(&w.roots).unwrap(),
        &backup.resolve(&w.roots).unwrap(),
        false,
    )
    .unwrap();
    drop(pending);
    assert!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .blocked
            .is_empty()
    );
    assert_eq!(w.read("map/info"), b"old");
}

#[test]
fn user_edit_in_applied_tree_blocks_recovery_and_retains_both_trees() {
    let w = Workspace::new();
    w.write("map/info", b"old");
    staged(&w, "map", "info", b"new");
    let mut pending = w.prepare(vec![change(&w, "map", Some("map"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    let types::RecordedMutation::Tree { backup, .. } = &pending.journal.records[0].operation else {
        panic!()
    };
    let backup = backup.resolve(&w.roots).unwrap();
    drop(pending);
    w.write("map/user.txt", b"new user");
    assert_eq!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .blocked
            .len(),
        1
    );
    assert_eq!(w.read("map/user.txt"), b"new user");
    assert_eq!(fs::read(backup.join("info")).unwrap(), b"old");
}

#[test]
fn tree_cancel_reverses_preceding_file_and_committed_restart_only_cleans() {
    let w = Workspace::new();
    w.write("first.dll", b"old");
    w.write("map/info", b"old map");
    staged(&w, "map", "info", b"new map");
    let pending = w.prepare(vec![
        w.file("first.dll", Some(b"new")),
        change(&w, "map", Some("map")),
    ]);
    let calls = AtomicUsize::new(0);
    assert!(
        pending
            .apply_with_cancel(|| calls.fetch_add(1, Ordering::SeqCst) >= 2, |_| {})
            .is_err()
    );
    assert_eq!(w.read("first.dll"), b"old");
    assert_eq!(w.read("map/info"), b"old map");
    let mut pending = w.prepare(vec![change(&w, "map", Some("map"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    pending.journal.state = JournalState::Committed;
    pending.persist().unwrap();
    drop(pending);
    assert_eq!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .cleaned
            .len(),
        1
    );
    assert_eq!(w.read("map/info"), b"new map");
}

#[test]
fn changed_committed_backup_is_retained_and_partial_purge_resumes() {
    let w = Workspace::new();
    w.write("map/info", b"old");
    staged(&w, "map", "info", b"new");
    let mut pending = w.prepare(vec![change(&w, "map", Some("map"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0, || {}).unwrap();
    pending.journal.state = JournalState::Committed;
    pending.persist().unwrap();
    let types::RecordedMutation::Tree { backup, .. } = &pending.journal.records[0].operation else {
        panic!()
    };
    let backup = backup.resolve(&w.roots).unwrap();
    fs::write(backup.join("user.txt"), b"later user").unwrap();
    drop(pending);
    assert_eq!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .blocked
            .len(),
        1
    );
    assert_eq!(fs::read(backup.join("user.txt")).unwrap(), b"later user");
    fs::remove_file(backup.join("user.txt")).unwrap();
    let directory = fs::read_dir(&w.journal_root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut journal = durable::read_journal(&directory).unwrap();
    journal.records[0].phase = types::RecordPhase::Purging;
    durable::write_journal(&directory, &journal).unwrap();
    fs::remove_file(backup.join("info")).unwrap();
    assert_eq!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .cleaned
            .len(),
        1
    );
    assert_eq!(w.read("map/info"), b"new");
}
