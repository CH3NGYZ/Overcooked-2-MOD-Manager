use super::*;

fn request() -> reqwest::RequestBuilder {
    reqwest::Client::new().post("https://localhost.invalid/release")
}

#[test]
fn nested_json_preserves_diagnostics_and_redacts_secret_subtrees() {
    let redactor = Redactor::new(&request().build().unwrap());
    let body = serde_json::json!({
        "error_message": "Tag rejected", "code": 400,
        "errors": [{"field": "tag_name", "message": "required", "accessToken": "hidden-token"}],
        "credentials": {"nested": ["hidden-nested"]}, "metadata": {"author": "厨师"},
        "X-API-Key": "hidden-key", "signature": "hidden-signature"
    });
    let result = redactor.body(
        body.to_string().as_bytes(),
        "application/problem+json",
        false,
    );
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["errors"][0]["accessToken"], HIDDEN);
    assert_eq!(parsed["code"], 400);
    assert_eq!(parsed["error_message"], "Tag rejected");
    assert_eq!(parsed["errors"][0]["message"], "required");
    assert_eq!(parsed["metadata"]["author"], "厨师");
    for secret in [
        "hidden-token",
        "hidden-nested",
        "hidden-key",
        "hidden-signature",
    ] {
        assert!(!result.contains(secret));
    }
}

#[test]
fn request_header_query_json_secrets_are_hidden_when_echoed_under_other_keys() {
    let password = ["body", "fixture"].join("-");
    let request = request()
        .query(&[("code", "oauth-code-fixture"), ("signature", "signed-query-fixture")])
        .bearer_auth("bearer-fixture")
        .header("Cookie", "session=cookie-fixture; other=another-cookie-fixture")
        .header("X-OC2-Token", "handshake-fixture")
        .json(&serde_json::json!({"nested": {"password": password}, "code_verifier": "verifier-fixture", "tag_name": "v1"}))
        .build().unwrap();
    let redactor = Redactor::new(&request);
    let request_body = redactor.request_body(&request);
    assert!(request_body.contains("v1"));
    let echoes = [
        "oauth-code-fixture",
        "signed-query-fixture",
        "bearer-fixture",
        "cookie-fixture",
        "another-cookie-fixture",
        "handshake-fixture",
        "body-fixture",
        "verifier-fixture",
    ];
    let result = redactor.body(
        serde_json::json!({"error_message": echoes.join(" "), "code": 400})
            .to_string()
            .as_bytes(),
        "application/json",
        false,
    );
    for secret in echoes {
        assert!(!result.contains(secret), "{secret}");
        assert!(!request_body.contains(secret));
    }
    assert_eq!(serde_json::from_str::<Value>(&result).unwrap()["code"], 400);
}

#[test]
fn form_values_and_encoded_echoes_are_redacted() {
    let request = request()
        .form(&[
            ("client_secret", "a+b/ c"),
            ("code", "form-code-fixture"),
            ("grant_type", "authorization_code"),
        ])
        .build()
        .unwrap();
    let redactor = Redactor::new(&request);
    let output = redactor.request_body(&request);
    assert!(!output.contains("a+b/ c"));
    assert!(!output.contains("form-code-fixture"));
    for text in ["a+b/ c", "a%2Bb%2F%20c", "a%2Bb%2F+c", "form-code-fixture"] {
        assert_eq!(redactor.text(text), HIDDEN);
        let echo = serde_json::json!({"error_message": text, "code": 400});
        let output = redactor.body(echo.to_string().as_bytes(), "application/json", false);
        let output: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["error_message"], HIDDEN);
        assert_eq!(output["code"], 400);
    }
}

#[test]
fn url_userinfo_and_query_echoes_are_hidden() {
    let request = reqwest::Request::new(
        reqwest::Method::GET,
        reqwest::Url::parse(
            "https://fixture-user:fixture%2Fpass@localhost.invalid/?access_token=fixture%2Fquery",
        )
        .unwrap(),
    );
    let redactor = Redactor::new(&request);
    let mut expected = vec![
        request.url().username().to_string(),
        request.url().password().unwrap().to_string(),
    ];
    expected.extend(
        request
            .url()
            .query_pairs()
            .map(|(_, value)| value.into_owned()),
    );
    for value in &expected {
        assert!(!redactor.text(value).contains(value));
    }
}

#[test]
fn plain_text_supports_chinese_html_and_control_cleanup_without_urls_or_credentials() {
    let text = redact_text(
        "中文\n\u{1b}错误\u{202e} https://localhost.invalid/private?token=fixture-secret",
    );
    assert!(text.contains("中文"));
    assert!(text.contains("错误"));
    assert!(!text.contains("https://"));
    assert!(!text.contains("fixture-secret"));
    assert!(!text.chars().any(char::is_control));
    let redactor = Redactor::new(&request().build().unwrap());
    let result = redactor.body(
        b"<html>Tag rejected <b>required</b></html>",
        "text/html",
        false,
    );
    assert!(result.contains("Tag rejected"));
    assert!(result.contains("required"));
    for text in [
        "access_token=hidden-fixture",
        "Authorization: Bearer hidden-fixture",
        "code_verifier=hidden-fixture",
        "Cookie: hidden-fixture",
        "Bearer hidden-fixture",
        "github_pat_hidden_fixture",
    ] {
        assert!(!redact_text(text).contains("hidden-fixture"));
    }
    let jwt = [
        ["eyJ", "hbGciOiJIUzI1NiJ9"].concat(),
        ["cGF5", "bG9hZA"].concat(),
        ["c2ln", "bmF0dXJl"].concat(),
    ]
    .join(".");
    let jwt_message = format!("gateway rejected {jwt}");
    let jwt_redacted = redact_text(&jwt_message);
    assert!(jwt_redacted.contains("gateway rejected"));
    assert!(!jwt_redacted.contains(&jwt));
    assert_eq!(jwt_redacted, "gateway rejected [REDACTED_SECRET]");
}

#[test]
fn basic_credentials_are_hidden_in_unrelated_response_fields_and_invalid_base64_is_safe() {
    let username = ["basic", "fixture", "user"].join("-");
    let password = ["basic", "fixture", "password:tail"].join("-");
    let basic_request = request()
        .basic_auth(&username, Some(&password))
        .build()
        .unwrap();
    let redactor = Redactor::new(&basic_request);
    let echo = serde_json::json!({"errors": [username, password], "code": 400});
    let output = redactor.body(echo.to_string().as_bytes(), "application/json", false);
    let output: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(output["errors"], serde_json::json!([HIDDEN, HIDDEN]));
    assert_eq!(output["code"], 400);
    for credential in [&username, &password] {
        assert_eq!(redactor.text(credential.as_str()), HIDDEN);
    }
    for encoded in ["not-valid-base64!", "/w=="] {
        let request = request()
            .header("Authorization", format!("Basic {encoded}"))
            .build()
            .unwrap();
        let redactor = Redactor::new(&request);
        assert_eq!(redactor.text(encoded), HIDDEN);
    }
}

#[test]
fn explicit_oauth_aliases_share_sensitive_json_form_and_query_rules() {
    for key in ["auth_code", "oauth_code", "oauth_verifier"] {
        assert!(sensitive(key));
        let credential = ["alias", "fixture", "value"].join("-");
        let payload = serde_json::json!({key: credential});
        let requests = [
            request().json(&payload).build().unwrap(),
            request().form(&[(key, &credential)]).build().unwrap(),
            request().query(&[(key, &credential)]).build().unwrap(),
        ];
        for request in requests {
            let redactor = Redactor::new(&request);
            assert_eq!(redactor.text(&credential), HIDDEN);
            let result = redactor.body(payload.to_string().as_bytes(), "application/json", false);
            let result: Value = serde_json::from_str(&result).unwrap();
            assert_eq!(result[key], HIDDEN);
        }
        assert_eq!(redact_text(&format!("{key}={credential}")), HIDDEN);
    }
    assert!(!sensitive("code"));
}

#[test]
fn final_plain_text_preview_is_byte_bounded_after_expansion_and_truncation_marker() {
    let request = request().bearer_auth("q").build().unwrap();
    let redactor = Redactor::new(&request);
    let text = "q".repeat(MAX_BODY_BYTES / 2);
    for truncated in [false, true] {
        let result = redactor.body(text.as_bytes(), "text/plain", truncated);
        assert!(result.contains("64KiB"));
        assert!(result.len() <= MAX_BODY_BYTES);
    }
    let text = "中".repeat(MAX_BODY_BYTES / 3);
    assert_eq!(redactor.body(text.as_bytes(), "text/plain", false), text);
    let result = redactor.body(text.as_bytes(), "text/plain", true);
    assert!(result.contains("64KiB"));
    assert!(result.len() <= MAX_BODY_BYTES);
}

#[test]
fn partial_json_and_credentials_at_preview_boundaries_are_not_printed() {
    let request = request()
        .bearer_auth("abcdef-secret-fixture")
        .build()
        .unwrap();
    let redactor = Redactor::new(&request);
    for bytes in [b"{\"secret\":\"abc".as_slice(), b"{\"secret".as_slice()] {
        let result = redactor.body(bytes, "application/json", true);
        assert!(!result.contains("abc"));
        assert!(result.contains("已省略"));
    }
    let result = redactor.body(b"Rejected abcdef-sec", "text/plain", true);
    assert!(result.contains("Rejected"));
    assert!(!result.contains("abcdef-sec"));
    assert!(
        redactor
            .body(b"partial", "text/plain", true)
            .contains("partial")
    );
}

#[test]
fn multipart_and_binary_request_bodies_are_omitted_but_buffered_secret_echo_is_hidden() {
    let bytes = "--fixture-boundary\r\nContent-Disposition: form-data; name=\"client_secret\"\r\n\r\nmultipart-fixture-secret\r\n--fixture-boundary--\r\n";
    let request = request()
        .header(
            "Content-Type",
            "multipart/form-data; boundary=fixture-boundary",
        )
        .body(bytes)
        .build()
        .unwrap();
    let redactor = Redactor::new(&request);
    assert!(redactor.request_body(&request).contains("multipart"));
    assert!(
        !redactor
            .text("echo multipart-fixture-secret")
            .contains("multipart-fixture-secret")
    );
    for kind in ["application/zip", "image/png", "application/octet-stream"] {
        assert!(
            !redactor
                .body(b"binary-fixture", kind, false)
                .contains("binary-fixture")
        );
    }
}

#[test]
fn stream_is_never_polled_and_large_or_invalid_bodies_have_explicit_markers() {
    let body = reqwest::Body::wrap_stream(futures_util::stream::poll_fn(
        |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
            panic!("diagnostics must not poll upload streams")
        },
    ));
    let request = request().body(body).build().unwrap();
    let redactor = Redactor::new(&request);
    assert!(redactor.request_body(&request).contains("流式"));
    assert!(
        redactor
            .body(&vec![b'x'; MAX_BODY_BYTES + 1], "text/plain", false)
            .contains("64KiB")
    );
    assert!(
        redactor
            .body(&[0xff, 0xfe], "text/plain", false)
            .contains("已省略")
    );
    assert!(
        redactor
            .body(b"binary\0fixture", "", false)
            .contains("二进制")
    );
    assert_eq!(redactor.body(b"", "", false), "[无正文]");
}
