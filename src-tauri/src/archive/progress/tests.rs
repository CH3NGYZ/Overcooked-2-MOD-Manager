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
