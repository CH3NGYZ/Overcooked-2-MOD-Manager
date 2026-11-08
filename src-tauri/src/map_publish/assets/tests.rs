use super::*;

#[test]
fn signed_upload_uses_put_and_never_inherits_oauth_credentials() {
    let client = upload_client().unwrap();
    let mut upload = SignedUpload {
        url: "https://bucket.example.invalid/file?signature=fixture".into(),
        headers: [
            ("x-obs-acl".into(), "private".into()),
            ("Content-Type".into(), "application/zip".into()),
        ]
        .into(),
    };
    let request = signed_upload_request(&client, &upload)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(request.method(), reqwest::Method::PUT);
    assert!(!request.headers().contains_key("authorization"));
    assert!(!request.headers().contains_key("cookie"));
    assert_eq!(request.headers()["x-obs-acl"], "private");
    upload
        .headers
        .insert("Authorization".into(), "Bearer fixture".into());
    assert!(signed_upload_request(&client, &upload).is_err());
    upload.headers.clear();
    for url in [
        "http://example.invalid/file",
        "https://user:pass@example.invalid/file",
        "file:///tmp/file",
    ] {
        upload.url = url.into();
        assert!(signed_upload_request(&client, &upload).is_err());
    }
}
