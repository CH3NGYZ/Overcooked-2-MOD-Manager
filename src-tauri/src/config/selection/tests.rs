use super::*;

#[test]
fn changing_region_reselects_its_default_and_persists_both_directions() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let mut config = service.load().unwrap();
    for region in ["cn", "global", "cn"] {
        // A conflicting stale provider patch cannot override automatic selection.
        service
            .save_with_selection(&mut config, Some(region), Some(DataProvider::Builtin))
            .unwrap();
        let loaded = service.load().unwrap();
        assert_eq!(loaded.region, region);
        assert_eq!(loaded.data_provider, DataProvider::for_region(region));
        assert_eq!(
            loaded.repository_sources[0].url,
            loaded.data_provider.document("mods.toml")
        );
        let stored: AppConfig =
            toml::from_str(&std::fs::read_to_string(service.config_path()).unwrap()).unwrap();
        assert_eq!(stored.region, region);
        config = loaded;
    }
}

#[tokio::test]
async fn region_change_invalidates_pending_work_even_with_the_same_provider() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let mut config = service.load().unwrap();
    service
        .save_with_selection(&mut config, None, Some(DataProvider::Gitcode))
        .unwrap();
    let selection = service.data_selection();
    let (started, ready) = tokio::sync::oneshot::channel();
    let scoped = service.in_data_selection(selection, async {
        started.send(()).unwrap();
        std::future::pending::<()>().await;
        Ok::<_, String>(())
    });
    let change = async {
        ready.await.unwrap();
        service
            .save_with_selection(&mut config, Some("cn"), None)
            .unwrap();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(scoped, change)
    })
    .await
    .expect("region change must cancel old work");
    assert_eq!(result.err().as_deref(), Some("source.addressMismatch"));
    assert_eq!(service.data_selection().0, selection.0);
    assert!(service.data_selection().1 > selection.1);
    assert_eq!(service.load().unwrap().region, "cn");
}

#[test]
fn unchanged_region_keeps_a_manual_provider_and_invalid_region_changes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let mut config = service.load().unwrap();
    service
        .save_with_selection(&mut config, None, Some(DataProvider::Builtin))
        .unwrap();
    let selection = service.data_selection();
    service
        .save_with_selection(&mut config, Some("global"), None)
        .unwrap();
    assert_eq!(service.data_selection(), selection);
    let stored = std::fs::read(service.config_path()).unwrap();
    assert!(
        service
            .save_with_selection(&mut config, Some("invalid"), None)
            .is_err()
    );
    assert_eq!(service.data_selection(), selection);
    assert_eq!(std::fs::read(service.config_path()).unwrap(), stored);
}

#[test]
fn stale_saves_and_error_defaults_cannot_undo_the_current_region() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let stale = service.load().unwrap();
    let mut config = stale.clone();
    service
        .save_with_selection(&mut config, Some("cn"), None)
        .unwrap();
    service.save(&stale).unwrap();
    assert_eq!(service.load().unwrap().region, "cn");
    let isolated = ConfigService::for_test(root.path().join("isolated"));
    assert_eq!(isolated.load().unwrap().region, "global");
    std::fs::remove_file(service.config_path()).unwrap();
    std::fs::create_dir(service.config_path()).unwrap();
    let fallback = service.load_or_default();
    assert_eq!(fallback.region, "cn");
    assert_eq!(fallback.data_provider, DataProvider::Gitcode);
}

#[test]
fn concurrent_readers_never_mix_a_region_and_its_automatic_source() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let mut config = service.load().unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let reader = service.clone();
            scope.spawn(move || {
                for _ in 0..40 {
                    let config = reader.load().unwrap();
                    assert_eq!(
                        config.data_provider,
                        DataProvider::for_region(&config.region)
                    );
                    assert_eq!(
                        config.repository_sources[0].url,
                        config.data_provider.document("mods.toml")
                    );
                }
            });
        }
        for _ in 0..10 {
            for region in ["cn", "global"] {
                service
                    .save_with_selection(&mut config, Some(region), None)
                    .unwrap();
            }
        }
    });
}

#[cfg(windows)]
#[test]
fn failed_disk_save_preserves_region_provider_and_generation() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::for_test(root.path().into());
    let mut config = service.load().unwrap();
    let selection = service.data_selection();
    let stored = std::fs::read(service.config_path()).unwrap();
    let caller_before = serde_json::to_value(&config).unwrap();
    let changes = service.data_selection_changes();
    let _locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(service.config_path())
        .unwrap();
    assert!(
        service
            .save_with_selection(&mut config, Some("cn"), None)
            .is_err()
    );
    assert_eq!(service.data_selection(), selection);
    assert!(!changes.has_changed().unwrap());
    assert_eq!(serde_json::to_value(&config).unwrap(), caller_before);
    assert_eq!(service.load().unwrap().region, "global");
    assert_eq!(std::fs::read(service.config_path()).unwrap(), stored);
}
