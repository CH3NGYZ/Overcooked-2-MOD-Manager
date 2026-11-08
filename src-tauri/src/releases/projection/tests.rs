use super::*;

#[test]
fn versions_and_channels_are_supported() {
    assert_eq!(
        parse_version("v1.2.3-beta").unwrap().to_string(),
        "1.2.3-beta"
    );
    assert!(parse_version("5.4.23.5").is_some());
    let releases = vec![
        ReleaseInfo {
            release_id: None,
            tag_name: "v1.0.0".into(),
            version: Some("1.0.0".into()),
            name: String::new(),
            body: String::new(),
            published_at: None,
            prerelease: false,
            draft: false,
            assets: Vec::new(),
        },
        ReleaseInfo {
            release_id: None,
            tag_name: "v1.1.0-beta".into(),
            version: Some("1.1.0-beta".into()),
            name: String::new(),
            body: String::new(),
            published_at: None,
            prerelease: true,
            draft: false,
            assets: Vec::new(),
        },
    ];
    assert_eq!(filter_channel(releases.clone(), &Channel::Stable).len(), 1);
    assert_eq!(filter_channel(releases, &Channel::Beta).len(), 1);
}

#[test]
fn release_id_and_missing_asset_size_are_parsed() {
    let value = serde_json::json!([{
        "id": "42",
        "tag_name": "v1.0.0",
        "assets": [{
            "name": "HostUtilities.zip",
            "browser_download_url": "https://gitee.com/example/download"
        }]
    }]);
    let releases = parse_release_json(&value, &Provider::Gitee).unwrap();
    assert_eq!(releases[0].release_id, Some(42));
    assert_eq!(releases[0].assets[0].size, 0);
    assert_eq!(releases[0].assets[0].download_count, None);
}

#[test]
fn release_asset_download_counts_preserve_zero_and_missing_metadata() {
    for (input, expected) in [
        (Some(serde_json::json!(42)), Some(42)),
        (Some(serde_json::json!(0)), Some(0)),
        (Some(serde_json::json!("1234")), Some(1234)),
        (None, None),
        (Some(serde_json::Value::Null), None),
        (Some(serde_json::json!(-1)), None),
        (Some(serde_json::json!(1.5)), None),
        (Some(serde_json::json!("unknown")), None),
    ] {
        let mut asset = serde_json::json!({
            "name": "example.zip", "size": 1024,
            "browser_download_url": "https://github.com/example/mod/releases/download/v1/example.zip"
        });
        if let Some(input) = input {
            asset["download_count"] = input;
        }
        let releases = parse_release_json(
            &serde_json::json!([{ "tag_name": "v1.0.0", "assets": [asset] }]),
            &Provider::Github,
        )
        .unwrap();
        let asset = &releases[0].assets[0];
        assert_eq!(asset.download_count, expected);
        let cached: ReleaseAsset =
            serde_json::from_value(serde_json::to_value(asset).unwrap()).unwrap();
        assert_eq!(cached.download_count, expected);
        assert_eq!(cached.name, "example.zip");
        assert_eq!(cached.size, 1024);
    }

    let legacy: ReleaseAsset = serde_json::from_value(serde_json::json!({
        "name": "legacy.zip", "size": 1024, "download_url": "https://example.com/legacy.zip"
    }))
    .unwrap();
    assert_eq!(legacy.download_count, None);
}

#[test]
fn gitcode_filters_only_generated_archives_and_keeps_optional_metadata() {
    let input = serde_json::json!([{"tag_name":"map", "assets":[
        {"name":"map.zip", "type":"source", "browser_download_url":"https://raw.atomgit.com/a/b/archive/refs/heads/map.zip"},
        {"name":"fixture_map.tar.gz", "browser_download_url":"https://raw.atomgit.com/a/b/archive/refs/tags/map.tar.gz"},
        {"name":"archive-map.zip", "type":"attach", "size":123, "digest":"sha256:abc", "browser_download_url":"https://atomgit.com/a/b/releases/download/map/archive-map.zip"},
        {"name":"unknown.zip", "type":"attach", "browser_download_url":"https://atomgit.com/a/b/releases/download/map/unknown.zip"}
    ]}]);
    let result = parse_release_json(&input, &Provider::Gitcode).unwrap();
    assert_eq!(result[0].assets.len(), 2);
    assert_eq!(result[0].assets[0].name, "archive-map.zip");
    assert_eq!(result[0].assets[0].size, 123);
    assert_eq!(result[0].assets[0].digest.as_deref(), Some("sha256:abc"));
    assert_eq!(result[0].assets[1].size, 0);
    assert!(result[0].assets[1].digest.is_none());
}
