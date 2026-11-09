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
