use super::*;

#[test]
fn navigation_and_results_reject_other_origins_credentials_and_paths() {
    let expected = Url::parse("https://gitee.com/explore/topic/overcooked-2?page=1").unwrap();
    for candidate in [
        "https://example.com/explore/topic/overcooked-2",
        "http://gitee.com/explore/topic/overcooked-2",
        "https://user@gitee.com/explore/topic/overcooked-2",
        "https://gitee.com:444/explore/topic/overcooked-2",
        "https://gitee.com/api/v5/repos",
        "https://gitee.com/login",
    ] {
        let url = Url::parse(candidate).unwrap();
        assert!(!same_topic(&url, &expected));
        assert!(
            snapshot_html(
                &serde_json::json!({"state":"ready", "url":candidate, "html":"x"}).to_string(),
                &expected
            )
            .is_err()
        );
    }
}

#[test]
fn challenge_and_previous_page_never_become_successful_empty_results() {
    let expected = Url::parse("https://gitee.com/explore/topic/overcooked-2?page=2").unwrap();
    assert!(snapshot_html("null", &expected).unwrap().is_none());
    assert!(
        snapshot_html(r#"{"state":"waiting"}"#, &expected)
            .unwrap()
            .is_none()
    );
    let ready = |url| {
        serde_json::json!({"state":"ready", "url":url, "html":"<div class='explore-repo__list'></div>"}).to_string()
    };
    assert!(
        snapshot_html(
            &ready("https://gitee.com/explore/topic/overcooked-2"),
            &expected
        )
        .unwrap()
        .is_none()
    );
    assert!(
        snapshot_html(&ready(expected.as_str()), &expected)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        page_number(&Url::parse("https://gitee.com/?page=1&page=2").unwrap()),
        None
    );
}

#[test]
fn snapshot_limits_count_utf8_bytes_and_reject_invalid_or_failed_evaluation() {
    let expected = Url::parse("https://gitee.com/explore/topic/overcooked-2").unwrap();
    for raw in [
        "not JSON",
        r#"{"state":"error"}"#,
        r#"{"state":"oversized"}"#,
    ] {
        assert!(snapshot_html(raw, &expected).is_err());
    }
    let raw = serde_json::json!({"state":"ready", "url":expected, "html":"界".repeat(MAX_HTML_BYTES / 3 + 1)}).to_string();
    assert!(
        snapshot_html(
            &raw,
            &Url::parse("https://gitee.com/explore/topic/overcooked-2").unwrap()
        )
        .is_err()
    );
}
