use super::*;

#[test]
fn disabled_dll_names_keep_the_same_logical_path() {
    assert_eq!(
        super::canonical_dll_name("BepInEx/plugins/example.dll.disabled"),
        "BepInEx/plugins/example.dll"
    );
    assert_eq!(
        super::disabled_dll_file("BepInEx/plugins/example.dll"),
        "BepInEx/plugins/example.dll.disabled"
    );
}

/// 恢复框架留下的 `.before-restore-*` 目录不能被当成 MOD。
///
/// 这是真实用户会踩的坑：恢复 DIYLevel 时，暂存目录就建在
/// `BepInEx/plugins/OC2DIYLevel.before-restore-<ts>`，里面的 DLL 被扫成
/// 插件后，界面上「刚恢复完就冒出一堆重复 MOD」，而且因为文件确实存在，
/// 用户只能自己翻目录手删才能消除。
///
/// 断言方式是**对比两次扫描**：先放一个暂存目录，期望只扫到真实 MOD。
/// 这个测试必须能失败 —— 把 `scan_mods_with_manifests` 里的跳过分支删掉
/// 时它应当红。为此还要断言暂存目录**确实在磁盘上**（否则「扫不到」
/// 可能只是因为文件没建成，测试就变成了空转）。
#[test]
fn restore_staging_directories_are_not_scanned_as_mods() {
    let root = std::env::temp_dir().join(format!("oc2-scan-stage-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(&plugins).unwrap();

    // 正常 MOD
    std::fs::create_dir_all(plugins.join("RealMod")).unwrap();
    std::fs::write(
        plugins.join("RealMod/Real.dll"),
        plugin_blob("com.example.real", "Real", "1.0.0"),
    )
    .unwrap();

    // 恢复残留：DIYLevel 的暂存目录就在 plugins 下
    let staging = plugins.join("OC2DIYLevel.before-restore-1793336706");
    std::fs::create_dir_all(&staging).unwrap();
    let staged_dll = staging.join("OC2DIYLevel.dll");
    std::fs::write(
        &staged_dll,
        plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "1.0.0"),
    )
    .unwrap();

    // 前置条件：暂存 DLL 真的在磁盘上，且确实是 .dll（否则下面「扫不到」
    // 可能是 `is_mod_file` 不认它，而不是过滤生效）
    assert!(staged_dll.is_file(), "夹具没建好，测试会空转");
    assert!(super::is_mod_file(&staged_dll));

    let config = crate::config::default_config();
    let scanned = super::scan_mods_with_manifests(&root, &config, None);

    assert!(
        !scanned
            .iter()
            .any(|item| item.dll_file.contains(".before-restore-")),
        "暂存目录里的文件不应出现在扫描结果里：{:?}",
        scanned
            .iter()
            .map(|item| &item.dll_file)
            .collect::<Vec<_>>()
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 关卡集目录里的随包 DLL 归地图管，不能同时出现在本地 MOD 列表。
///
/// 两套状态会打架：MOD 列表能把它改成 `.dll.disabled`，地图页又按
/// 「整目录 rename」开关，同一份文件于是有两个真相。
#[test]
fn bundled_level_set_plugins_are_not_scanned_as_mods() {
    let root = std::env::temp_dir().join(format!("oc2-scan-map-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(&plugins).unwrap();

    std::fs::create_dir_all(plugins.join("RealMod")).unwrap();
    std::fs::write(
        plugins.join("RealMod/Real.dll"),
        plugin_blob("com.example.real", "Real", "1.0.0"),
    )
    .unwrap();

    // 地图集目录：随包 DLL 与地图资源同处一个父目录
    let set_dir = plugins.join("OC2DIYLevel/levels/MySet");
    std::fs::create_dir_all(&set_dir).unwrap();
    std::fs::write(set_dir.join("info"), b"map-directory-fixture").unwrap();
    let bundled = set_dir.join("BundledFix.dll");
    std::fs::write(
        &bundled,
        plugin_blob("com.example.bundled", "Bundled", "1.0.0"),
    )
    .unwrap();

    // 前置条件：它确实是会被 is_mod_file 认下的文件（否则「扫不到」
    // 可能是过滤谓词不认它，而不是排除生效）
    assert!(bundled.is_file(), "夹具没建好，测试会空转");
    assert!(super::is_mod_file(&bundled));

    let config = crate::config::default_config();
    let scanned = super::scan_mods_with_manifests(&root, &config, None);
    let files: Vec<&str> = scanned.iter().map(|item| item.dll_file.as_str()).collect();
    assert!(
        !files.iter().any(|file| file.contains("levels")),
        "关卡集目录里的 DLL 不应出现在 MOD 列表：{files:?}"
    );
    assert!(
        files.iter().any(|file| file.ends_with("Real.dll")),
        "真实 MOD 仍要扫到：{files:?}"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 单测过滤谓词本身：目录名判定不依赖扫描管线的其它环节。
#[test]
fn staging_component_detection_matches_only_before_restore_dirs() {
    assert!(super::is_restore_staging_component(std::ffi::OsStr::new(
        "OC2DIYLevel.before-restore-1793336706"
    )));
    assert!(super::is_restore_staging_component(std::ffi::OsStr::new(
        "BepInEx.before-restore-1"
    )));
    // 正常目录不能误伤
    assert!(!super::is_restore_staging_component(std::ffi::OsStr::new(
        "OC2DIYLevel"
    )));
    assert!(!super::is_restore_staging_component(std::ffi::OsStr::new(
        "SomeMod"
    )));
    assert!(!super::is_restore_staging_component(std::ffi::OsStr::new(
        "before-restore"
    )));
}

#[test]
fn plugin_dll_files_returns_game_relative_paths_for_a_lexical_root() {
    let root = std::env::temp_dir().join(format!("oc2-plugin-dlls-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(plugins.join("Nested")).unwrap();
    // 地图随包插件：与 `scan_mods_with_manifests` 一样不许当成普通 MOD
    std::fs::create_dir_all(plugins.join("OC2DIYLevel/levels/SomeSet")).unwrap();
    std::fs::create_dir_all(plugins.join("OC2DIYLevel/disabled/OtherSet")).unwrap();
    std::fs::write(
        plugins.join("OC2DIYLevel/levels/SomeSet/info"),
        b"map-fixture",
    )
    .unwrap();
    std::fs::write(
        plugins.join("OC2DIYLevel/disabled/OtherSet/info"),
        b"map-fixture",
    )
    .unwrap();
    std::fs::write(plugins.join("OC2DIYLevel/levels/SomeSet/Plugin.dll"), b"x").unwrap();
    std::fs::write(
        plugins.join("OC2DIYLevel/levels/SomeSet/Plugin.dll.disabled"),
        b"x",
    )
    .unwrap();
    std::fs::write(
        plugins.join("OC2DIYLevel/disabled/OtherSet/Other.dll"),
        b"x",
    )
    .unwrap();
    std::fs::write(plugins.join("Alpha.dll"), b"x").unwrap();
    std::fs::write(plugins.join("Beta.dll.disabled"), b"x").unwrap();
    std::fs::write(plugins.join("Nested/Gamma.dll"), b"x").unwrap();
    std::fs::write(plugins.join("readme.txt"), b"x").unwrap();

    // 入参是未 canonicalize 的路径（`detect_game().path` 就是这个形态），
    // 与 safe_game_path 解析出的 verbatim 目录不是同一个字符串：
    // 直接用入参做 strip_prefix 会一个文件都收集不到。
    let files = super::plugin_dll_files(&root, None).unwrap();
    assert_eq!(
        files,
        vec![
            "BepInEx/plugins/Alpha.dll".to_string(),
            "BepInEx/plugins/Beta.dll.disabled".to_string(),
            "BepInEx/plugins/Nested/Gamma.dll".to_string(),
        ]
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 登记在 `manifests/level-set-plugins.json` 的 DLL 归地图管，不许出现在
/// MOD 列表或批量启停清单里；去掉登记后又必须能扫到（否则这条测试只是
/// 在证明「文件没建成」）。
#[test]
fn registered_level_set_plugins_are_not_scanned_as_mods() {
    let root = std::env::temp_dir().join(format!("oc2-registry-scan-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    std::fs::create_dir_all(plugins.join("Fix")).unwrap();
    std::fs::write(
        plugins.join("Real.dll"),
        plugin_blob("com.example.real", "Real", "1.0.0"),
    )
    .unwrap();
    std::fs::write(
        plugins.join("Fix/Fix.dll"),
        plugin_blob("com.example.fix", "Fix", "1.0.0"),
    )
    .unwrap();
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&manifests).unwrap();
    std::fs::write(
        manifests.join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        r#"{"version":1,"entries":[{"dll":"BepInEx/plugins/Fix/Fix.dll","owner":{"name":"Set"}}]}"#,
    )
    .unwrap();

    let config = crate::config::default_config();
    let scanned = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
    let names: Vec<&str> = scanned.iter().map(|item| item.dll_file.as_str()).collect();
    assert_eq!(
        names,
        vec!["BepInEx/plugins/Real.dll"],
        "登记 DLL 不该出现在 MOD 列表"
    );

    let files = super::plugin_dll_files(&root, Some(&manifests)).unwrap();
    assert_eq!(files, vec!["BepInEx/plugins/Real.dll".to_string()]);

    let without = super::scan_mods_with_manifests(&root, &config, None);
    assert!(
        without
            .iter()
            .any(|item| item.dll_file.ends_with("Fix.dll")),
        "去掉登记后应重新被当成 MOD"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 目录名不再写死 OC2DIYLevel；地图里的同 GUID 高版本 DLL 不能抢走实际根。
#[test]
fn dynamic_map_directories_do_not_pollute_guid_selection_or_bulk_toggles() {
    let root = std::env::temp_dir().join(format!("oc2-dynamic-map-dlls-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    for (path, version) in [
        ("CustomLevel-v2/Plugin.dll", "2.0.0"),
        ("CustomLevel-v2/levels/One/Fake.dll", "99.0.0"),
        ("CustomLevel-v2/LEVELS/Two/Disabled.dll.disabled", "98.0.0"),
        ("CustomLevel-v2/disabled/Three/Legacy.dll", "97.0.0"),
        ("CustomLevel-v2.before-restore-123/Plugin.dll", "96.0.0"),
    ] {
        let target = plugins.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(
            &target,
            plugin_blob("dev.gua.overcooked.diylevel", path, version),
        )
        .unwrap();
        assert!(is_mod_file(&target));
    }
    let scanned = scan_mods_with_manifests(&root, &crate::config::default_config(), None);
    assert_eq!(scanned.len(), 1);
    let selected =
        crate::mods::plugin_target::unique(&scanned, crate::mods::plugin_target::DIYLEVEL_GUID)
            .unwrap();
    assert_eq!(
        selected.dll_file,
        "BepInEx/plugins/CustomLevel-v2/Plugin.dll"
    );
    let files = plugin_dll_files(&root, None).unwrap();
    assert_eq!(files, ["BepInEx/plugins/CustomLevel-v2/Plugin.dll"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registered_disabled_map_plugin_is_excluded_from_scan_and_bulk_toggles() {
    let root = std::env::temp_dir().join(format!(
        "oc2-disabled-map-registry-{}",
        uuid::Uuid::new_v4()
    ));
    let dll = root.join("BepInEx/plugins/Fix/Fix.dll.disabled");
    std::fs::create_dir_all(dll.parent().unwrap()).unwrap();
    std::fs::write(
        &dll,
        plugin_blob("dev.gua.overcooked.diylevel", "Fix", "99.0.0"),
    )
    .unwrap();
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&manifests).unwrap();
    std::fs::write(
        manifests.join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        r#"{"version":1,"entries":[{"dll":"BepInEx/plugins/Fix/Fix.dll","owner":{"name":"Set"}}]}"#,
    )
    .unwrap();
    assert!(
        scan_mods_with_manifests(&root, &crate::config::default_config(), Some(&manifests))
            .is_empty()
    );
    assert!(
        plugin_dll_files(&root, Some(&manifests))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        scan_mods_with_manifests(&root, &crate::config::default_config(), None).len(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ordinary_plugins_in_levels_or_disabled_named_directories_remain_visible_and_toggleable() {
    let root =
        std::env::temp_dir().join(format!("oc2-ordinary-named-dirs-{}", uuid::Uuid::new_v4()));
    let expected = [
        "BepInEx/plugins/disabled/Tool.dll",
        "BepInEx/plugins/levels/Tool.dll",
        "BepInEx/plugins/Other/disabled/Tool.dll",
        "BepInEx/plugins/Other/levels/Tool.dll.disabled",
    ];
    for (index, relative) in expected.iter().enumerate() {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            plugin_blob(&format!("com.example.ordinary.{index}"), "Tool", "1.0.0"),
        )
        .unwrap();
    }
    let mut scanned = scan_mods_with_manifests(&root, &crate::config::default_config(), None)
        .into_iter()
        .map(|item| {
            if item.enabled {
                item.dll_file
            } else {
                disabled_dll_file(&item.dll_file)
            }
        })
        .collect::<Vec<_>>();
    scanned.sort_by_key(|path| path.to_ascii_lowercase());
    let toggleable = plugin_dll_files(&root, None).unwrap();
    std::fs::remove_dir_all(root).unwrap();

    assert_eq!(scanned, expected);
    assert_eq!(toggleable, expected);
}

#[test]
fn diylevel_package_named_levels_owns_its_map_subtrees_without_hiding_sibling_mods() {
    let root = std::env::temp_dir().join(format!("oc2-named-diy-package-{}", uuid::Uuid::new_v4()));
    for (relative, guid, version) in [
        (
            "Regular/Framework.dll",
            "dev.gua.overcooked.diylevel",
            "1.0.0",
        ),
        (
            "levels/Framework.dll.disabled",
            "dev.gua.overcooked.diylevel",
            "2.0.0",
        ),
        ("levels/Tool.dll", "com.example.ordinary", "1.0.0"),
        (
            "levels/levels/Map/Fake.dll",
            "dev.gua.overcooked.diylevel",
            "99.0.0",
        ),
        (
            "levels/disabled/Map/Fake.dll.disabled",
            "dev.gua.overcooked.diylevel",
            "98.0.0",
        ),
    ] {
        let path = root.join("BepInEx/plugins").join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, plugin_blob(guid, relative, version)).unwrap();
    }
    let scanned = scan_mods_with_manifests(&root, &crate::config::default_config(), None);
    let toggleable = plugin_dll_files(&root, None).unwrap();
    std::fs::remove_dir_all(root).unwrap();

    assert_eq!(scanned.len(), 3);
    let selected =
        crate::mods::plugin_target::unique(&scanned, crate::mods::plugin_target::DIYLEVEL_GUID)
            .unwrap();
    assert_eq!(selected.dll_file, "BepInEx/plugins/levels/Framework.dll");
    assert!(!selected.enabled);
    assert_eq!(
        toggleable,
        [
            "BepInEx/plugins/levels/Framework.dll.disabled",
            "BepInEx/plugins/levels/Tool.dll",
            "BepInEx/plugins/Regular/Framework.dll",
        ]
    );
}

#[test]
fn removed_diylevel_packages_require_fixed_or_unique_info_before_claiming_map_plugins() {
    let root =
        std::env::temp_dir().join(format!("oc2-orphan-map-ownership-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    for directory in [
        "Gone/levels/Unique",
        "Gone/disabled/Fixed/Nested",
        "Other/levels/Ambiguous",
    ] {
        let path = plugins.join(directory).join("Tool.dll");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, plugin_blob("com.example.tool", directory, "1.0.0")).unwrap();
    }
    for relative in [
        "Gone/levels/Unique/info_custom",
        "Gone/disabled/Fixed/info",
        "Gone/disabled/Fixed/info_one",
        "Gone/disabled/Fixed/info_two",
        "Other/levels/Ambiguous/info_one",
        "Other/levels/Ambiguous/info_two",
    ] {
        std::fs::write(plugins.join(relative), b"map-directory-fixture").unwrap();
    }

    let scanned = scan_mods_with_manifests(&root, &crate::config::default_config(), None);
    let toggleable = plugin_dll_files(&root, None).unwrap();
    std::fs::remove_dir_all(root).unwrap();

    assert_eq!(scanned.len(), 1);
    assert_eq!(
        scanned[0].dll_file,
        "BepInEx/plugins/Other/levels/Ambiguous/Tool.dll"
    );
    assert_eq!(
        toggleable,
        ["BepInEx/plugins/Other/levels/Ambiguous/Tool.dll"]
    );
}

#[test]
fn map_ownership_never_uses_directory_names_above_the_plugins_root() {
    let workspace = std::env::temp_dir().join(format!(
        "oc2-plugin-ownership-boundary-{}",
        uuid::Uuid::new_v4()
    ));
    let root = workspace.join("levels/Game");
    let path = root.join("BepInEx/plugins/Tool.dll");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(root.join("info"), b"outside-plugins-fixture").unwrap();
    std::fs::write(path, plugin_blob("com.example.tool", "Tool", "1.0.0")).unwrap();

    let scanned = scan_mods_with_manifests(&root, &crate::config::default_config(), None);
    let toggleable = plugin_dll_files(&root, None).unwrap();
    std::fs::remove_dir_all(workspace).unwrap();

    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].dll_file, "BepInEx/plugins/Tool.dll");
    assert_eq!(toggleable, ["BepInEx/plugins/Tool.dll"]);
}
