use super::*;

#[test]
fn explicit_api_false_promotes_numeric_beta_and_overrides_legacy_cache() {
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        let releases = parse_release_json(
            &serde_json::json!([
                {"tag_name": "v1.9.9.2", "prerelease": false},
                {"tag_name": "v1.9.9-beta.3"},
                {"tag_name": "v1.9.9", "prerelease": true}
            ]),
            &provider,
        )
        .unwrap();
        assert_eq!(releases[0].api_prerelease, Some(false));
        assert!(!releases[0].prerelease);
        assert_eq!(releases[1].api_prerelease, None);
        assert!(releases[1].prerelease);
        assert!(releases[2].prerelease);
        let snapshot = project_mod_releases(releases, &Channel::Beta);
        assert!(snapshot.cached_releases_complete);
        assert_eq!(
            snapshot.release_prerelease_metadata.get("v1.9.9.2"),
            Some(&false)
        );
        assert!(
            !snapshot
                .cached_releases
                .iter()
                .any(|release| release.tag_name == "v1.9.9.2")
        );
    }
    let legacy: ReleaseInfo = serde_json::from_value(serde_json::json!({
        "tag_name": "v1.9.9-beta", "version": null, "name": "", "body": "",
        "published_at": null, "prerelease": false, "draft": false, "assets": []
    }))
    .unwrap();
    assert_eq!(legacy.api_prerelease, None);
    assert_eq!(filter_channel(vec![legacy], &Channel::Beta).len(), 1);
    let legacy: crate::models::ModRepositoryReleaseSnapshot =
        serde_json::from_value(serde_json::json!({
            "cached_releases":[], "release_prerelease_metadata":{}
        }))
        .unwrap();
    assert!(!legacy.cached_releases_complete);
    let staged: crate::models::StagedReleaseCache =
        serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(!staged.cached_releases_complete);
}

#[test]
fn explicit_beta_tag_keeps_its_version_and_channel_when_platform_flag_is_false() {
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        for tag in ["v2.0.0-beta.3", "2.0-beta.3"] {
            let releases = parse_release_json(
                &serde_json::json!({"tag_name": tag, "prerelease": false}),
                &provider,
            )
            .unwrap();
            assert_eq!(releases[0].version.as_deref(), Some("2.0.0-beta.3"));
            assert_eq!(releases[0].api_prerelease, Some(false));
            assert!(releases[0].prerelease);
            assert!(filter_channel(releases.clone(), &Channel::Stable).is_empty());
            let snapshot = project_mod_releases(releases, &Channel::Beta);
            assert_eq!(snapshot.cached_releases.len(), 1);
            assert_eq!(snapshot.release_prerelease_metadata.get(tag), Some(&false));
        }
    }
}

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
            api_prerelease: None,
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
            api_prerelease: None,
            prerelease: true,
            draft: false,
            assets: Vec::new(),
        },
    ];
    assert_eq!(filter_channel(releases.clone(), &Channel::Stable).len(), 1);
    assert_eq!(filter_channel(releases, &Channel::Beta).len(), 1);
}

#[test]
fn sort_uses_complete_versions_and_preserves_exact_tags_on_equal_precedence() {
    let releases = parse_release_json(
        &serde_json::json!([
            {"tag_name":"v0.99", "published_at":"2026-10-01"},
            {"tag_name":"Kitchen2", "published_at":"2026-11-01"},
            {"tag_name":"1.0.0.9", "published_at":"2026-11-01"},
            {"tag_name":"v1.0.0-beta10", "published_at":"2026-10-01"},
            {"tag_name":"1.0-beta.11", "published_at":"2026-09-01"},
            {"tag_name":"v1", "published_at":"2026-09-01"},
            {"tag_name":"1.0.0", "published_at":"2026-08-01"}
        ]),
        &Provider::Github,
    )
    .unwrap();
    assert!(releases[1].version.is_none());
    assert_eq!(releases[5].version.as_deref(), Some("1.0.0"));
    let tags = filter_channel(releases, &Channel::All)
        .into_iter()
        .map(|release| release.tag_name)
        .collect::<Vec<_>>();
    assert_eq!(
        tags,
        [
            "v1",
            "1.0.0",
            "1.0-beta.11",
            "v1.0.0-beta10",
            "1.0.0.9",
            "v0.99",
            "Kitchen2"
        ]
    );
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
    let digest = "sha256:adc004bf74c98b5dd60d3001c3316c5a732c7df5ea4daad1cbae97c22e017c22";
    let input = serde_json::json!([{"tag_name":"map", "assets":[
        {"name":"map.zip", "type":"source", "browser_download_url":"https://raw.atomgit.com/a/b/archive/refs/heads/map.zip"},
        {"name":"fixture_map.tar.gz", "browser_download_url":"https://raw.atomgit.com/a/b/archive/refs/tags/map.tar.gz"},
        {"name":"archive-map.zip", "type":"attach", "size":123, "digest":digest, "browser_download_url":"https://atomgit.com/a/b/releases/download/map/archive-map.zip"},
        {"name":"unknown.zip", "type":"attach", "browser_download_url":"https://atomgit.com/a/b/releases/download/map/unknown.zip"}
    ]}]);
    let result = parse_release_json(&input, &Provider::Gitcode).unwrap();
    assert_eq!(result[0].assets.len(), 2);
    assert_eq!(result[0].assets[0].name, "archive-map.zip");
    assert_eq!(result[0].assets[0].size, 123);
    assert_eq!(result[0].assets[0].digest.as_deref(), Some(digest));
    assert_eq!(result[0].assets[1].size, 0);
    assert!(result[0].assets[1].digest.is_none());
}

#[test]
fn asset_digest_uses_only_explicit_nonempty_api_fields() {
    let tagged = "sha256:fb34009065a5e12919e6d0b088e6bf550e6c36d30651a24b04862444b8fe6f18";
    let raw = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    for (digest, sha256, expected) in [
        (Some(tagged), Some(raw), Some(tagged)),
        (Some(" "), Some(raw), Some(raw)),
        (None, Some(raw), Some(raw)),
        (None, Some(" "), None),
        (None, None, None),
    ] {
        let release = serde_json::json!({"tag_name":"v1.2.3", "assets":[{
            "name":"map.zip", "size":123, "id":456, "etag":raw, "hash":raw,
            "digest":digest, "sha256":sha256,
            "browser_download_url":"https://github.com/u/r/releases/download/v1.2.3/map.zip"
        }]});
        let loaded = parse_release_json(&release, &Provider::Github).unwrap();
        let asset = &loaded[0].assets[0];
        assert_eq!(asset.digest.as_deref(), expected);
        let roundtrip: ReleaseAsset =
            serde_json::from_value(serde_json::to_value(asset).unwrap()).unwrap();
        assert_eq!(roundtrip.digest.as_deref(), expected);
    }
}
