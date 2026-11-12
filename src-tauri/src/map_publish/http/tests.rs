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

#[test]
fn publish_requests_have_one_sensitive_authorization_header() {
    let client = reqwest::Client::new();
    for provider in [PublishProvider::Github, PublishProvider::Gitcode] {
        for method in [
            reqwest::Method::GET,
            reqwest::Method::POST,
            reqwest::Method::PATCH,
        ] {
            let request = authorized(
                client.request(method.clone(), "http://localhost/releases"),
                "fixture-token",
                provider,
            )
            .build()
            .unwrap();
            let headers = request.headers().get_all(reqwest::header::AUTHORIZATION);
            assert_eq!(headers.iter().count(), 1, "{provider:?} {method}");
            let header = headers.iter().next().unwrap();
            assert_eq!(header, "Bearer fixture-token");
            assert!(header.is_sensitive());
        }
    }
}

async fn fixture_failure(status: u16, body: String) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let size = socket.read(&mut buffer).unwrap();
            assert_ne!(size, 0);
            request.extend_from_slice(&buffer[..size]);
            assert!(request.len() < 8192);
        }
        write!(socket, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap()
        .get(endpoint)
        .send()
        .await
        .unwrap();
    let error = ensure_success(response, "save-release")
        .await
        .expect_err("fixture response must fail");
    worker.join().unwrap();
    error
}

fn failure_reason(tag: &str) -> String {
    let query = tag.split_once('?').unwrap().1;
    reqwest::Url::parse(&format!("http://localhost/?{query}"))
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "reason")
        .unwrap()
        .1
        .into_owned()
}

#[tokio::test]
async fn gitcode_http_errors_preserve_the_platform_reason() {
    for body in [
        serde_json::json!({"error_message": "Cannot create tag in an image repository"}),
        serde_json::json!({"message": "", "error_message": "Cannot create tag in an image repository"}),
        serde_json::json!({"msg": "Cannot create tag in an image repository", "code": 400}),
    ] {
        let error = fixture_failure(400, body.to_string()).await;
        assert!(error.starts_with("mapPublish.requestFailed?action=save-release&"));
        assert_eq!(
            failure_reason(&error),
            "Cannot create tag in an image repository"
        );
    }
    let error = fixture_failure(
        422,
        serde_json::json!({"message": "Validation Failed", "error_message": "secondary"})
            .to_string(),
    )
    .await;
    assert!(error.starts_with("mapPublish.rejected?reason="));
    assert_eq!(failure_reason(&error), "Validation Failed");
}

#[tokio::test]
async fn http_error_details_are_plain_bounded_and_redacted() {
    let fixture_secret = ["fixture", "secret", "value"].join("-");
    let error = fixture_failure(
        400,
        serde_json::json!({"error_message": format!("Tag rejected\n\u{1b}access_token={fixture_secret}")})
            .to_string(),
    )
    .await;
    let reason = failure_reason(&error);
    assert!(reason.starts_with("Tag rejected"));
    assert!(!reason.contains(&fixture_secret));
    assert!(!reason.contains('\n'));
    assert!(!reason.contains('\u{1b}'));

    let error = fixture_failure(
        400,
        serde_json::json!({"message": "Rejected Bearer fixture-bearer-secret at https://localhost/?access_token=fixture-query-secret"})
            .to_string(),
    )
    .await;
    let reason = failure_reason(&error);
    assert!(!reason.contains("fixture-bearer-secret"));
    assert!(!reason.contains("fixture-query-secret"));

    let error = fixture_failure(
        400,
        serde_json::json!({"error_message": "错".repeat(2000)}).to_string(),
    )
    .await;
    let reason = failure_reason(&error);
    assert!(reason.starts_with('错'));
    assert!(reason.chars().count() <= 512);

    let error = fixture_failure(
        400,
        serde_json::json!({"error_message": "错".repeat(20_000)}).to_string(),
    )
    .await;
    assert_eq!(failure_reason(&error), "HTTP 400");

    for body in [
        "<html><body>fixture-private-page</body></html>".to_string(),
        serde_json::json!({"message": "<script>fixture-private-page</script>"}).to_string(),
        serde_json::json!({"access_token": fixture_secret, "code": 400}).to_string(),
    ] {
        let error = fixture_failure(400, body).await;
        assert_eq!(failure_reason(&error), "HTTP 400");
    }
}
