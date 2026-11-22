use super::*;

fn start(updates: &AppUpdates) -> StageGuard<'_> {
    match updates.start_download().unwrap() {
        DownloadStart::Started(guard) => guard,
        DownloadStart::Ready(_) => panic!("fresh update slot should start a download"),
    }
}

#[test]
fn only_one_background_download_runs_and_retry_can_acquire_the_slot() {
    let updates = AppUpdates::default();
    let first = start(&updates);
    assert!(matches!(updates.start_download(), Err(error) if error == IN_PROGRESS));
    assert!(!first.cancelled());
    drop(first);
    assert!(!start(&updates).cancelled());
}

#[test]
fn ordinary_exit_cancels_the_update_and_waits_for_the_download_to_release_its_slot() {
    let updates = AppUpdates::default();
    let download = start(&updates);
    let closing = updates.closing().unwrap();
    assert!(download.cancelled());
    assert!(matches!(updates.start_download(), Err(error) if error == IN_PROGRESS));
    assert!(matches!(updates.closing(), Err(error) if error == IN_PROGRESS));
    std::thread::scope(|scope| {
        scope.spawn(move || drop(download));
        assert!(closing.wait_for_download(Duration::from_secs(5)).unwrap());
    });
    assert!(closing.take_ready(UpdateOnExit::Skip).unwrap().is_none());
    assert!(closing.take_ready(UpdateOnExit::Install).unwrap().is_none());
    assert!(
        matches!(closing.take_ready(UpdateOnExit::Restart), Err(error) if error == UNAVAILABLE)
    );
    drop(closing);
    assert!(!start(&updates).cancelled());
}

#[test]
fn stalled_staging_does_not_prevent_exit_after_the_cancel_deadline() {
    let updates = AppUpdates::default();
    let download = start(&updates);
    let closing = updates.closing().unwrap();
    assert!(download.cancelled());
    assert!(!closing.wait_for_download(Duration::ZERO).unwrap());
    assert!(closing.take_ready(UpdateOnExit::Install).unwrap().is_none());
    drop(closing);
    assert!(matches!(updates.start_download(), Err(error) if error == IN_PROGRESS));
    drop(download);
    assert!(!start(&updates).cancelled());
}

#[test]
fn mandatory_recovery_disallows_download_and_install_handoff() {
    let updates = AppUpdates::default();
    let download = start(&updates);
    let info = serde_json::from_value(serde_json::json!({
        "version": "99.1.0", "current_version": "99.0.0",
        "installer_name": "update.exe", "installer_type": "exe", "installer_signature": "fixture",
    }))
    .unwrap();
    let alternate_link = "https://example.com/releases".to_owned();
    updates
        .require_recovery(Recovery {
            update: info,
            alternate_link: alternate_link.clone(),
        })
        .unwrap();
    assert!(download.cancelled());
    drop(download);
    assert_eq!(
        updates.recovery().unwrap().unwrap().alternate_link,
        alternate_link
    );
    assert!(matches!(updates.start_download(), Err(error) if error == APPLY_FAILED));
    let closing = updates.closing().unwrap();
    for update in [
        UpdateOnExit::Skip,
        UpdateOnExit::Install,
        UpdateOnExit::Restart,
    ] {
        assert!(matches!(closing.take_ready(update), Err(error) if error == APPLY_FAILED));
    }
}
