use super::*;

#[test]
fn mandatory_update_rejects_cancel_and_duplicate_start_until_guard_drops() {
    let control = DownloadControl::default();
    assert!(control.cancel());
    assert_eq!(control.0.load(Ordering::Acquire), DOWNLOAD_CANCELLED);
    let guard = control
        .begin_update()
        .expect("update clears old cancellation");
    assert!(control.updating());
    assert!(control.begin_update().is_none());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let control = &control;
            scope.spawn(move || {
                for _ in 0..100 {
                    assert!(!control.cancel());
                    control.start();
                    assert!(control.updating());
                }
            });
        }
    });
    drop(guard);
    assert!(!control.updating());
    assert!(control.cancel());
    control.start();
    assert_eq!(control.0.load(Ordering::Acquire), DOWNLOAD_IDLE);
    assert!(control.begin_update().is_some());
}

#[test]
fn mandatory_update_error_releases_guard_without_leaving_cancellation() {
    let control = DownloadControl::default();
    let fail = || -> Result<(), &str> {
        let _guard = control.begin_update().unwrap();
        Err("download or signature failure")
    };
    assert!(fail().is_err());
    assert_eq!(control.0.load(Ordering::Acquire), DOWNLOAD_IDLE);
    assert!(control.cancel());
}
