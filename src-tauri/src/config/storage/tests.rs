use super::*;
use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

fn assert_no_transaction_files(root: &Path) {
    let leftovers = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "tmp")
                || path
                    .file_name()
                    .is_some_and(|name| name == "config.previous.toml")
        })
        .collect::<Vec<_>>();
    assert!(leftovers.is_empty(), "transaction leftovers: {leftovers:?}");
}

#[test]
fn cloned_services_serialize_concurrent_saves_and_reads() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let mut initial = default_config();
    initial.game.path = "config-transaction-sentinel".into();
    initial.network.timeout_seconds = 50;
    service.save(&initial).unwrap();
    let barrier = Arc::new(Barrier::new(16));
    let workers = (0..16)
        .map(|worker| {
            let service = service.clone();
            let barrier = barrier.clone();
            let mut config = initial.clone();
            std::thread::spawn(move || -> Result<(), String> {
                barrier.wait();
                if worker < 12 {
                    config.network.timeout_seconds = 51 + worker;
                    for _ in 0..8 {
                        service.save(&config).map_err(|error| error.to_string())?;
                    }
                } else {
                    for _ in 0..32 {
                        let config = service.load().map_err(|error| error.to_string())?;
                        if config.game.path != "config-transaction-sentinel"
                            || !(50..=62).contains(&config.network.timeout_seconds)
                        {
                            return Err("read observed a default or unrelated configuration".into());
                        }
                    }
                }
                Ok(())
            })
        })
        .collect::<Vec<_>>();
    let failures = workers
        .into_iter()
        .filter_map(|worker| worker.join().unwrap().err())
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "concurrent config failures: {failures:?}"
    );
    let persisted: AppConfig =
        toml::from_str(&fs::read_to_string(service.config_path()).unwrap()).unwrap();
    let cached = service.load().unwrap();
    assert_eq!(
        cached.network.timeout_seconds,
        persisted.network.timeout_seconds
    );
    assert_eq!(cached.game.path, persisted.game.path);
    assert_no_transaction_files(root.path());
}

fn load_with_deadline(service: ConfigService) -> Result<AppConfig, ConfigError> {
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        sender.send(service.load()).unwrap();
    });
    let result = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("load re-entered its storage lock while persisting defaults or migrations");
    worker.join().unwrap();
    result
}

#[test]
fn first_load_and_schema_migration_can_save_inside_the_read_transaction() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let config = load_with_deadline(service.clone()).unwrap();
    assert_eq!(
        config.schema_version,
        super::super::defaults::CONFIG_SCHEMA_VERSION
    );
    let mut legacy = default_config();
    legacy.schema_version = 1;
    legacy.ui_zoom = 1.75;
    fs::write(
        service.config_path(),
        toml::to_string_pretty(&legacy).unwrap(),
    )
    .unwrap();
    // A fresh service forces a disk read rather than relying on filesystem timestamp resolution.
    let loaded = load_with_deadline(ConfigService::for_test(root.path().into())).unwrap();
    assert_eq!(
        loaded.schema_version,
        super::super::defaults::CONFIG_SCHEMA_VERSION
    );
    assert_eq!(loaded.ui_zoom, 1.75);
    let persisted: AppConfig =
        toml::from_str(&fs::read_to_string(service.config_path()).unwrap()).unwrap();
    assert_eq!(persisted.schema_version, loaded.schema_version);
    assert_no_transaction_files(root.path());
}

#[test]
fn corrupt_config_recovery_can_save_inside_the_read_transaction() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    fs::write(service.config_path(), "invalid = [").unwrap();
    assert!(matches!(
        load_with_deadline(service.clone()),
        Err(ConfigError::Parse(_))
    ));
    assert!(service.load().is_ok());
    assert!(fs::read_dir(root.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("config.corrupt-")
    }));
    assert_no_transaction_files(root.path());
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Instant;

    fn open_without_delete_sharing(path: &Path) -> fs::File {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(path)
            .unwrap()
    }

    fn assert_sharing_violation(error: ConfigError) {
        let ConfigError::Write(reason) = error else {
            panic!("expected a configuration write error, got {error}");
        };
        assert!(
            reason.contains("os error 32"),
            "original system reason lost: {reason}"
        );
    }

    #[test]
    fn persistent_external_target_lock_preserves_original_and_cleans_own_temp() {
        let root = tempfile::tempdir().unwrap();
        let service = ConfigService::for_test(root.path().into());
        let config = default_config();
        service.save(&config).unwrap();
        let original = fs::read(service.config_path()).unwrap();
        let locked = open_without_delete_sharing(&service.config_path());
        let mut replacement = config.clone();
        replacement.ui_zoom = 1.75;
        let started = Instant::now();
        assert_sharing_violation(service.clone().save(&replacement).unwrap_err());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "persistent lock was not bounded"
        );
        assert_eq!(fs::read(service.config_path()).unwrap(), original);
        assert_eq!(service.load().unwrap().ui_zoom, config.ui_zoom);
        assert_no_transaction_files(root.path());
        drop(locked);
        service.save(&replacement).unwrap();
        assert_eq!(service.load().unwrap().ui_zoom, replacement.ui_zoom);
        assert_no_transaction_files(root.path());
    }

    #[test]
    fn external_backup_lock_preserves_both_original_and_recovery_material() {
        let root = tempfile::tempdir().unwrap();
        let service = ConfigService::for_test(root.path().into());
        let config = default_config();
        service.save(&config).unwrap();
        let original = fs::read(service.config_path()).unwrap();
        let previous = root.path().join("config.previous.toml");
        fs::write(&previous, b"previous recovery material").unwrap();
        let locked = open_without_delete_sharing(&previous);
        let expected_reason = fs::remove_file(&previous).unwrap_err().to_string();
        let mut replacement = config.clone();
        replacement.ui_zoom = 1.75;
        let ConfigError::Write(reason) = service.save(&replacement).unwrap_err() else {
            panic!("expected the original backup removal error");
        };
        assert_eq!(reason, expected_reason, "original system reason changed");
        assert_eq!(fs::read(service.config_path()).unwrap(), original);
        assert_eq!(fs::read(&previous).unwrap(), b"previous recovery material");
        let own_temps = fs::read_dir(root.path())
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "tmp")
            })
            .count();
        assert_eq!(own_temps, 0, "failed save leaked a temporary configuration");
        drop(locked);
        service.save(&replacement).unwrap();
        assert_no_transaction_files(root.path());
    }

    #[test]
    fn brief_external_target_lock_can_clear_during_a_real_service_save() {
        let root = tempfile::tempdir().unwrap();
        let service = ConfigService::for_test(root.path().into());
        let mut config = default_config();
        service.save(&config).unwrap();
        let locked = open_without_delete_sharing(&service.config_path());
        config.ui_zoom = 1.75;
        let saving = service.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || sender.send(saving.save(&config)).unwrap());
        let deadline = Instant::now() + Duration::from_secs(1);
        while !fs::read_dir(root.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|extension| extension == "tmp")
        }) {
            assert!(
                Instant::now() < deadline,
                "save never created its temporary file"
            );
            std::thread::yield_now();
        }
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        // Keep the real target handle closed to delete sharing briefly, then
        // release it while the service's bounded rename retry is still active.
        std::thread::sleep(Duration::from_millis(20));
        drop(locked);
        receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(service.load().unwrap().ui_zoom, 1.75);
        assert_no_transaction_files(root.path());
    }

    #[test]
    fn sharing_retry_retries_a_real_rename_after_the_first_handle_conflict() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.toml");
        let destination = root.path().join("destination.toml");
        fs::write(&source, b"original").unwrap();
        let mut locked = Some(open_without_delete_sharing(&source));
        let mut attempts = 0;
        transaction::retry_sharing_violation(|| {
            attempts += 1;
            let result = fs::rename(&source, &destination);
            if attempts == 1 {
                assert_eq!(result.as_ref().unwrap_err().raw_os_error(), Some(32));
                drop(locked.take());
            }
            result
        })
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(fs::read(&destination).unwrap(), b"original");
    }
}
