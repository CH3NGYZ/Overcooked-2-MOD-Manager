use super::*;
use crate::level_sets::recompress::replacement::hash;
use std::{fs, sync::mpsc, time::Duration};

struct Fixture {
    _directory: tempfile::TempDir,
    registry: Registry,
    root: PathBuf,
    source: PathBuf,
    backup: PathBuf,
    id: String,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let source = root.join("source");
        let backup = root.join("backup");
        fs::write(&source, b"partial write").unwrap();
        fs::write(&backup, b"original").unwrap();
        let registry = Registry::default();
        let id = registry.register(
            (&root, &source),
            &backup,
            FileIdentity::capture(&source).unwrap().unwrap(),
            &hash(&backup).unwrap(),
            Some(hash(&source).unwrap()),
            "initial failure",
        );
        Self {
            _directory: directory,
            registry,
            root,
            source,
            backup,
            id,
        }
    }

    fn retry(&self) -> Result<(), String> {
        self.registry
            .retry(&self.id, Some(&root_key(&self.root)), &|_| Ok(()))
    }

    fn assert_pending(&self, error: &str) {
        let rows = self.registry.list(&self.root);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].restore_error.as_deref(), Some(error));
        assert!(self.registry.blocks_conversion(&self.source));
    }
}

#[test]
fn retry_restores_original_and_clears_only_the_recovery_record() {
    let fixture = Fixture::new();
    assert!(fixture.registry.has_pending(&fixture.root));
    fixture.retry().unwrap();
    assert!(!fixture.registry.has_pending(&fixture.root));
    assert!(!fixture.registry.blocks_conversion(&fixture.source));
    assert_eq!(fs::read(&fixture.source).unwrap(), b"original");
    assert_eq!(fs::read(&fixture.backup).unwrap(), b"original");
}

#[test]
fn retry_preserves_source_modified_after_failed_rollback() {
    let fixture = Fixture::new();
    fs::write(&fixture.source, b"user edit after recovery failure").unwrap();
    assert_eq!(fixture.retry().unwrap_err(), SOURCE_CHANGED);
    assert_eq!(
        fs::read(&fixture.source).unwrap(),
        b"user edit after recovery failure"
    );
    fixture.assert_pending(SOURCE_CHANGED);
}

#[test]
fn replace_with_same_bytes_is_not_the_original_file() {
    let fixture = Fixture::new();
    let moved = fixture.root.join("moved");
    fs::rename(&fixture.source, &moved).unwrap();
    fs::write(&fixture.source, b"partial write").unwrap();
    assert_eq!(fixture.retry().unwrap_err(), SOURCE_CHANGED);
    assert_eq!(fs::read(&fixture.source).unwrap(), b"partial write");
    assert_eq!(fs::read(moved).unwrap(), b"partial write");
    fixture.assert_pending(SOURCE_CHANGED);
}

#[test]
fn new_hard_link_to_source_is_not_overwritten() {
    let fixture = Fixture::new();
    let link = fixture.root.join("linked-copy");
    fs::hard_link(&fixture.source, &link).unwrap();
    assert_eq!(fixture.retry().unwrap_err(), SOURCE_CHANGED);
    assert_eq!(fs::read(link).unwrap(), b"partial write");
    fixture.assert_pending(SOURCE_CHANGED);
}

#[test]
fn retry_and_exit_do_not_recreate_deleted_sources() {
    let fixture = Fixture::new();
    fs::remove_file(&fixture.source).unwrap();
    assert_eq!(fixture.retry().unwrap_err(), SOURCE_CHANGED);
    assert_eq!(fixture.registry.restore_all(&|_| Ok(())), 1);
    assert!(!fixture.source.exists());
    fixture.assert_pending(SOURCE_CHANGED);
}

#[test]
fn ignore_keeps_source_and_backup_and_suppresses_future_conversion() {
    let fixture = Fixture::new();
    fixture.registry.ignore(&fixture.root, &fixture.id).unwrap();
    assert!(!fixture.registry.has_pending(&fixture.root));
    assert_eq!(fixture.registry.restore_all(&|_| Ok(())), 0);
    assert!(fixture.registry.blocks_conversion(&fixture.source));
    assert!(!fixture.registry.blocks_conversion(&fixture.backup));
    assert_eq!(fs::read(&fixture.source).unwrap(), b"partial write");
    assert_eq!(fs::read(&fixture.backup).unwrap(), b"original");
}

#[test]
fn changed_backup_is_not_written_and_restored_backup_can_be_retried() {
    let fixture = Fixture::new();
    fs::write(&fixture.backup, b"wrong original").unwrap();
    let error = fixture.retry().unwrap_err();
    fixture.assert_pending(&error);
    assert_eq!(fs::read(&fixture.source).unwrap(), b"partial write");

    fs::write(&fixture.backup, b"original").unwrap();
    assert_eq!(fixture.registry.restore_all(&|_| Ok(())), 0);
    assert_eq!(fs::read(&fixture.source).unwrap(), b"original");
}

#[test]
fn exit_recovery_checks_the_original_game_root_and_keeps_live_game_files_unchanged() {
    let fixture = Fixture::new();
    let other_root = fixture.root.join("other-game");
    fs::create_dir(&other_root).unwrap();
    assert_eq!(
        fixture
            .registry
            .retry(&fixture.id, Some(&root_key(&other_root)), &|_| {
                panic!("another game cannot authorize recovery")
            }),
        Err(NOT_FOUND.into())
    );
    assert_eq!(
        fixture.registry.restore_all(&|game_root| {
            assert_eq!(game_root, fixture.root);
            Err("game.running".into())
        }),
        1
    );
    fixture.assert_pending("game.running");
    assert_eq!(fs::read(&fixture.source).unwrap(), b"partial write");
    assert_eq!(fs::read(&fixture.backup).unwrap(), b"original");
}

#[test]
fn in_flight_recovery_remains_visible_and_excludes_ignore_and_retry() {
    let fixture = Fixture::new();
    let registry = &fixture.registry;
    let (started, wait_for_start) = mpsc::channel();
    let (release, wait_for_release) = mpsc::channel();
    std::thread::scope(|scope| {
        let id = &fixture.id;
        scope.spawn(move || {
            let mut item = registry.begin(id, None).unwrap();
            started.send(()).unwrap();
            wait_for_release
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            let result = restore_unchanged(&mut item, &|_| Ok(()));
            registry.finish(item, &result);
            result.unwrap();
        });
        wait_for_start.recv_timeout(Duration::from_secs(5)).unwrap();
        let visible =
            registry.has_pending(&fixture.root) && registry.blocks_conversion(&fixture.source);
        let ignore = registry.ignore(&fixture.root, &fixture.id);
        let retry = fixture.retry();
        let rest = registry.restore_all(&|_| Ok(()));
        release.send(()).unwrap();
        assert!(visible);
        assert_eq!(ignore.unwrap_err(), REQUIRED);
        assert_eq!(retry.unwrap_err(), REQUIRED);
        assert_eq!(rest, 1);
    });
    assert!(!registry.has_pending(&fixture.root));
    assert_eq!(fs::read(&fixture.source).unwrap(), b"original");
}
