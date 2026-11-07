use super::super::block::{parse_release_note, strip_note_block};
use super::super::test_support::body;
use super::*;
use crate::models::{AssetVersionEntry, RepositoryRelation};

#[test]
fn published_version_relations_round_trip_without_changing_asset_order() {
    let text = body(
        r#"level_set_name = "Map"
author = "Author"
asset_versions = [{ mapset_version = "0.9", info_version = "0.0.9" }, { mapset_version = "0.1", info_version = "0.0.10" }]
requires = { "global" = ">=1" }
[[release_relations]]
version = ">=0.9"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0", "oc2.diylevel.fastinit" = ">=1.7.0", "oc2.oc2diylevelruntimewloader" = ">=3.10.2" }
compatible = { "compat" = { version = "*", prerelease = false } }
[[release_relations]]
version = "==0.1"
requires = { "only-this-version" = { version = "*", optional = true } }
"#,
        "release note",
    );
    let parsed = parse_release_note(&text).unwrap().unwrap();
    let metadata = publish_metadata_from_note(&parsed.metadata);
    let rendered = render_release_body(&metadata, &parsed.body);
    assert!(
        rendered.find("asset_versions").unwrap() < rendered.find("[[release_relations]]").unwrap()
    );
    assert!(rendered.contains("requires = {"));
    let back = parse_release_note(&rendered).unwrap().unwrap();
    let overrides = back.metadata.release_relation_overrides();
    assert_eq!(overrides, metadata.release_relations);
    assert_eq!(
        back.metadata.derived_mapset_version().as_deref(),
        Some("0.1")
    );
    assert_eq!(
        back.metadata.derived_info_version().as_deref(),
        Some("0.0.10")
    );
    assert_eq!(back.metadata.relations()[0].guid, "global");
    assert_eq!(back.body, "release note");
    assert_eq!(
        crate::mods::select_release_relations(&overrides, Some("0.9"))
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        crate::mods::select_release_relations(&overrides, Some("0.1")).unwrap()[0].guid,
        "only-this-version"
    );
    assert!(crate::mods::select_release_relations(&overrides, Some("0.0.9")).is_none());
    let mut duplicate = metadata;
    let relation = duplicate.release_relations[0]
        .relations
        .last()
        .unwrap()
        .clone();
    duplicate.release_relations[0]
        .relations
        .push(RepositoryRelation {
            prerelease: Some(true),
            ..relation
        });
    let rendered = render_metadata_block(&duplicate);
    assert!(rendered.contains("relations = ["));
    let back = parse_release_note(&rendered).unwrap().unwrap();
    assert_eq!(
        back.metadata.release_relation_overrides(),
        duplicate.release_relations
    );
}

#[test]
fn replacing_the_body_keeps_the_existing_block() {
    let metadata = MapPublishMetadata {
        level_set_name: "Gaoda".to_string(),
        ..MapPublishMetadata::default()
    };
    let remote = render_release_body(&metadata, "旧说明");
    let updated = replace_body_keeping_block(&remote, "新说明");
    let parsed = parse_release_note(&updated).unwrap().expect("block kept");
    assert_eq!(parsed.metadata.level_set_name, "Gaoda");
    assert_eq!(parsed.body, "新说明");
    assert!(!updated.contains("旧说明"));
    // 清空说明也不能把块带走
    let emptied = replace_body_keeping_block(&remote, "  ");
    assert!(parse_release_note(&emptied).unwrap().is_some());
    assert_eq!(strip_note_block(&emptied), "");
}

#[test]
fn replacing_the_body_without_a_block_is_just_the_new_text() {
    assert_eq!(replace_body_keeping_block("old", " new "), "new");
}

#[test]
fn rendered_metadata_round_trips_through_the_parser() {
    let metadata = MapPublishMetadata {
        level_set_name: "GAODA".into(),
        level_set_name_zh: "胡闹达学".into(),
        author: "GAODA".into(),
        name_i18n: [("zh-CN".to_string(), "胡闹达学".to_string())]
            .into_iter()
            .collect(),
        group_i18n: [("zh-CN".to_string(), "GAODA".to_string())]
            .into_iter()
            .collect(),
        note_i18n: [("en".to_string(), "Two levels.".to_string())]
            .into_iter()
            .collect(),
        asset: "daxue_v${version}.zip".into(),
        latest_mapset_version: "0.3.2".into(),
        latest_info_version: "0.3.2".into(),
        asset_versions: vec![
            AssetVersionEntry {
                mapset_version: "3.4.1".to_string(),
                info_version: "3.4.1".to_string(),
            },
            AssetVersionEntry {
                mapset_version: "3.4.1-fix".to_string(),
                info_version: "3.4.1".to_string(),
            },
        ],
        requires: [(
            "dev.gua.overcooked.diylevel".to_string(),
            ">=0.10.0".to_string(),
        )]
        .into_iter()
        .collect(),
        compatible: Default::default(),
        release_relations: Vec::new(),
        conflicts: [(
            "oc2.oc2diylevelruntimewloader".to_string(),
            ">=4.0.0".to_string(),
        )]
        .into_iter()
        .collect(),
    };
    let body = render_release_body(&metadata, "## 更新\n\n修了贴图。");
    let parsed = parse_release_note(&body).unwrap().unwrap();
    assert_eq!(parsed.metadata.level_set_name, "GAODA");
    assert_eq!(parsed.metadata.level_set_name_zh, "胡闹达学");
    assert_eq!(
        parsed.metadata.asset.as_deref(),
        Some("daxue_v${version}.zip")
    );
    // 版本表顺序即语义：渲染一次再读回来必须原样，末项才是最新
    assert_eq!(parsed.metadata.asset_versions.len(), 2);
    assert_eq!(parsed.metadata.asset_versions[0].mapset_version, "3.4.1");
    assert_eq!(
        parsed.metadata.asset_versions[1].mapset_version,
        "3.4.1-fix"
    );
    assert_eq!(parsed.metadata.asset_versions[1].info_version, "3.4.1");
    assert_eq!(
        parsed.metadata.derived_mapset_version().as_deref(),
        Some("3.4.1-fix")
    );
    assert_eq!(
        parsed.metadata.derived_info_version().as_deref(),
        Some("3.4.1")
    );
    // 新块不再写 latest_* 标量：最新版只来自表尾，写了反而会与表打架
    assert!(!body.contains("latest_mapset_version"), "{body}");
    assert!(!body.contains("latest_info_version"), "{body}");
    assert_eq!(parsed.metadata.latest_mapset_version, None);
    assert_eq!(
        parsed.metadata.name_i18n.get("zh-CN").map(String::as_str),
        Some("胡闹达学")
    );
    assert_eq!(
        parsed.metadata.note_i18n.get("en").map(String::as_str),
        Some("Two levels.")
    );
    // 关系表回读成同一份 GUID → 规范化区间
    let relations = parsed.metadata.relations();
    assert_eq!(relations.len(), 2);
    // 顺序固定为 requires → compatible → conflicts
    assert_eq!(relations[0].guid, "dev.gua.overcooked.diylevel");
    assert_eq!(relations[0].version, ">=0.10.0");
    assert_eq!(relations[1].guid, "oc2.oc2diylevelruntimewloader");
    assert_eq!(relations[1].version, ">=4.0.0");
    // 块外正文原样保留
    assert_eq!(parsed.body, "## 更新\n\n修了贴图。");
    // 再折回上传页形态，字段不丢
    let back = publish_metadata_from_note(&parsed.metadata);
    assert_eq!(back.asset, "daxue_v${version}.zip");
    assert_eq!(back.asset_versions.len(), 2);
    assert_eq!(back.latest_mapset_version, "3.4.1-fix");
    assert_eq!(
        back.conflicts
            .get("oc2.oc2diylevelruntimewloader")
            .map(String::as_str),
        Some(">=4.0.0")
    );
}

#[test]
fn a_rendered_block_without_optional_fields_stays_parseable() {
    let metadata = MapPublishMetadata {
        level_set_name: "GAODA".into(),
        author: "GAODA".into(),
        ..Default::default()
    };
    let body = render_release_body(&metadata, "");
    let parsed = parse_release_note(&body).unwrap().unwrap();
    assert_eq!(parsed.metadata.level_set_name, "GAODA");
    assert!(parsed.metadata.name_i18n.is_empty());
    assert!(parsed.metadata.requires.is_empty());
    assert_eq!(parsed.body, "");
}

#[test]
fn a_rendered_table_comes_after_every_scalar_key() {
    let metadata = MapPublishMetadata {
        level_set_name: "GAODA".into(),
        author: "GAODA".into(),
        asset: "gaoda_v${version}.zip".into(),
        latest_mapset_version: "3.4.1-fix".into(),
        requires: [(
            "dev.gua.overcooked.diylevel".to_string(),
            ">=0.10.0".to_string(),
        )]
        .into_iter()
        .collect(),
        asset_versions: vec![
            AssetVersionEntry {
                mapset_version: "0.7".to_string(),
                info_version: "0.7".to_string(),
            },
            AssetVersionEntry {
                mapset_version: "3.4.1-fix".to_string(),
                info_version: "3.4.1".to_string(),
            },
        ],
        ..MapPublishMetadata::default()
    };
    let block = render_metadata_block(&metadata);
    // 键会按最长键对齐补空格，按行取键名定位，不匹配字面的 `asset = `。
    let line_of = |wanted: &str| {
        block
            .lines()
            .position(|line| line.split_once('=').map(|(key, _)| key.trim()) == Some(wanted))
    };
    let table_at = line_of("asset_versions").expect("版本表必须被渲染出来");
    let scalar_at = line_of("asset").expect("标量键照旧");
    assert!(scalar_at < table_at, "{block}");
    // latest_* 由表尾派生，不再落盘
    assert!(!block.contains("latest_mapset_version"), "{block}");
    // 表里的非数字版本号原样落盘：`3.4.1-fix` 不能被当成非法版本丢掉
    assert!(block.contains(r#""3.4.1-fix""#), "{block}");
    let parsed = parse_release_note(&body(&block_body(&block), "")).unwrap();
    let parsed = parsed.unwrap();
    assert_eq!(parsed.metadata.asset_versions.len(), 2);
    assert_eq!(
        parsed.metadata.asset_versions[0].mapset_version,
        "0.7".to_string()
    );
    assert_eq!(
        parsed.metadata.derived_mapset_version().as_deref(),
        Some("3.4.1-fix")
    );
}

/// 去掉块的起止标记，只留 TOML 正文（`body()` 会自己再套一层）。
fn block_body(block: &str) -> String {
    block
        .trim()
        .trim_start_matches(NOTE_BLOCK_OPEN)
        .trim()
        .trim_end_matches(NOTE_BLOCK_CLOSE)
        .trim()
        .to_string()
}
