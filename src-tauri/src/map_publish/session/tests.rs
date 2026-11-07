use super::*;

#[test]
fn access_tokens_count_as_expired_shortly_before_the_deadline() {
    assert!(!access_token_expired(None, 10_000));
    assert!(!access_token_expired(
        Some(10_000 + ACCESS_EXPIRY_MARGIN_SECS + 1),
        10_000
    ));
    assert!(access_token_expired(
        Some(10_000 + ACCESS_EXPIRY_MARGIN_SECS),
        10_000
    ));
    assert!(access_token_expired(Some(9_000), 10_000));
}

#[test]
fn refresh_tokens_must_exist_and_be_unexpired() {
    assert!(refresh_token_usable(Some("ghr_b"), None, 10));
    assert!(refresh_token_usable(Some("ghr_b"), Some(11), 10));
    assert!(!refresh_token_usable(Some("ghr_b"), Some(10), 10));
    assert!(!refresh_token_usable(Some(" "), None, 10));
    assert!(!refresh_token_usable(None, None, 10));
}

#[test]
fn refresh_rejections_map_to_reauthorize_tags() {
    assert_eq!(
        refresh_rejection_reason("bad_refresh_token", Some("expired")),
        "mapPublish.sessionExpired"
    );
    assert!(
        refresh_rejection_reason("unauthorized_client", Some("nope"))
            .starts_with("mapPublish.sessionRefreshRejected?")
    );
}
