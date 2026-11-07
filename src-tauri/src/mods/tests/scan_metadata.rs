use super::*;

#[test]
fn manifest_group_snapshot_survives_missing_repository_config() {
    // 安装清单里固化了分组：即使当前配置里找不到这个仓库
    // （刚启动、源文档还没刷新出来），MOD 仍然带着安装时的分组。
    let root = std::env::temp_dir().join(format!("oc2-group-snapshot-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    let path = plugins.join("Grouped.dll");
    let mut bytes = b"BepInPlugin".to_vec();
    bytes.extend_from_slice(&[1, 0]);
    for value in ["oc2.grouped", "Grouped", "1.0.0"] {
        bytes.push(value.len() as u8);
        bytes.extend_from_slice(value.as_bytes());
    }
    std::fs::write(&path, bytes).unwrap();
    let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1,"ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[]
    }))
    .unwrap();
    let mut group_i18n = std::collections::BTreeMap::new();
    group_i18n.insert("zh-CN".into(), "内容拓展".to_string());
    group_i18n.insert("en".into(), "Content".to_string());
    let manifest = crate::archive::InstallManifest {
        repository_id: "gone-repository".into(),
        release_tag: "v1.0.0".into(),
        repository_binding: None,
        package_name_i18n: crate::archive::single_language("Grouped"),
        note_i18n: Default::default(),
        group_i18n: group_i18n.clone(),
        prerelease: false,
        relations: Vec::new(),
        installed_at: "2026-10-07T00:00:00Z".into(),
        files: vec!["BepInEx/plugins/Grouped.dll".into()],
    };
    let installed = super::inspect_dll_at(&path, &root, &config, std::slice::from_ref(&manifest));
    assert_eq!(installed.group_i18n, group_i18n);
    assert_eq!(installed.group, "内容拓展");
    // 仓库已不在配置里 → 状态仍是未绑定，但分组信息不丢
    assert_eq!(installed.status, "unbound");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn manifest_prerelease_flag_reaches_installed_mod() {
    // 预发布标记随安装清单固化，扫描时原样带出（仓库已不在配置里也不丢）。
    let root = std::env::temp_dir().join(format!("oc2-pre-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    let path = plugins.join("Pre.dll");
    let mut bytes = b"BepInPlugin".to_vec();
    bytes.extend_from_slice(&[1, 0]);
    for value in ["oc2.pre", "Pre", "2.0.0"] {
        bytes.push(value.len() as u8);
        bytes.extend_from_slice(value.as_bytes());
    }
    std::fs::write(&path, bytes).unwrap();
    let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1,"ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[]
    }))
    .unwrap();
    let manifest = crate::archive::InstallManifest {
        repository_id: "pre-repo".into(),
        release_tag: "v2.0.0-beta.1".into(),
        repository_binding: None,
        package_name_i18n: crate::archive::single_language("Pre"),
        note_i18n: Default::default(),
        group_i18n: Default::default(),
        prerelease: true,
        relations: Vec::new(),
        installed_at: "2026-10-07T00:00:00Z".into(),
        files: vec!["BepInEx/plugins/Pre.dll".into()],
    };
    let installed = super::inspect_dll_at(&path, &root, &config, std::slice::from_ref(&manifest));
    assert!(installed.prerelease);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_manifest_without_snapshot_falls_back_to_repository_group() {
    // 旧版本管理器写的清单没有 group_i18n / group。
    // 升级后应立刻用当前仓库的分组兜底显示，而不是掉进「未绑定」。
    let root = std::env::temp_dir().join(format!("oc2-legacy-group-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    let path = plugins.join("Legacy.dll");
    let mut bytes = b"BepInPlugin".to_vec();
    bytes.extend_from_slice(&[1, 0]);
    for value in ["oc2.legacy", "Legacy", "1.0.0"] {
        bytes.push(value.len() as u8);
        bytes.extend_from_slice(value.as_bytes());
    }
    std::fs::write(&path, bytes).unwrap();

    let mut repository = binding_repository();
    repository.id = "legacy-repo".into();
    // 用测试 DLL 真实的 GUID 识别，否则扫描时绑不到这个仓库
    repository.plugin_guid = "oc2.legacy".into();
    repository.group = "实用工具".into();
    repository
        .group_i18n
        .insert("zh-CN".into(), "实用工具".into());
    repository
        .group_i18n
        .insert("en".into(), "Utilities".into());
    let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1,"ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[repository]
    }))
    .unwrap();

    // 旧清单：分组与绑定都为空
    let manifest = crate::archive::InstallManifest {
        repository_id: "legacy-repo".into(),
        release_tag: "v1.0.0".into(),
        repository_binding: None,
        package_name_i18n: crate::archive::single_language("Legacy"),
        note_i18n: Default::default(),
        group_i18n: Default::default(),
        prerelease: false,
        relations: Vec::new(),
        installed_at: "2026-10-07T00:00:00Z".into(),
        files: vec!["BepInEx/plugins/Legacy.dll".into()],
    };
    let installed = super::inspect_dll_at(&path, &root, &config, std::slice::from_ref(&manifest));
    // 分组回退到仓库当前值
    assert_eq!(installed.group, "实用工具");
    assert_eq!(
        installed.group_i18n.get("en").map(String::as_str),
        Some("Utilities")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_mod_scan_returns_unknown_without_readable_bepinplugin_version() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/bepinplugin-version")
        .join(uuid::Uuid::new_v4().to_string());
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "ui_zoom": 1.5,
        "game": {"path": ""},
        "network": {"timeout_seconds": 5, "download_threads": 1, "max_retry_count": 3},
        "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
        "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
        "repositories": []
    }))
    .unwrap();
    let mut truncated = plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "0.10.0");
    truncated.pop();
    let cases = [
        (
            "no-attribute",
            b"AssemblyVersion\0FileVersion\0ProductVersion\09.9.9.9".to_vec(),
        ),
        (
            "empty",
            plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", ""),
        ),
        (
            "blank",
            plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "   "),
        ),
        (
            "invalid",
            plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "not-a-version"),
        ),
        ("truncated", truncated),
    ];
    for (name, bytes) in cases {
        for suffix in [".dll", ".dll.disabled"] {
            let path = plugins.join(format!("{name}{suffix}"));
            std::fs::write(&path, &bytes).unwrap();
            let installed = super::inspect_dll_at(&path, &root, &config, &[]);
            assert_eq!(installed.version, None, "{name}{suffix}");
            assert_eq!(installed.version_source, "unknown", "{name}{suffix}");
            assert!(!installed.update_available);
            assert_eq!(installed.enabled, suffix == ".dll");
        }
    }
    let scan = super::scan_mods_with_manifests(&root, &config, None);
    assert_eq!(scan.len(), 5);
    assert!(
        scan.iter()
            .all(|item| item.version.is_none() && item.version_source == "unknown")
    );
    let verified = std::fs::canonicalize(&root).unwrap();
    let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    assert!(verified.starts_with(workspace.join("target/verification/bepinplugin-version")));
    std::fs::remove_dir_all(&root).unwrap();
}
