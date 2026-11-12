use super::*;
use crate::map_note::NOTE_BLOCK_OPEN;
use crate::models::AssetVersionEntry;

#[test]
fn gitcode_source_archives_without_ids_do_not_break_release_refresh() {
    // Shape captured from GitCode's real public Release API: generated source
    // archives have no id/size; only uploaded attachments have deletion IDs.
    let remote: ApiRelease = serde_json::from_value(json!({
        "tag_name": "blni", "name": "blni", "prerelease": false,
        "created_at": "2026-11-08T14:37:28+08:00",
        "assets": [
            {"name": "blni.zip", "type": "source"},
            {"id": 303782, "name": "blni_v2.1.zip", "type": "attach", "size": 2362693}
        ]
    }))
    .unwrap();
    let release = release_from_api(remote, PublishProvider::Gitcode);
    assert_eq!(release.id, "blni");
    assert_eq!(release.assets.len(), 1);
    assert_eq!(release.assets[0].id, "303782");
    assert_eq!(release.assets[0].size, 2362693);
    // Missing IDs on real attachments must still fail; otherwise cleanup would
    // act on a fabricated or empty server identifier.
    assert!(
        serde_json::from_value::<ApiRelease>(json!({
            "tag_name": "blni", "assets": [{"name": "broken.zip", "type": "attach"}]
        }))
        .is_err()
    );
}

#[test]
fn publishing_rejects_invalid_relation_overrides_before_writing() {
    let mut metadata: MapPublishMetadata = serde_json::from_value(serde_json::json!({
        "release_relations": [{ "version": ">=0.9", "relations": [
            { "kind": "requires", "guid": "dep", "version": ">=1.7" }
        ] }]
    }))
    .unwrap();
    assert!(validate_publish_relations(&metadata).is_ok());
    for version in ["", "*", "~=1", ">=2,<1"] {
        metadata.release_relations[0].version = version.to_string();
        assert_eq!(
            validate_publish_relations(&metadata).unwrap_err(),
            "mapPublish.invalidRelations"
        );
    }
    metadata.release_relations[0].version = "==0.1".to_string();
    metadata.release_relations[0].relations[0].version = ">=2,<1".to_string();
    assert!(validate_publish_relations(&metadata).is_err());
    metadata.release_relations[0].relations.clear();
    assert!(validate_publish_relations(&metadata).is_err());
    metadata.release_relations.clear();
    metadata.requires.insert("dep".to_string(), "".to_string());
    assert!(validate_publish_relations(&metadata).is_ok());
}

#[test]
fn release_metadata_survives_the_round_trip_through_the_block() {
    let metadata = MapPublishMetadata {
        level_set_name: "Gaoda".to_string(),
        level_set_name_zh: "高達".to_string(),
        author: "GAODA".to_string(),
        asset: "gaoda-${version}.zip".to_string(),
        requires: [("dev.gua.diylevel".to_string(), ">=4.0.0".to_string())]
            .into_iter()
            .collect(),
        ..MapPublishMetadata::default()
    };
    let body = render_release_body(&metadata, "首个版本");
    let block = crate::map_note::parse_release_note(&body)
        .unwrap()
        .expect("rendered body must contain a block");
    let parsed = publish_metadata_from_note(&block.metadata);
    assert_eq!(parsed.level_set_name, "Gaoda");
    assert_eq!(parsed.level_set_name_zh, "高達");
    assert_eq!(parsed.asset, "gaoda-${version}.zip");
    assert_eq!(
        parsed.requires.get("dev.gua.diylevel").map(String::as_str),
        Some(">=4.0.0")
    );
    assert_eq!(block.body.trim(), "首个版本");
    assert!(body.starts_with(NOTE_BLOCK_OPEN));
}

#[test]
fn the_table_tail_overrides_the_stale_latest_versions() {
    let mut metadata = MapPublishMetadata {
        latest_mapset_version: "0.7".to_string(),
        latest_info_version: "0.7".to_string(),
        asset_versions: vec![
            AssetVersionEntry {
                mapset_version: "0.7".to_string(),
                info_version: "0.7".to_string(),
            },
            AssetVersionEntry {
                mapset_version: " 0.69 ".to_string(),
                info_version: "0.69".to_string(),
            },
        ],
        ..MapPublishMetadata::default()
    };
    apply_table_tail(&mut metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
    assert_eq!(metadata.latest_info_version, "0.69");
    // 表尾那一格的版本号空着时不能把已有值抹成空
    metadata.asset_versions.push(AssetVersionEntry::default());
    apply_table_tail(&mut metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
    // 空表完全不动：老源的 latest_* 仍然只有一个来源
    metadata.asset_versions.clear();
    apply_table_tail(&mut metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
}

#[test]
fn gitcode_releases_keep_tag_identity_and_only_attachment_assets() {
    let remote: ApiRelease =
        serde_json::from_value(json!({"tag_name":"map/测试", "name":"Map", "assets":[
        {"id":"opaque-12", "name":"map.zip", "type":"attach"},
        {"id":99, "name":"archive.zip", "type":"source"}]}))
        .unwrap();
    let projected = release_from_api(remote, PublishProvider::Gitcode);
    assert_eq!(projected.id, "map/测试");
    assert_eq!(projected.assets.len(), 1);
    assert_eq!(projected.assets[0].id, "opaque-12");
    let github: ApiRelease = serde_json::from_value(
        json!({"id":123,"tag_name":"tag","assets":[{"id":42,"name":"asset.zip"}]}),
    )
    .unwrap();
    let github = release_from_api(github, PublishProvider::Github);
    assert_eq!(github.id, "123");
    assert_eq!(github.assets[0].id, "42");
    assert!(
        release_payload(PublishProvider::Gitcode, "map", "body")
            .get("draft")
            .is_none()
    );
    assert!(
        release_payload(PublishProvider::Github, "map", "body")
            .get("release_status")
            .is_none()
    );
}

fn metadata_with_pending_version(existing_release: bool) -> MapPublishMetadata {
    let mut metadata = MapPublishMetadata {
        level_set_name: "Gaoda".to_string(),
        level_set_name_zh: "高达地图集".to_string(),
        author: "Map Author".to_string(),
        asset: "Gaoda_v${version}.zip".to_string(),
        name_i18n: [("en".to_string(), "Gaoda Maps".to_string())]
            .into_iter()
            .collect(),
        requires: [("dev.gua.diylevel".to_string(), ">=4.0.0".to_string())]
            .into_iter()
            .collect(),
        ..MapPublishMetadata::default()
    };
    if existing_release {
        metadata.asset_versions.push(AssetVersionEntry {
            mapset_version: "1.0".to_string(),
            info_version: "1.0".to_string(),
        });
    }
    metadata.asset_versions.push(AssetVersionEntry {
        mapset_version: "2.0".to_string(),
        info_version: "1.9".to_string(),
    });
    apply_table_tail(&mut metadata);
    metadata
}

#[test]
fn publish_payload_contains_pending_version_and_changelog_before_asset_upload() {
    let changelog = "## 2.0 更新\n\n- 新增关卡。\n- 保留 **Markdown** 和 [说明](https://example.invalid/maps)。";
    for existing_release in [false, true] {
        let metadata = metadata_with_pending_version(existing_release);
        for provider in [PublishProvider::Github, PublishProvider::Gitcode] {
            let rendered = render_release_body(&metadata, changelog);
            let payload = release_payload(provider, "Gaoda", &rendered);
            let body = payload["body"].as_str().expect("Release body is text");
            assert_eq!(body, rendered);
            assert_eq!(body.matches(NOTE_BLOCK_OPEN).count(), 1);
            assert_eq!(body.matches(crate::map_note::NOTE_BLOCK_CLOSE).count(), 1);
            assert_eq!(body.matches(changelog).count(), 1);
            assert_eq!(payload["tag_name"], "Gaoda");
            assert_eq!(payload["name"], "Gaoda");

            let note = crate::map_note::parse_release_note(body)
                .unwrap()
                .expect("body already contains the map declaration");
            let parsed = publish_metadata_from_note(&note.metadata);
            assert_eq!(note.body, changelog);
            assert_eq!(parsed.level_set_name, metadata.level_set_name);
            assert_eq!(parsed.level_set_name_zh, metadata.level_set_name_zh);
            assert_eq!(parsed.author, metadata.author);
            assert_eq!(parsed.asset, metadata.asset);
            assert_eq!(parsed.name_i18n, metadata.name_i18n);
            assert_eq!(parsed.requires, metadata.requires);
            assert_eq!(parsed.latest_mapset_version, "2.0");
            assert_eq!(parsed.latest_info_version, "1.9");
            let versions = parsed
                .asset_versions
                .iter()
                .map(|entry| (entry.mapset_version.as_str(), entry.info_version.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(
                versions,
                if existing_release {
                    vec![("1.0", "1.0"), ("2.0", "1.9")]
                } else {
                    vec![("2.0", "1.9")]
                }
            );
        }
    }
}

#[test]
fn publish_payload_with_empty_changelog_still_has_nonempty_map_body() {
    let metadata = metadata_with_pending_version(false);
    for provider in [PublishProvider::Github, PublishProvider::Gitcode] {
        for changelog in ["", " \r\n\t "] {
            let rendered = render_release_body(&metadata, changelog);
            let payload = release_payload(provider, "Gaoda", &rendered);
            let body = payload["body"].as_str().expect("Release body is text");
            assert!(!body.trim().is_empty());
            assert_eq!(body.matches(NOTE_BLOCK_OPEN).count(), 1);
            assert_eq!(body.matches(crate::map_note::NOTE_BLOCK_CLOSE).count(), 1);
            let note = crate::map_note::parse_release_note(body)
                .unwrap()
                .expect("empty changelog cannot remove the map declaration");
            assert!(note.body.is_empty());
            assert_eq!(
                note.metadata.derived_mapset_version().as_deref(),
                Some("2.0")
            );
            assert_eq!(note.metadata.derived_info_version().as_deref(), Some("1.9"));
        }
    }
}

#[test]
fn updating_a_legacy_note_canonicalizes_identity_and_preserves_its_release_data() {
    // This is a hand-written old note: the parser must retain its raw identity,
    // while the save lock accepts the canonical identity used by the renderer.
    let old_body = r#"<!-- oc2-map
level_set_name = " \tGaoda\u3000 "
level_set_name_zh = "\u3000高达地图集\t "
author = " \tMap Author\u3000"
asset = "Gaoda_v${version}.zip"
name_i18n = { en = "Gaoda Maps" }
asset_versions = [{ mapset_version = "1.0", info_version = "1.0" }]
requires = { "dev.gua.diylevel" = ">=4.0.0" }
-->
## 1.0
旧更新说明"#;
    let old_note = crate::map_note::parse_release_note(old_body)
        .unwrap()
        .unwrap();
    assert_eq!(old_note.metadata.level_set_name, " \tGaoda\u{3000} ");
    assert_eq!(old_note.metadata.level_set_name_zh, "\u{3000}高达地图集\t ");
    assert_eq!(old_note.metadata.author, " \tMap Author\u{3000}");

    let changelog = "## 2.0\n新版本保留 **Markdown**。";
    for provider in [PublishProvider::Github, PublishProvider::Gitcode] {
        let old: ApiRelease = serde_json::from_value(json!({
            "id": 42, "tag_name": "Gaoda", "body": old_body, "assets": []
        }))
        .unwrap();
        let projected = release_from_api(old, provider);
        let mut metadata = projected.metadata.unwrap();
        metadata.asset_versions.push(AssetVersionEntry {
            mapset_version: "2.0".into(),
            info_version: "1.9".into(),
        });
        apply_table_tail(&mut metadata);
        let body = render_release_body(&metadata, changelog);
        let payload = release_payload(provider, "Gaoda", &body);
        let updated = crate::map_note::parse_release_note(payload["body"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(updated.metadata.level_set_name, "Gaoda");
        assert_eq!(updated.metadata.level_set_name_zh, "高达地图集");
        assert_eq!(updated.metadata.author, "Map Author");
        assert_eq!(updated.metadata.asset, old_note.metadata.asset);
        assert_eq!(updated.metadata.name_i18n, old_note.metadata.name_i18n);
        assert_eq!(updated.metadata.asset_versions, metadata.asset_versions);
        assert_eq!(
            updated.metadata.derived_mapset_version().as_deref(),
            Some("2.0")
        );
        assert_eq!(
            updated.metadata.derived_info_version().as_deref(),
            Some("1.9")
        );
        assert_eq!(updated.body, changelog);
        assert_eq!(body.matches(NOTE_BLOCK_OPEN).count(), 1);
    }
}

#[test]
fn save_identity_lock_compares_all_three_trimmed_fields_strictly_before_rendering() {
    // The command fixes its official API endpoint; keep its inline lock wired
    // without adding a second identity helper solely to make it callable here.
    let source = include_str!("../releases.rs");
    let lock = source
        .split_once("let locked = &note.metadata;")
        .unwrap()
        .1
        .split_once("apply_table_tail(&mut metadata);")
        .unwrap()
        .0;
    for field in ["level_set_name", "level_set_name_zh", "author"] {
        assert!(lock.contains(&format!("locked.{field}.trim() != metadata.{field}.trim()")));
    }
    assert!(lock.contains("return Err(\"mapPublish.zipIdentityMismatch\".to_string())"));
    assert!(!lock.contains("to_lowercase"));
    assert!(!lock.contains("eq_ignore_ascii_case"));
    assert!(!lock.contains("is_empty"));
}
