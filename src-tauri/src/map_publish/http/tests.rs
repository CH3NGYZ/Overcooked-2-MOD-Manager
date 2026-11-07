use super::*;

#[test]
fn forbidden_without_headers_still_reports_the_github_message() {
    let tag = classify_write_failure(
        403,
        None,
        None,
        None,
        "Resource not accessible by integration",
    )
    .expect("403 必须归类");
    assert!(tag.starts_with("mapPublish.forbidden?reason="), "{tag}");
    assert!(tag.contains("Resource%20not%20accessible"), "{tag}");
}

#[test]
fn accepted_permissions_become_a_permission_hint() {
    let tag = classify_write_failure(
        403,
        Some("contents=write,administration=write"),
        None,
        None,
        "Resource not accessible by integration",
    )
    .expect("403 必须归类");
    assert_eq!(
        tag,
        "mapPublish.missingPermission?permission=contents%3Dwrite%2Cadministration%3Dwrite"
    );
}

#[test]
fn rate_limited_writes_carry_the_reset_time() {
    let tag = classify_write_failure(
        403,
        None,
        Some("0"),
        Some("1762264817"),
        "API rate limit exceeded",
    )
    .expect("403 必须归类");
    assert_eq!(tag, "mapPublish.rateLimited?reset=1762264817");
    // 次要限流的正文里有 "rate limit"，但没有 x-ratelimit-reset 头。
    let secondary = classify_write_failure(
        403,
        None,
        None,
        None,
        "You have exceeded a secondary rate limit",
    )
    .expect("次要限流也必须归类");
    assert_eq!(secondary, "mapPublish.rateLimited?reset=");
}

#[test]
fn other_statuses_map_to_their_own_tags() {
    assert_eq!(
        classify_write_failure(401, None, None, None, "Bad credentials"),
        Some("mapPublish.unauthorized".to_string())
    );
    assert!(
        classify_write_failure(404, None, None, None, "Not Found")
            .expect("404 必须归类")
            .starts_with("mapPublish.notFound?reason=")
    );
    assert!(
        classify_write_failure(422, None, None, None, "Validation Failed")
            .expect("422 必须归类")
            .starts_with("mapPublish.rejected?reason=")
    );
    // 其它状态交给动作名兜底，分类函数不吞掉它。
    assert_eq!(classify_write_failure(500, None, None, None, "boom"), None);
}
