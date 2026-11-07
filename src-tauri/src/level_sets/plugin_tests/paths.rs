use super::*;

#[test]
fn disabled_root_lives_outside_the_bepinex_plugin_tree() {
    let root = Path::new("C:/Game");
    let disabled = disabled_root(root);
    assert_eq!(
        disabled,
        root.join("BepInEx/cache/modmanager_disabled_maps"),
        "停用根的位置是跨设备兼容约定，不要随手改"
    );
    // 关键断言：BepInEx 只递归扫 plugins/，所以停用根必须在它之外
    let relative = disabled.strip_prefix(root).unwrap();
    assert!(
        !relative.components().any(|part| part
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("plugins")),
        "停用根不能落在 plugins 树里，否则停用的地图带的修复 DLL 照样被加载"
    );
}

#[test]
fn migrates_the_legacy_disabled_root() {
    let root = std::env::temp_dir().join(format!("oc2-lv-legacy-{}", uuid::Uuid::new_v4()));
    let legacy = root.join(LEGACY_DISABLED_ROOT);
    std::fs::create_dir_all(legacy.join("Old")).unwrap();
    std::fs::write(legacy.join("Old/info"), b"x").unwrap();

    migrate_legacy_disabled_root(&root);
    assert!(
        disabled_root(&root).join("Old/info").is_file(),
        "旧停用根应整体搬到新位置"
    );
    assert!(!legacy.exists(), "搬空后旧目录壳应删掉");

    // 两边都有时：不覆盖新位置，把旧目录原样留在原地
    let legacy_c = root.join(LEGACY_DISABLED_ROOT);
    std::fs::create_dir_all(legacy_c.join("Clash")).unwrap();
    std::fs::write(legacy_c.join("Clash/info"), b"old").unwrap();
    std::fs::create_dir_all(disabled_root(&root).join("Clash")).unwrap();
    std::fs::write(disabled_root(&root).join("Clash/info"), b"new").unwrap();
    migrate_legacy_disabled_root(&root);
    assert_eq!(
        std::fs::read(disabled_root(&root).join("Clash/info")).unwrap(),
        b"new",
        "同名目录不许覆盖新位置的数据"
    );
    assert!(legacy_c.join("Clash/info").is_file(), "冲突项留在旧位置");

    std::fs::remove_dir_all(&root).unwrap();
}
