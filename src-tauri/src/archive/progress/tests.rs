use super::*;

#[test]
fn waiting_progress_is_visible_before_size_or_bytes_and_omits_connection_context() {
    let progress = DownloadProgress::connecting("MOD 下载");
    let event = serde_json::to_value(&progress).unwrap();
    assert_eq!(event["label"], "MOD 下载");
    assert_eq!(event["phase"], "connecting");
    assert_eq!(event["downloaded"], 0);
    assert!(event["total"].is_null());
    assert_eq!(event["finished"], false);
    for key in ["route", "source", "attempt", "attempt_total"] {
        assert!(!event.as_object().unwrap().contains_key(key));
    }
}

#[tokio::test]
async fn observers_are_scoped_to_one_future_and_release_after_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let result =
        observe_download_progress(move |value| captured.lock().unwrap().push(value), async {
            emit_progress_event(None, DownloadProgress::connecting("first"));
            Err::<(), _>("download.cancelled")
        })
        .await;
    assert_eq!(result, Err("download.cancelled"));
    emit_progress_event(None, DownloadProgress::connecting("late"));
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(seen.lock().unwrap()[0].label, "first");
}

#[tokio::test]
async fn simultaneous_observers_do_not_share_progress_and_nesting_restores_the_parent() {
    let first = Arc::new(Mutex::new(Vec::new()));
    let second = Arc::new(Mutex::new(Vec::new()));
    let (a, b, nested) = (first.clone(), second.clone(), second.clone());
    tokio::join!(
        observe_download_progress(move |p| a.lock().unwrap().push(p.label), async {
            emit_progress_event(None, DownloadProgress::connecting("outer-before"));
            observe_download_progress(move |p| nested.lock().unwrap().push(p.label), async {
                tokio::task::yield_now().await;
                emit_progress_event(None, DownloadProgress::connecting("inner"));
            })
            .await;
            emit_progress_event(None, DownloadProgress::connecting("outer-after"));
        }),
        observe_download_progress(move |p| b.lock().unwrap().push(p.label), async {
            tokio::task::yield_now().await;
            emit_progress_event(None, DownloadProgress::connecting("parallel"));
        })
    );
    assert_eq!(*first.lock().unwrap(), ["outer-before", "outer-after"]);
    let mut second = second.lock().unwrap().clone();
    second.sort();
    assert_eq!(second, ["inner", "parallel"]);
}
