use crate::level_sets::install::install_parsed_sets;
use crate::level_sets::installed_versions::installed_mapset_version;
use crate::level_sets::installed_versions::record_installed_mapset_versions;
use crate::level_sets::paths::levels_root;
use crate::level_sets::zip_inspect::ImportedLevelSet;
use crate::models::LevelSetLayout;
use std::fs;

fn imported(name: &str, author: &str) -> ImportedLevelSet {
    ImportedLevelSet {
        dir_name: name.to_string(),
        layout: LevelSetLayout::Flattened,
        metadata: crate::asset_bundle::LevelSetMetadata {
            name: name.to_string(),
            author: author.to_string(),
            ..Default::default()
        },
        files: Vec::new(),
        plugins: Vec::new(),
    }
}

fn origin(set: &ImportedLevelSet, version: &str) -> crate::models::MapInstallOrigin {
    crate::models::MapInstallOrigin {
        level_set_name: set.metadata.name.clone(),
        level_set_name_zh: set.metadata.name_zh.clone().unwrap_or_default(),
        author: set.metadata.author.clone(),
        mapset_version: version.to_string(),
    }
}

#[test]
fn bundled_map_must_not_inherit_primary_asset_version() {
    let dir = std::env::temp_dir().join(format!("oc2-mapset-bundle-{}", uuid::Uuid::new_v4()));
    let mut primary = imported("pear", "Author");
    primary.metadata.version = "10.5".into();
    let mut bundled = imported("Music", "Author");
    bundled.metadata.version = "1.1".into();
    record_installed_mapset_versions(&dir, &[&bundled, &primary], Some(&origin(&primary, "10.5")));
    let after = crate::archive::read_map_relation_snapshots(&dir);
    let bundled_version = after
        .entries
        .values()
        .find(|entry| entry.level_set_name == "Music")
        .map(|entry| entry.installed_mapset_version.as_str())
        .unwrap_or("");
    assert_eq!(bundled_version, "", "合集版本不能冒充随包地图的资产版本");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn bundled_reinstall_clears_stale_versions_but_keeps_other_maps() {
    let dir = std::env::temp_dir().join(format!("oc2-mapset-stale-{}", uuid::Uuid::new_v4()));
    let primary = imported("pear", "Author");
    let bundled = imported("Music", "Author");
    let untouched = imported("Other", "Author");
    record_installed_mapset_versions(&dir, &[&bundled], Some(&origin(&bundled, "10.5")));
    record_installed_mapset_versions(&dir, &[&untouched], Some(&origin(&untouched, "2-fix")));
    record_installed_mapset_versions(&dir, &[&bundled, &primary], Some(&origin(&primary, "11.0")));
    let after = crate::archive::read_map_relation_snapshots(&dir);
    for (name, expected) in [("pear", "11.0"), ("Music", ""), ("Other", "2-fix")] {
        assert_eq!(
            after
                .entries
                .values()
                .find(|entry| entry.level_set_name == name)
                .unwrap()
                .installed_mapset_version,
            expected
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn chinese_identity_separates_same_name_and_author() {
    let dir = std::env::temp_dir().join(format!("oc2-mapset-identity-{}", uuid::Uuid::new_v4()));
    let mut first = imported("GAODA", "GAODA");
    first.metadata.name_zh = Some("胡闹糕中".into());
    let mut second = imported("GAODA", "GAODA");
    second.metadata.name_zh = Some("胡闹达学".into());
    record_installed_mapset_versions(&dir, &[&first], Some(&origin(&first, "1-fix")));
    record_installed_mapset_versions(&dir, &[&second], Some(&origin(&second, "2-fix")));
    let after = crate::archive::read_map_relation_snapshots(&dir);
    assert_eq!(after.entries.len(), 2);
    assert_eq!(
        after
            .entries
            .values()
            .find(|entry| entry.level_set_name_zh == "胡闹糕中")
            .unwrap()
            .installed_mapset_version,
        "1-fix"
    );
    assert_eq!(
        after
            .entries
            .values()
            .find(|entry| entry.level_set_name_zh == "胡闹达学")
            .unwrap()
            .installed_mapset_version,
        "2-fix"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn exact_identity_wins_over_legacy_id_and_local_reinstall_clears_it() {
    let dir = std::env::temp_dir().join(format!("oc2-mapset-legacy-{}", uuid::Uuid::new_v4()));
    let mut primary = imported("pear", "Author");
    record_installed_mapset_versions(&dir, &[&primary], Some(&origin(&primary, "10.5")));
    let mut snapshots = crate::archive::read_map_relation_snapshots(&dir);
    let old = snapshots.entries.values().next().unwrap().clone();
    snapshots.entries.clear();
    snapshots.entries.insert("legacy-map-id".into(), old);
    crate::archive::write_map_relation_snapshots(&dir, &snapshots).unwrap();
    primary.metadata.name_zh = Some("梨之盛宴".into());
    record_installed_mapset_versions(&dir, &[&primary], Some(&origin(&primary, "11.0-fix")));
    let after = crate::archive::read_map_relation_snapshots(&dir);
    assert!(
        after.entries["legacy-map-id"]
            .installed_mapset_version
            .is_empty()
    );
    assert_eq!(
        installed_mapset_version(
            &after,
            Some("legacy-map-id"),
            "pear",
            Some("梨之盛宴"),
            "Author"
        ),
        "11.0-fix"
    );
    record_installed_mapset_versions(&dir, &[&primary], None);
    let after = crate::archive::read_map_relation_snapshots(&dir);
    assert_eq!(
        installed_mapset_version(
            &after,
            Some("legacy-map-id"),
            "pear",
            Some("梨之盛宴"),
            "Author"
        ),
        ""
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn origin_from_parsed_info_does_not_treat_empty_chinese_name_as_wildcard() {
    let dir = std::env::temp_dir().join(format!("oc2-mapset-origin-{}", uuid::Uuid::new_v4()));
    let primary = imported("Set", "Author");
    let mut bundled = imported("Set", "Author");
    bundled.metadata.name_zh = Some("Another map".into());
    record_installed_mapset_versions(
        &dir,
        &[&bundled, &primary],
        Some(&origin(&primary, "2-fix")),
    );
    let after = crate::archive::read_map_relation_snapshots(&dir);
    assert_eq!(after.entries.len(), 1);
    assert!(
        after
            .entries
            .values()
            .next()
            .unwrap()
            .level_set_name_zh
            .is_empty()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn both_install_layouts_scope_asset_versions_even_with_single_selection() {
    use std::io::Write;
    for layout in [LevelSetLayout::Flattened, LevelSetLayout::Bepinex] {
        let root =
            std::env::temp_dir().join(format!("oc2-mapset-install-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&game).unwrap();
        let zip_path = root.join("bundle.zip");
        let mut bundled = imported("Music", "Author");
        let mut primary = imported("pear", "Author");
        bundled.layout = layout;
        primary.layout = layout;
        let source = origin(&primary, "11.0-fix");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        for set in [&mut bundled, &mut primary] {
            let entry = match layout {
                LevelSetLayout::Flattened => format!("{}/info", set.dir_name),
                LevelSetLayout::Bepinex => {
                    format!("BepInEx/plugins/OC2DIYLevel/levels/{}/info", set.dir_name)
                }
            };
            writer
                .start_file(&entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"fixture").unwrap();
            set.files.push((entry, "info".into()));
        }
        writer.finish().unwrap();
        let sets = vec![bundled, primary];
        // 先只安装随包地图，不能因为本轮只有一张就把主包版本塞给它。
        install_parsed_sets(
            &sets,
            &zip_path,
            &game,
            &manifests,
            &["Music".into()],
            Some(&source),
        )
        .unwrap();
        assert!(
            crate::archive::read_map_relation_snapshots(&manifests)
                .entries
                .is_empty()
        );
        install_parsed_sets(
            &sets,
            &zip_path,
            &game,
            &manifests,
            &["pear".into()],
            Some(&source),
        )
        .unwrap();
        let after = crate::archive::read_map_relation_snapshots(&manifests);
        assert_eq!(after.entries.len(), 1);
        assert_eq!(
            after
                .entries
                .values()
                .next()
                .unwrap()
                .installed_mapset_version,
            "11.0-fix"
        );
        for name in ["Music", "pear"] {
            assert!(levels_root(&game).join(name).join("info").is_file());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// 安装时记的资产版本要能按身份写进已有条目；表里没有这条时补一条，别丢版本；
/// 本地重新导入时资产来源未知，必须清掉旧资产版本而非冒用上一次的记录。
#[test]
fn installed_asset_version_follows_identity_and_survives_missing_entries() {
    let dir = std::env::temp_dir().join(format!("oc2-mapset-version-{}", uuid::Uuid::new_v4()));
    let mut snapshots = crate::models::MapRelationSnapshots::default();
    snapshots.entries.insert(
        "map".into(),
        crate::models::MapRelationSnapshot {
            repository_id: "map".into(),
            level_set_name: "Set".into(),
            level_set_name_zh: String::new(),
            author: "Author".into(),
            group_i18n: Default::default(),
            group: String::new(),
            relations: Vec::new(),
            release_relations: Vec::new(),
            installed_mapset_version: String::new(),
        },
    );
    crate::archive::write_map_relation_snapshots(&dir, &snapshots).unwrap();

    let set = imported("Set", "Author");
    record_installed_mapset_versions(&dir, &[&set], Some(&origin(&set, " 3.4.1-fix ")));
    let after = crate::archive::read_map_relation_snapshots(&dir);
    assert_eq!(after.entries["map"].installed_mapset_version, "3.4.1-fix");

    let other = imported("Other", "Someone");
    record_installed_mapset_versions(&dir, &[&other], Some(&origin(&other, "0.7")));
    let after = crate::archive::read_map_relation_snapshots(&dir);
    assert_eq!(after.entries.len(), 2);
    assert_eq!(
        after
            .entries
            .values()
            .find(|entry| entry.level_set_name == "Other")
            .unwrap()
            .installed_mapset_version,
        "0.7"
    );

    record_installed_mapset_versions(&dir, &[&set], None);
    assert_eq!(
        crate::archive::read_map_relation_snapshots(&dir).entries["map"].installed_mapset_version,
        ""
    );

    std::fs::remove_dir_all(&dir).unwrap();
}
