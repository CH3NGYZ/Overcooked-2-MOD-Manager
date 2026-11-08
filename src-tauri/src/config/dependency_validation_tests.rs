use crate::config::validation::validate_mod_repository;
use crate::models::*;

fn base_export(relations: Vec<RepositoryRelation>) -> ModRepositoryExport {
    ModRepositoryExport {
        network_plan: Default::default(),
        kind: Default::default(),
        name: "dep-owner".into(),
        name_i18n: std::collections::BTreeMap::new(),
        group: String::new(),
        group_i18n: std::collections::BTreeMap::new(),
        provider: Provider::Github,
        username: "a".into(),
        repository: "b".into(),
        download_mode: ModDownloadMode::Release,
        branch: String::new(),
        file_path: String::new(),
        region: None,
        platform: None,
        channel: Channel::Stable,
        asset_rules: vec![AssetRule {
            rule_type: AssetRuleType::Exact,
            value: "a.zip".into(),
        }],
        install_target: "BepInEx/plugins".into(),
        note: String::new(),
        note_i18n: std::collections::BTreeMap::new(),
        plugin_guid: String::new(),
        local_product_name: String::new(),
        local_original_filename: String::new(),
        relations,
        release_relations: Vec::new(),
        version_regex: String::new(),
        max_release_count: 10,
        download_threads: 1,
        enabled: true,
        source_id: None,
    }
}

fn relation(kind: RelationKind, version: &str, optional: bool) -> RepositoryRelation {
    RepositoryRelation {
        kind,
        guid: "com.example.dep".into(),
        version: version.into(),
        prerelease: None,
        optional,
    }
}

#[test]
fn relation_without_guid_is_rejected() {
    let mut entry = relation(RelationKind::Requires, "", false);
    entry.guid = "   ".into();
    assert!(validate_mod_repository(&base_export(vec![entry])).is_err());
}

#[test]
fn relation_with_unparsable_min_version_is_rejected() {
    let entry = relation(RelationKind::Requires, ">=not-a-version", false);
    assert!(validate_mod_repository(&base_export(vec![entry])).is_err());
}

#[test]
fn relation_with_unparsable_max_version_is_rejected() {
    let entry = relation(RelationKind::Compatible, "<=not-a-version", false);
    assert!(validate_mod_repository(&base_export(vec![entry])).is_err());
}

/// 区间反了（min > max）永远匹配不到任何插件，属于配置错误。
#[test]
fn inverted_relation_range_is_rejected() {
    let entry = relation(RelationKind::Conflicts, ">=2.0,<=1.0", false);
    assert!(validate_mod_repository(&base_export(vec![entry])).is_err());
}

/// 合法关系（含四段 beta 序号写法）应当通过。
#[test]
fn valid_relations_pass() {
    for version in ["", "*", ">=1.8.7", ">=1.8.7.2", ">=1.8.7-beta.2", "==1.8.7"] {
        let entry = relation(RelationKind::Requires, version, false);
        assert!(
            validate_mod_repository(&base_export(vec![entry])).is_ok(),
            "version={version}"
        );
    }
    // 裸版本号没有比较符，迁移自 min_version 时会静默收窄成「恰好等于」
    for version in ["1.8.7", "1.8.7,<=2.0"] {
        let entry = relation(RelationKind::Requires, version, false);
        assert!(
            validate_mod_repository(&base_export(vec![entry])).is_err(),
            "version={version}"
        );
    }
    // conflicts / compatible 同样接受双端区间
    for kind in [RelationKind::Conflicts, RelationKind::Compatible] {
        let entry = relation(kind, ">=1.0,<=2.0", false);
        assert!(
            validate_mod_repository(&base_export(vec![entry])).is_ok(),
            "kind={}",
            kind.as_str()
        );
    }
}

/// `optional` 只属于依赖：冲突与兼容关系标记可选是自相矛盾的。
#[test]
fn optional_is_rejected_for_non_requirement_relations() {
    for kind in [RelationKind::Conflicts, RelationKind::Compatible] {
        let entry = relation(kind, "", true);
        let error = validate_mod_repository(&base_export(vec![entry])).unwrap_err();
        assert!(
            error.to_string().contains("relationOptionalUnsupported"),
            "{error}"
        );
    }
}

/// Release 覆盖表的区间两端全空等同于不限版本，应当拒绝。
#[test]
fn release_relations_without_any_bound_is_rejected() {
    let mut export = base_export(Vec::new());
    export.release_relations.push(ReleaseRelationOverride {
        version: String::new(),
        relations: Vec::new(),
    });
    let error = validate_mod_repository(&export).unwrap_err();
    assert!(error.to_string().contains("relationsEmpty"), "{error}");
}

#[test]
fn self_conflict_without_prerelease_flag_is_rejected() {
    let mut export = base_export(Vec::new());
    export.plugin_guid = "com.example.owner".into();
    let mut entry = relation(RelationKind::Conflicts, "", false);
    entry.guid = "com.example.owner".into();
    export.relations = vec![entry];
    let error = validate_mod_repository(&export).unwrap_err();
    assert!(error.to_string().contains("relationSelfGuid"), "{error}");
}

/// 带了 prerelease 的自冲突是合法写法（正式版排斥测试版）。
#[test]
fn self_conflict_with_prerelease_flag_passes() {
    for flag in [true, false] {
        let mut export = base_export(Vec::new());
        export.plugin_guid = "com.example.owner".into();
        let mut entry = relation(RelationKind::Conflicts, "", false);
        entry.guid = "com.example.owner".into();
        entry.prerelease = Some(flag);
        export.relations = vec![entry];
        assert!(
            validate_mod_repository(&export).is_ok(),
            "prerelease={flag}"
        );
    }
}

/// `prerelease` 不能用在依赖上：那是「必须是测试版依赖」，语义应写进版本区间。
#[test]
fn prerelease_is_rejected_for_requires() {
    let mut entry = relation(RelationKind::Requires, "", false);
    entry.prerelease = Some(true);
    let error = validate_mod_repository(&base_export(vec![entry])).unwrap_err();
    assert!(
        error.to_string().contains("relationRequiresPrerelease"),
        "{error}"
    );
}
