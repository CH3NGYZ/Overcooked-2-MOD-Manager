use super::*;

/// BepInEx 表层包的判定：只看 `BepInEx/plugins/` 前缀，且只有 `levels/`
/// 根的直接子目录算关卡集。
#[test]
fn bepinex_layer_detection_is_case_insensitive_and_shallow() {
    assert!(is_bepinex_layer_entry("BepInEx/plugins/Fix/Fix.dll"));
    assert!(is_bepinex_layer_entry("bepinex/PLUGINS/x.dll"));
    assert!(!is_bepinex_layer_entry("BepInEx/config/x.cfg"));
    assert!(!is_bepinex_layer_entry("levels/Set/info"));
    assert!(is_bepinex_layer_zip(&[
        "BepInEx/plugins/OC2DIYLevel/levels/Set/info".to_string()
    ]));
    assert!(!is_bepinex_layer_zip(&["levels/Set/info".to_string()]));

    assert_eq!(
        bepinex_level_set_dir_name("BepInEx/plugins/OC2DIYLevel/levels/MySet"),
        Some("MySet"),
        "目录名保留 ZIP 里的原始大小写"
    );
    assert_eq!(
        bepinex_level_set_dir_name("BepInEx/plugins/OC2DIYLevel/levels"),
        None,
        "levels 根本身不是关卡集"
    );
    assert_eq!(
        bepinex_level_set_dir_name("BepInEx/plugins/OC2DIYLevel/levels/A/B"),
        None,
        "只有直接子目录成卡，更深的目录按原样解压"
    );
}

/// BepInEx 表层包：`levels/` 直接子目录成一张卡，插件带 ZIP 内原路径。
#[test]
fn bepinex_layer_zip_becomes_one_set_with_install_paths() {
    let Some(info) = real_info_bytes() else {
        return;
    };
    let (root, _game, _recycle) = workspace("bepinex-inspect");
    let zip = root.join("layer.zip");
    write_test_zip(
        &zip,
        &[
            (
                "BepInEx/plugins/OC2DIYLevel/levels/RW_LevelSet/info",
                info.as_slice(),
            ),
            ("BepInEx/plugins/Fix/Fix.dll", b"not a real dll"),
        ],
    );

    let sets = inspect_zip(&zip).unwrap();
    assert_eq!(sets.len(), 1);
    let set = &sets[0];
    assert_eq!(set.dir_name, "RW_LevelSet");
    assert_eq!(set.layout, LevelSetLayout::Bepinex);
    assert_eq!(set.plugins.len(), 1);
    assert_eq!(set.plugins[0].file_name, "Fix.dll");
    assert_eq!(
        set.plugins[0].install_path.as_deref(),
        Some("BepInEx/plugins/Fix/Fix.dll"),
        "表层包插件按 ZIP 内原路径落盘"
    );
    assert!(!set.files.is_empty());

    std::fs::remove_dir_all(&root).unwrap();
}

/// 表层包端到端：原样解压、登记归属、扫描并入、MOD 避让、卸载一起进回收站。
#[test]
fn bepinex_layer_install_registers_plugins_and_uninstall_recycles_them() {
    let Some(info) = real_info_bytes() else {
        return;
    };
    let (root, game, recycle) = workspace("bepinex-install");
    let manifests = root.join("manifests");
    let zip = root.join("layer.zip");
    write_test_zip(
        &zip,
        &[
            (
                "BepInEx/plugins/OC2DIYLevel/levels/RW_LevelSet/info",
                info.as_slice(),
            ),
            ("BepInEx/plugins/Fix/Fix.dll", b"not a real dll"),
        ],
    );

    let installed = install_from_zip(&zip, &game, &manifests, &["RW_LevelSet".to_string()])
        .expect("表层包应能安装");
    assert_eq!(installed, vec!["RW_LevelSet".to_string()]);
    assert!(
        game.join("BepInEx/plugins/OC2DIYLevel/levels/RW_LevelSet/info")
            .is_file(),
        "ZIP 内路径原样落盘"
    );
    let plugin = game.join("BepInEx/plugins/Fix/Fix.dll");
    assert!(plugin.is_file());

    let registered = read_registered_plugins(&manifests);
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].dll, "BepInEx/plugins/Fix/Fix.dll");

    // 扫描按身份并入「随包插件」
    let sets = scan_level_sets_with_progress(
        &game,
        Some(&manifests),
        crate::metadata_read::cores(),
        &|_, _, _| {},
    );
    assert_eq!(sets.len(), 1);
    assert!(
        sets[0]
            .plugins
            .iter()
            .any(|plugin| plugin.file_name == "Fix.dll"),
        "登记 DLL 应出现在该集的随包插件里：{:?}",
        sets[0]
            .plugins
            .iter()
            .map(|plugin| plugin.file_name.as_str())
            .collect::<Vec<_>>()
    );

    // MOD 管理页不许出现它
    let mods = crate::mods::scan_mods_with_manifests(
        &game,
        &crate::config::default_config(),
        Some(&manifests),
    );
    assert!(
        !mods.iter().any(|item| item.dll_file.ends_with("Fix.dll")),
        "登记 DLL 不是普通 MOD"
    );

    // 卸载：集目录与登记 DLL 一起进回收站，登记表清空
    remove_level_set(&recycle, &manifests, &game, "RW_LevelSet", true, true).unwrap();
    assert!(
        !game
            .join("BepInEx/plugins/OC2DIYLevel/levels/RW_LevelSet")
            .exists()
    );
    assert!(!plugin.exists(), "登记 DLL 应随关卡集一起进回收站");
    assert!(read_registered_plugins(&manifests).is_empty(), "登记要清掉");

    std::fs::remove_dir_all(&root).unwrap();
}
