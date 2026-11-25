use super::*;

fn offer(version: &str) -> AppUpdateInfo {
    serde_json::from_value(serde_json::json!({
        "version": version,
        "current_version": "99.0.0",
        "installer_name": format!("OC2ModManager_{version}_x64-setup.exe"),
        "installer_type": "exe",
        "installer_url": "https://example.test/signed-installer",
        "installer_signature": "fixture",
        "install_mode": "manual",
    }))
    .unwrap()
}

fn confirm(updates: &AppUpdates, version: &str) {
    updates
        .confirm_offer(Some(offer(version)), None, AppUpdateInstallMode::Deferred)
        .unwrap();
}

fn start(updates: &AppUpdates) -> StageGuard<'_> {
    match updates.start_download("99.1.0").unwrap() {
        DownloadStart::Started(guard) => guard,
        DownloadStart::Ready(_) => panic!("fresh update slot should start a download"),
    }
}

#[test]
fn only_one_background_download_runs_and_retry_can_acquire_the_slot() {
    let updates = AppUpdates::default();
    confirm(&updates, "99.1.0");
    let first = start(&updates);
    assert!(matches!(updates.start_download("99.1.0"), Err(error) if error == IN_PROGRESS));
    assert!(!first.cancelled());
    drop(first);
    assert!(!start(&updates).cancelled());
}

#[test]
fn exit_validates_before_cancel_and_waits_for_the_download_to_release_its_slot() {
    let updates = AppUpdates::default();
    confirm(&updates, "99.1.0");
    let download = start(&updates);
    let closing = updates.closing().unwrap();
    assert!(!download.cancelled());
    assert!(matches!(updates.start_download("99.1.0"), Err(error) if error == IN_PROGRESS));
    assert!(matches!(updates.closing(), Err(error) if error == IN_PROGRESS));
    assert!(!closing.wait_for_download(Duration::ZERO).unwrap());
    assert!(download.cancelled());
    std::thread::scope(|scope| {
        scope.spawn(move || drop(download));
        assert!(closing.wait_for_download(Duration::from_secs(5)).unwrap());
    });
    assert!(matches!(closing.take_ready("99.1.0"), Err(error) if error == UNAVAILABLE));
    drop(closing);
    assert!(!start(&updates).cancelled());
}

#[test]
fn quit_installation_cannot_be_deferred_before_staging() {
    let updates = AppUpdates::default();
    assert!(matches!(updates.defer("99.1.0"), Err(error) if error == UNAVAILABLE));
    confirm(&updates, "99.1.0");
    let _download = start(&updates);
    assert!(matches!(updates.defer("99.1.0"), Err(error) if error == UNAVAILABLE));
    let closing = updates.closing().unwrap();
    assert!(matches!(updates.defer("99.1.0"), Err(error) if error == IN_PROGRESS));
    assert!(!closing.installs_on_quit().unwrap());
}

#[test]
fn lower_or_offline_checks_cannot_clear_a_confirmed_newer_update() {
    let updates = AppUpdates::default();
    confirm(&updates, "99.2.0");
    confirm(&updates, "99.1.0");
    let retained = updates
        .confirm_offer(None, None, AppUpdateInstallMode::Deferred)
        .unwrap()
        .unwrap();
    assert_eq!(retained.version, "99.2.0");
    assert!(matches!(updates.start_download("99.1.0"), Err(error) if error == UNAVAILABLE));
}

#[test]
fn a_newer_release_without_a_signed_package_stays_mandatory_but_never_stages() {
    let updates = AppUpdates::default();
    confirm(&updates, "99.1.0");
    let missing = AppUpdateInfo {
        installer_url: None,
        installer_signature: String::new(),
        ..offer("99.2.0")
    };
    let manual = updates
        .confirm_offer(Some(missing), None, AppUpdateInstallMode::Deferred)
        .unwrap()
        .unwrap();
    assert_eq!(manual.version, "99.2.0");
    assert_eq!(manual.install_mode, AppUpdateInstallMode::Manual);
    assert!(matches!(updates.start_download("99.2.0"), Err(error) if error == UNAVAILABLE));
    assert!(matches!(updates.start_download("99.1.0"), Err(error) if error == UNAVAILABLE));
}

#[test]
fn receipt_offer_cannot_replace_an_equal_or_newer_manifest_offer() {
    let updates = AppUpdates::default();
    confirm(&updates, "99.1.0");
    let receipt = |version| AppUpdateInfo {
        installer_url: None,
        installer_signature: String::new(),
        ..offer(version)
    };
    let retained = updates
        .confirm_offer(
            None,
            Some(receipt("99.1.0")),
            AppUpdateInstallMode::Deferred,
        )
        .unwrap()
        .unwrap();
    assert!(retained.has_signed_package());
    let newer = updates
        .confirm_offer(
            None,
            Some(receipt("99.2.0")),
            AppUpdateInstallMode::Deferred,
        )
        .unwrap()
        .unwrap();
    assert_eq!(newer.version, "99.2.0");
    assert_eq!(newer.install_mode, AppUpdateInstallMode::Manual);
}

#[test]
fn staged_identity_requires_the_same_signed_package_as_the_confirmed_offer() {
    let staged = AppUpdateInfo {
        install_mode: AppUpdateInstallMode::Deferred,
        ..offer("99.1.0")
    };
    assert!(matches_package(&staged, &staged));
    let replaced = AppUpdateInfo {
        installer_signature: "new signature".into(),
        ..staged.clone()
    };
    assert!(!matches_package(&replaced, &staged));
    let manual = AppUpdateInfo {
        install_mode: AppUpdateInstallMode::Manual,
        ..staged.clone()
    };
    assert!(!matches_package(&manual, &staged));
}
