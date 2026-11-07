use super::*;

fn token_body(json: &str) -> TokenResponse {
    serde_json::from_str(json).unwrap()
}

#[test]
fn expiring_token_responses_become_absolute_deadlines() {
    let body = token_body(
        r#"{"access_token":"ghu_a","expires_in":28800,"refresh_token":"ghr_b",
                "refresh_token_expires_in":15897600,"scope":"","token_type":"bearer"}"#,
    );
    let tokens = tokens_from_response(&body, 1_000).unwrap();
    assert_eq!(tokens.access_token, "ghu_a");
    assert_eq!(tokens.refresh_token.as_deref(), Some("ghr_b"));
    assert_eq!(tokens.expires_at, Some(1_000 + 28_800));
    assert_eq!(tokens.refresh_expires_at, Some(1_000 + 15_897_600));
}

#[test]
fn non_expiring_token_responses_have_no_refresh_side() {
    // App 关了令牌过期：只有 access_token，不过期，也不需要刷新
    let tokens = tokens_from_response(&token_body(r#"{"access_token":"ghu_a"}"#), 1_000).unwrap();
    assert_eq!(tokens.refresh_token, None);
    assert_eq!(tokens.expires_at, None);
    assert_eq!(tokens.refresh_expires_at, None);
    assert!(tokens_from_response(&token_body(r#"{"access_token":"  "}"#), 1).is_none());
    assert!(tokens_from_response(&token_body(r#"{"error":"bad_refresh_token"}"#), 1).is_none());
}
