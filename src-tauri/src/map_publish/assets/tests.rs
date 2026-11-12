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

fn rendered_identity(identity: [&str; 3]) -> crate::map_note::MapNoteMetadata {
    let metadata = crate::models::MapPublishMetadata {
        level_set_name: identity[0].into(),
        level_set_name_zh: identity[1].into(),
        author: identity[2].into(),
        asset: "map_v${version}.zip".into(),
        ..Default::default()
    };
    let body = crate::map_note::render_release_body(&metadata, "## 1.0\n正式更新说明");
    let parsed = crate::map_note::parse_release_note(&body)
        .expect("rendered release body must parse")
        .expect("rendered release body must contain metadata");
    assert_eq!(parsed.body, "## 1.0\n正式更新说明");
    parsed.metadata
}

fn inspected_identity(identity: [&str; 3]) -> PublishZipInspection {
    PublishZipInspection {
        level_set_name: identity[0].into(),
        level_set_name_zh: identity[1].into(),
        author: identity[2].into(),
        set_count: 1,
        ..Default::default()
    }
}

#[test]
fn rendered_release_identity_accepts_cached_zip_edge_whitespace() {
    let raw = [" \tMap\r\n", "\u{3000}地图\t ", "\tAuthor\u{3000}"];
    let metadata = rendered_identity(raw);
    assert_eq!(metadata.level_set_name, "Map");
    assert_eq!(metadata.level_set_name_zh, "地图");
    assert_eq!(metadata.author, "Author");
    let inspection = inspected_identity(raw);
    assert_eq!(validate_zip_identity(Some(&metadata), &inspection), Ok(()));
}

#[test]
fn rendered_release_identity_rejects_real_differences_and_empty_chinese_name() {
    let canonical = ["Map", "地图", "Author"];
    for (release, zip) in [
        (canonical, ["map", "地图", "Author"]),
        (canonical, ["Map", "地图", "author"]),
        (canonical, ["OtherMap", "地图", "Author"]),
        (canonical, ["Map", "另一地图", "Author"]),
        (canonical, ["Map", "地图", "OtherAuthor"]),
        (canonical, ["Ma p", "地图", "Author"]),
        (canonical, ["Map", "地图", "Au thor"]),
        (canonical, ["Map", "", "Author"]),
        (["Map", "", "Author"], canonical),
    ] {
        let metadata = rendered_identity(release);
        let inspection = inspected_identity(zip);
        assert_eq!(
            validate_zip_identity(Some(&metadata), &inspection),
            Err("mapPublish.zipIdentityMismatch".into()),
            "release={release:?}, zip={zip:?}",
        );
    }
}

#[test]
fn rendered_release_identity_keeps_single_map_requirement_with_or_without_metadata() {
    let identity = ["Map", "地图", "Author"];
    let metadata = rendered_identity(identity);
    let mut inspection = inspected_identity(identity);
    assert_eq!(validate_zip_identity(None, &inspection), Ok(()));
    for set_count in [0, 2] {
        inspection.set_count = set_count;
        for metadata in [None, Some(&metadata)] {
            assert_eq!(
                validate_zip_identity(metadata, &inspection),
                Err(invalid_asset("ZIP must contain exactly one level set")),
                "set_count={set_count}, metadata_present={}",
                metadata.is_some(),
            );
        }
    }
}
