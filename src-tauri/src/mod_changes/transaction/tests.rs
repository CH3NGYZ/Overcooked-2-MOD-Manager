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
fn rejected_transaction_preparation_still_correlates_operation_and_failure() {
    let workspace = Workspace::new();
    let marker = workspace
        ._temporary
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let operation = uuid::Uuid::new_v4().to_string();
    let capture = crate::http_diagnostics::capture_test_logs(diagnostics::TARGET, marker);
    let invalid_root = workspace.roots.game_root.join("transactions");
    assert!(
        prepare_for_operation(
            &invalid_root,
            &workspace.roots,
            Vec::new(),
            Some(&operation)
        )
        .is_err()
    );
    let messages = capture.messages();
    let transaction_id = messages
        .iter()
        .find(|line| line.contains(&format!("op={operation}")))
        .and_then(|line| line.split_once("txn="))
        .and_then(|(_, value)| value.split_ascii_whitespace().next())
        .unwrap();
    assert!(
        messages
            .iter()
            .any(|line| line.contains(&format!("事务={transaction_id}，阶段=validate_roots")))
    );
    assert!(!invalid_root.exists());
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
fn domain_check_before_apply_rejects_with_no_live_writes_or_pending_journal() {
    let w = Workspace::new();
    w.write("plugins/a.dll", b"old");
    let pending = w.prepare(vec![
        w.file("plugins/a.dll", Some(b"new")),
        w.file("levels/New/info", Some(b"map")),
    ]);
    let journal = w.journal_root.join(pending.id());
    assert_eq!(
        pending
            .apply_checked(
                || false,
                || {
                    assert_eq!(
                        durable::read_journal(&journal)?.state,
                        JournalState::Prepared
                    );
                    Err("game.running".into())
                },
                || panic!("a rejected transaction must not commit"),
                |_| panic!("a rejected transaction must not start applying"),
            )
            .unwrap_err(),
        "game.running"
    );
    assert_eq!(w.read("plugins/a.dll"), b"old");
    assert!(!w.roots.game_root.join("levels").exists());
    assert!(!journal.exists());
    assert!(!has_pending(&w.journal_root).unwrap());
}

#[cfg(windows)]
#[test]
fn locked_replacement_logs_transaction_record_endpoints_os_code_and_rollback() {
    use std::os::windows::fs::OpenOptionsExt;
    use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    let workspace = Workspace::new();
    workspace.write("plugins/dependency.dll", b"old dependency");
    workspace.write("plugins/locked-target.dll", b"old target");
    let pending = workspace.prepare(vec![
        workspace.file("plugins/dependency.dll", Some(b"new dependency")),
        workspace.file("plugins/locked-target.dll", Some(b"new target")),
    ]);
    let id = pending.id().to_owned();
    let journal = workspace.journal_root.join(&id);
    let capture = crate::http_diagnostics::capture_test_logs(diagnostics::TARGET, &id);
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(workspace.roots.game_root.join("plugins/locked-target.dll"))
        .unwrap();
    let error = pending.apply().unwrap_err();
    drop(lock);
    assert!(error.starts_with("modChanges.writeInstallFile?"), "{error}");
    let messages = capture.messages();
    assert!(
        messages.iter().any(|message| message.contains("改名失败")
            && message.contains("记录号=2")
            && message.contains("locked-target.dll")
            && (message.contains("OS码=Some(5)") || message.contains("OS码=Some(32)"))
            && message.contains("允许替换=true")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("记录状态=Applying")
                && message.contains("before")
                && message.contains("after")
                && message.contains("sha256=")
                && message.contains("locked-target.dll")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("阶段=rolled_back") && message.contains("已回滚=2")),
        "{messages:?}"
    );
    assert_eq!(workspace.read("plugins/dependency.dll"), b"old dependency");
    assert_eq!(workspace.read("plugins/locked-target.dll"), b"old target");
    assert!(!journal.exists());
    assert!(!has_pending(&workspace.journal_root).unwrap());
}

#[test]
fn cancellation_after_journal_starts_reports_no_live_write() {
    let workspace = Workspace::new();
    workspace.write("plugins/a.dll", b"old");
    let pending = workspace.prepare(vec![workspace.file("plugins/a.dll", Some(b"new"))]);
    let id = pending.id().to_owned();
    let capture = crate::http_diagnostics::capture_test_logs(diagnostics::TARGET, &id);
    let checks = AtomicUsize::new(0);
    assert_eq!(
        pending
            .apply_with_cancel(
                || checks.fetch_add(1, Ordering::SeqCst) >= 1,
                |_| panic!("no record should start writing"),
            )
            .unwrap_err(),
        "download.cancelled"
    );
    let messages = capture.messages();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("阶段=no_live_write"))
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("阶段=rolled_back"))
    );
    assert_eq!(workspace.read("plugins/a.dll"), b"old");
    assert!(!workspace.journal_root.join(id).exists());
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
            .apply_with_cancel(|| calls.fetch_add(1, Ordering::SeqCst) >= 2, |_| {})
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
        pending.apply_record(index, || {}).unwrap();
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
    pending.apply_record(0, || {}).unwrap();
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
    pending.apply_record(0, || {}).unwrap();
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
    pending.apply_record(0, || {}).unwrap();
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
    let capture = crate::http_diagnostics::capture_test_logs(diagnostics::TARGET, pending.id());
    drop(pending);
    let mut roots = w.roots.clone();
    roots.game_root = w.roots.game_root.join("other");
    fs::create_dir(&roots.game_root).unwrap();
    assert_eq!(
        recover_all(&w.journal_root, &roots).unwrap().blocked.len(),
        1
    );
    assert!(directory.join("journal.json").exists());
    let messages = capture.messages();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("字段=roots") && message.contains("journal.json"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("当前游戏根=") && message.contains("other"))
    );
}

#[test]
fn invalid_journal_logs_parse_location_without_copying_content() {
    let workspace = Workspace::new();
    let pending = workspace.prepare(vec![workspace.file("a.dll", Some(b"A"))]);
    let id = pending.id().to_owned();
    let journal = pending.directory.join("journal.json");
    drop(pending);
    let invalid = br#"{"version":"private marker not for logs"}"#;
    fs::write(&journal, invalid).unwrap();
    let capture = crate::http_diagnostics::capture_test_logs(diagnostics::TARGET, &id);
    assert_eq!(
        recover_all(&workspace.journal_root, &workspace.roots)
            .unwrap()
            .blocked,
        [id]
    );
    let messages = capture.messages();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("journal.json")
                && message.contains("类型=Data")
                && message.contains("行=1")
                && message.contains("列="))
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("private marker"))
    );
    assert_eq!(fs::read(journal).unwrap(), invalid);
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
        if during_apply {
            pending.journal.state = JournalState::Applying;
            pending.persist().unwrap();
            pending.apply_record(0, || {}).unwrap();
            assert_eq!(w.read("first.dll"), b"new");
            assert!(!parent.exists());
            fs::create_dir(&parent).unwrap();
            assert!(pending.apply_record(1, || {}).is_err());
            drop(pending);
            assert_eq!(
                recover_all(&w.journal_root, &w.roots)
                    .unwrap()
                    .recovered
                    .len(),
                1
            );
        } else {
            fs::create_dir(&parent).unwrap();
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
