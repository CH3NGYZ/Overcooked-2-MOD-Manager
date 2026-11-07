use crate::models::*;

/// 测试用的最小 BepInPlugin 字节序列（`[1,0]` 头 + 长度前缀字符串）。
///
/// 放在模块级而不是某个测试模块里：MOD 预览、地图随包插件两处用例都要造
/// 「能读出元数据的假 DLL」，两份拷贝会随格式变化各自过期。
#[cfg(test)]
pub(crate) fn plugin_blob(guid: &str, name: &str, version: &str) -> Vec<u8> {
    let mut bytes = b"BepInPlugin".to_vec();
    bytes.extend_from_slice(&[1, 0]);
    for value in [guid, name, version] {
        bytes.push(value.len() as u8);
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes
}

pub(super) fn duplicate_configuration_manager(
    version: &str,
    online_version: &str,
) -> crate::models::InstalledMod {
    serde_json::from_value(serde_json::json!({
        "repository_id": "configuration-manager", "repository_ids": ["configuration-manager"],
        "name": "Configuration Manager", "guid": "com.bepis.bepinex.configurationmanager",
        "dll_file": format!("BepInEx/plugins/{version}/ConfigurationManager.dll"),
        "enabled": true, "version": version, "version_source": "BepInPlugin",
        "update_available": super::compare_mod_versions(online_version, version) == Some(std::cmp::Ordering::Greater),
        "duplicate_guid": false, "status": "latest"
    })).unwrap()
}

pub(super) fn binding_repository() -> crate::models::RepositoryConfig {
    crate::models::RepositoryConfig {
        id: "test".into(),
        name: "Test".into(),
        name_i18n: std::collections::BTreeMap::new(),
        group: String::new(),
        group_i18n: std::collections::BTreeMap::new(),
        provider: crate::models::Provider::Gitee,
        username: "example".into(),
        repository: "mod".into(),
        download_mode: crate::models::RepositoryDownloadMode::Release,
        branch: String::new(),
        file_path: String::new(),
        region: None,
        channel: crate::models::Channel::All,
        platform: None,
        asset_rules: Vec::new(),
        install_target: String::new(),
        note: String::new(),
        note_i18n: std::collections::BTreeMap::new(),
        plugin_guid: "com.example.guid".into(),
        local_product_name: "Example Product".into(),
        local_original_filename: "Example.dll".into(),
        relations: Vec::new(),
        release_relations: Vec::new(),
        version_regex: String::new(),
        max_release_count: 10,
        download_threads: 1,
        github_token: String::new(),
        gitee_token: String::new(),
        official_api_only: true,
        enabled: true,
        builtin: false,
        pinned_version: None,
        ignored_version: None,
        etag: None,
        cached_releases: Vec::new(),
        releases_cached_at: None,
        last_error: None,
        source_id: None,
    }
}

pub(super) fn high_cached_release() -> crate::models::ReleaseInfo {
    crate::models::ReleaseInfo {
        release_id: None,
        tag_name: "v99.0.0".into(),
        version: Some("99.0.0".into()),
        name: "newer Release".into(),
        body: String::new(),
        published_at: None,
        prerelease: false,
        draft: false,
        assets: Vec::new(),
    }
}

pub(super) fn plain_mod(dll_file: &str, extra: serde_json::Value) -> crate::models::InstalledMod {
    let mut base = serde_json::json!({
        "name": dll_file, "dll_file": dll_file, "enabled": true,
        "version_source": "BepInPlugin", "update_available": false,
        "duplicate_guid": false, "status": "latest",
    });
    if let (Some(base_map), Some(extra_map)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra_map {
            base_map.insert(key.clone(), value.clone());
        }
    }
    serde_json::from_value(base).unwrap()
}

pub(super) fn requires(guid: &str, version: &str) -> crate::models::RepositoryRelation {
    crate::models::RepositoryRelation {
        kind: crate::models::RelationKind::Requires,
        guid: guid.into(),
        version: version.into(),
        prerelease: None,
        optional: false,
    }
}

pub(super) fn override_entry(
    version: &str,
    relations: Vec<crate::models::RepositoryRelation>,
) -> crate::models::ReleaseRelationOverride {
    crate::models::ReleaseRelationOverride {
        version: version.into(),
        relations,
    }
}

/// 用源文档造一张地图条目：顺手覆盖 `[[maps.release_relations]]` 的解析。
pub(super) fn map_entry(release_relations: &str) -> crate::models::MapRepositoryConfig {
    let text = format!(
        r#"
[[maps]]
name = "Map"
provider = "github"
username = "u"
repository = "r"
level_set_name = "Map_Set"
author = "A"
asset_rules = [{{ type = "glob", value = "map_v*.zip" }}]
relations = [{{ kind = "requires", guid = "base.guid", version = ">=0.10.0" }}]
{release_relations}
"#
    );
    crate::map_sources::parse_document(
        &text,
        &crate::repository_sources::SourceLocation::parse("maps.toml").unwrap(),
    )
    .unwrap()
    .maps
    .into_iter()
    .next()
    .unwrap()
}

pub(crate) fn repository(id: &str, mode: RepositoryDownloadMode) -> RepositoryConfig {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": id, "provider": "github",
        "username": "example", "repository": "mod", "download_mode": mode,
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "cached_releases": [{
            "tag_name": "v99.0.0", "version": "99.0.0", "name": "newer release",
            "body": "", "published_at": null, "prerelease": false, "draft": false,
            "assets": []
        }]
    }))
    .unwrap()
}

pub(crate) fn config(repositories: Vec<RepositoryConfig>) -> AppConfig {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "ui_zoom": 1.5,
        "game": {"path": ""},
        "network": {"timeout_seconds": 5, "download_threads": 1,
            "max_retry_count": 3},
        "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
        "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
        "repositories": repositories
    }))
    .unwrap()
}
