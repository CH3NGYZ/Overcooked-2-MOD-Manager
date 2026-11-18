use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;

pub(super) struct Workspace {
    _temporary: TempDir,
    pub(super) roots: TransactionRoots,
    pub(super) journal_root: PathBuf,
    pub(super) stage: PathBuf,
}

impl Workspace {
    pub(super) fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let game_root = temporary.path().join("game");
        let data_root = temporary.path().join("data");
        fs::create_dir_all(&game_root).unwrap();
        fs::create_dir_all(&data_root).unwrap();
        let game_root = fs::canonicalize(game_root).unwrap();
        let data_root = fs::canonicalize(data_root).unwrap();
        let stage = data_root.join("stage");
        fs::create_dir(&stage).unwrap();
        let journal_root = data_root.join("transactions");
        Self {
            _temporary: temporary,
            roots: TransactionRoots {
                game_root,
                data_root,
            },
            journal_root,
            stage,
        }
    }

    pub(super) fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.roots.game_root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    pub(super) fn file(&self, relative: &str, bytes: Option<&[u8]>) -> Mutation {
        let target = self.roots.game_root.join(relative);
        let source = bytes.map(|bytes| {
            let path = self.stage.join(uuid::Uuid::new_v4().to_string());
            fs::write(&path, bytes).unwrap();
            StagedFile::capture(path).unwrap()
        });
        Mutation::File(FileChange {
            target: game(relative),
            source,
            expected: Fingerprint::capture(&target).unwrap(),
            require_single_link: false,
        })
    }

    fn movement(&self, from: &str, to: &str) -> Mutation {
        Mutation::Move(MoveChange {
            from: game(from),
            to: game(to),
            expected: Fingerprint::capture(&self.roots.game_root.join(from)).unwrap(),
        })
    }

    pub(super) fn prepare(&self, mutations: Vec<Mutation>) -> PreparedTransaction {
        prepare(&self.journal_root, &self.roots, mutations).unwrap()
    }

    pub(super) fn read(&self, relative: &str) -> Vec<u8> {
        fs::read(self.roots.game_root.join(relative)).unwrap()
    }
}

pub(super) fn game(relative: &str) -> TransactionPath {
    TransactionPath {
        root: RootKind::Game,
        relative: relative.into(),
    }
}

#[test]
fn prepared_does_not_change_targets_and_batch_commits_every_file() {
    let w = Workspace::new();
    w.write("plugins/a.dll", b"old A");
    w.write("plugins/b.dll", b"old B");
    let pending = w.prepare(vec![
        w.file("plugins/a.dll", Some(b"new A")),
        w.file("plugins/b.dll", Some(b"new B")),
    ]);
    assert_eq!(pending.state(), JournalState::Prepared);
    assert_eq!(w.read("plugins/a.dll"), b"old A");
    assert!(has_pending(&w.journal_root).unwrap());
    pending.apply().unwrap();
    assert_eq!(w.read("plugins/a.dll"), b"new A");
    assert_eq!(w.read("plugins/b.dll"), b"new B");
    assert!(!has_pending(&w.journal_root).unwrap());
}

#[test]
fn cancellation_after_first_write_reverses_batch_and_keeps_original_permissions() {
    let w = Workspace::new();
    w.write("a.dll", b"old A");
    let mut permissions = fs::metadata(w.roots.game_root.join("a.dll"))
        .unwrap()
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(w.roots.game_root.join("a.dll"), permissions).unwrap();
    let original = Fingerprint::capture(&w.roots.game_root.join("a.dll")).unwrap();
    let pending = w.prepare(vec![
        w.file("a.dll", Some(b"new A")),
        w.file("nested/b.dll", Some(b"new B")),
    ]);
    let calls = AtomicUsize::new(0);
    assert_eq!(
        pending
            .apply_with_cancel(|| calls.fetch_add(1, Ordering::SeqCst) >= 2)
            .unwrap_err(),
        "download.cancelled"
    );
    assert_eq!(
        Fingerprint::capture(&w.roots.game_root.join("a.dll")).unwrap(),
        original
    );
    assert!(!w.roots.game_root.join("nested/b.dll").exists());
}

#[test]
fn mixed_disabled_rename_directory_migration_and_manifest_rollback_after_restart() {
    let w = Workspace::new();
    w.write("plugins/a.dll", b"A");
    w.write("old/levels/map/info", b"map data");
    fs::write(w.roots.data_root.join("manifest.toml"), b"old manifest").unwrap();
    let source = w.stage.join("manifest");
    fs::write(&source, b"new manifest").unwrap();
    let manifest = Mutation::File(FileChange {
        target: TransactionPath {
            root: RootKind::Data,
            relative: "manifest.toml".into(),
        },
        source: Some(StagedFile::capture(source).unwrap()),
        expected: Fingerprint::capture(&w.roots.data_root.join("manifest.toml")).unwrap(),
        require_single_link: false,
    });
    let mut pending = w.prepare(vec![
        w.movement("plugins/a.dll", "plugins/a.dll.disabled"),
        w.movement("old/levels", "new/levels"),
        manifest,
    ]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    for index in 0..3 {
        pending.apply_record(index).unwrap();
    }
    assert!(w.roots.game_root.join("new/levels/map/info").exists());
    drop(pending);
    let report = recover_all(&w.journal_root, &w.roots).unwrap();
    assert_eq!(report.recovered.len(), 1);
    assert!(report.blocked.is_empty());
    assert_eq!(w.read("plugins/a.dll"), b"A");
    assert_eq!(w.read("old/levels/map/info"), b"map data");
    assert_eq!(
        fs::read(w.roots.data_root.join("manifest.toml")).unwrap(),
        b"old manifest"
    );
    let second = recover_all(&w.journal_root, &w.roots).unwrap();
    assert!(second.recovered.is_empty() && second.blocked.is_empty());
}

#[test]
fn write_ahead_gap_after_rename_is_recovered() {
    let w = Workspace::new();
    w.write("a.dll", b"old");
    let mut pending = w.prepare(vec![w.file("a.dll", Some(b"new"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0).unwrap();
    pending.journal.records[0].phase = types::RecordPhase::Applying;
    pending.persist().unwrap();
    drop(pending);
    assert!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .blocked
            .is_empty()
    );
    assert_eq!(w.read("a.dll"), b"old");
}

#[test]
fn user_edit_after_interruption_blocks_recovery_without_overwriting() {
    let w = Workspace::new();
    w.write("a.dll", b"old");
    let mut pending = w.prepare(vec![w.file("a.dll", Some(b"new"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0).unwrap();
    drop(pending);
    w.write("a.dll", b"user changed");
    let report = recover_all(&w.journal_root, &w.roots).unwrap();
    assert_eq!(report.blocked.len(), 1);
    assert_eq!(w.read("a.dll"), b"user changed");
    assert!(has_pending(&w.journal_root).unwrap());
    assert_eq!(
        recover_all(&w.journal_root, &w.roots)
            .unwrap()
            .blocked
            .len(),
        1
    );
}

#[test]
fn scene_change_between_prepare_and_apply_keeps_user_edit() {
    let w = Workspace::new();
    w.write("a.dll", b"old");
    let pending = w.prepare(vec![w.file("a.dll", Some(b"new"))]);
    w.write("a.dll", b"user edit");
    assert!(pending.apply().is_err());
    assert_eq!(w.read("a.dll"), b"user edit");
    assert!(!has_pending(&w.journal_root).unwrap());
}

#[test]
fn committed_restart_only_cleans_and_never_rolls_back() {
    let w = Workspace::new();
    w.write("a.dll", b"old");
    let mut pending = w.prepare(vec![w.file("a.dll", Some(b"new"))]);
    pending.journal.state = JournalState::Applying;
    pending.persist().unwrap();
    pending.apply_record(0).unwrap();
    pending.journal.state = JournalState::Committed;
    pending.persist().unwrap();
    drop(pending);
    let report = recover_all(&w.journal_root, &w.roots).unwrap();
    assert_eq!(report.cleaned.len(), 1);
    assert_eq!(w.read("a.dll"), b"new");
}

#[test]
fn endpoint_collisions_and_directory_descendants_are_rejected() {
    let w = Workspace::new();
    w.write("a.dll", b"A");
    assert!(
        prepare(
            &w.journal_root,
            &w.roots,
            vec![w.file("a.dll", Some(b"1")), w.file("A.dll", Some(b"2"))]
        )
        .is_err()
    );
    w.write("old/levels/map/info", b"map");
    assert!(
        prepare(
            &w.journal_root,
            &w.roots,
            vec![
                w.movement("old/levels", "new/levels"),
                w.file("new/levels/map/info", Some(b"new"))
            ]
        )
        .is_err()
    );
}

#[test]
fn traversal_and_journal_root_targets_are_rejected() {
    let w = Workspace::new();
    let mut change = match w.file("a.dll", Some(b"A")) {
        Mutation::File(change) => change,
        _ => unreachable!(),
    };
    change.target.relative = "../outside".into();
    assert!(
        prepare(
            &w.journal_root,
            &w.roots,
            vec![Mutation::File(change.clone())]
        )
        .is_err()
    );
    change.target = TransactionPath {
        root: RootKind::Data,
        relative: "transactions/hijack".into(),
    };
    assert!(prepare(&w.journal_root, &w.roots, vec![Mutation::File(change)]).is_err());
}

#[test]
fn tampered_payload_rolls_back_previously_applied_files() {
    let w = Workspace::new();
    w.write("a.dll", b"old A");
    let pending = w.prepare(vec![
        w.file("a.dll", Some(b"new A")),
        w.file("b.dll", Some(b"new B")),
    ]);
    fs::write(pending.directory.join("after/1"), b"tampered").unwrap();
    assert!(pending.apply().is_err());
    assert_eq!(w.read("a.dll"), b"old A");
    assert!(!w.roots.game_root.join("b.dll").exists());
}

#[test]
fn wrong_game_root_is_blocked_and_prepared_journal_is_retained() {
    let w = Workspace::new();
    let pending = w.prepare(vec![w.file("a.dll", Some(b"A"))]);
    let directory = pending.directory.clone();
    drop(pending);
    let mut roots = w.roots.clone();
    roots.game_root = w.roots.game_root.join("other");
    fs::create_dir(&roots.game_root).unwrap();
    assert_eq!(
        recover_all(&w.journal_root, &roots).unwrap().blocked.len(),
        1
    );
    assert!(directory.join("journal.json").exists());
}

#[test]
fn externally_created_parent_is_never_adopted_or_removed_by_rollback() {
    for during_apply in [false, true] {
        let w = Workspace::new();
        w.write("first.dll", b"old");
        let mut pending = w.prepare(vec![
            w.file("first.dll", Some(b"new")),
            w.file("external/sub/new.dll", Some(b"new child")),
        ]);
        let parent = w.roots.game_root.join("external");
        fs::create_dir(&parent).unwrap();
        if during_apply {
            pending.journal.state = JournalState::Applying;
            pending.persist().unwrap();
            assert!(pending.apply_record(0).is_err());
            drop(pending);
            assert_eq!(
                recover_all(&w.journal_root, &w.roots)
                    .unwrap()
                    .recovered
                    .len(),
                1
            );
        } else {
            assert!(pending.apply().is_err());
        }
        assert!(parent.is_dir());
        assert_eq!(w.read("first.dll"), b"old");
        assert!(!w.roots.game_root.join("external/sub/new.dll").exists());
        assert!(!has_pending(&w.journal_root).unwrap());
    }
}

#[test]
fn framework_directory_migration_and_sibling_package_replacement_commit_together() {
    let w = Workspace::new();
    w.write("BepInEx/plugins/old/framework.dll", b"old framework");
    w.write("BepInEx/plugins/old/levels/unrecognized/user.txt", b"user");
    fs::create_dir(w.roots.data_root.join("manifests")).unwrap();
    fs::write(
        w.roots.data_root.join("manifests/framework.toml"),
        b"old manifest",
    )
    .unwrap();
    fs::write(
        w.roots.data_root.join("manifests/level-set-plugins.json"),
        b"old registry",
    )
    .unwrap();
    let map_destination = if cfg!(windows) {
        "bepinex/plugins/new/levels"
    } else {
        "BepInEx/plugins/new/levels"
    };
    let mut changes = vec![
        w.movement("BepInEx/plugins/old/levels", map_destination),
        w.file("BepInEx/plugins/new/framework.dll", Some(b"new framework")),
        w.file("BepInEx/plugins/old/framework.dll", None),
    ];
    for (relative, bytes) in [
        ("manifests/framework.toml", b"new manifest".as_slice()),
        (
            "manifests/level-set-plugins.json",
            b"new registry".as_slice(),
        ),
    ] {
        let source = w.stage.join(uuid::Uuid::new_v4().to_string());
        fs::write(&source, bytes).unwrap();
        changes.push(Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Data,
                relative: relative.into(),
            },
            source: Some(StagedFile::capture(source).unwrap()),
            expected: Fingerprint::capture(&w.roots.data_root.join(relative)).unwrap(),
            require_single_link: false,
        }));
    }
    w.prepare(changes).apply().unwrap();
    assert_eq!(
        w.read("BepInEx/plugins/new/levels/unrecognized/user.txt"),
        b"user"
    );
    assert_eq!(
        w.read("BepInEx/plugins/new/framework.dll"),
        b"new framework"
    );
    assert!(
        !w.roots
            .game_root
            .join("BepInEx/plugins/old/framework.dll")
            .exists()
    );
    assert!(!has_pending(&w.journal_root).unwrap());
}

#[test]
fn unix_symlink_endpoints_are_rejected() {
    #[cfg(unix)]
    {
        let w = Workspace::new();
        std::os::unix::fs::symlink(&w.stage, w.roots.game_root.join("link")).unwrap();
        assert!(Fingerprint::capture(&w.roots.game_root.join("link/a.dll")).is_err());
    }
}
