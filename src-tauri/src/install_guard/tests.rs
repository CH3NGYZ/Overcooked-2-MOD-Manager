use super::*;
// 只有 junction 那条 Windows 专属测试用得到 fs。
#[cfg(windows)]
use std::fs;

#[test]
fn rejects_plugins_and_descendants_with_case_variants() {
    let root = std::env::temp_dir();
    for suffix in [
        "BepInEx/plugins",
        "bepinex/PLUGINS/OC2ModManager",
        "BepInEx/plugins/one/two",
        "BepInEx/plugins/./OC2ModManager",
    ] {
        assert!(
            validate_install_dir(&root.join(suffix)).is_err(),
            "{suffix}"
        );
    }
    assert!(validate_install_dir(Path::new("")).is_err());
    assert!(validate_install_dir(Path::new("relative/path")).is_err());
}

#[test]
fn allows_normal_locations_and_similarly_named_directories() {
    let root = std::env::temp_dir();
    for suffix in [
        "OC2ModManager",
        // 相似但不相同的名字不算 BepInEx 目录，仍允许
        "BepInExManager",
        "OtherBepInEx/Manager",
        "plugins/Manager",
    ] {
        assert!(validate_install_dir(&root.join(suffix)).is_ok(), "{suffix}");
    }
}

/// 只要路径里出现 `BepInEx` 目录就拒绝：框架备份/恢复按整树快照，
/// 管理器在里面会被自己的恢复流程覆盖。
#[test]
fn rejects_anything_inside_bepinex() {
    let root = std::env::temp_dir();
    for suffix in [
        "game/BepInEx",
        "game/BepInEx/plugins",
        "game/BepInEx/plugins/sub",
        "game/BepInEx/config",
        "game/BepInEx/patchers",
    ] {
        assert!(
            validate_install_dir(&root.join(suffix)).is_err(),
            "{suffix} 应被拒绝"
        );
    }
}

#[cfg(windows)]
#[test]
fn rejects_existing_junction_with_missing_destination_suffix() {
    let root = std::env::temp_dir().join(format!("oc2-install-guard-{}", uuid::Uuid::new_v4()));
    let plugins = root.join("game/BepInEx/plugins");
    let alias = root.join("alias");
    fs::create_dir_all(&plugins).unwrap();
    let junction = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&alias)
        .arg(plugins.to_string_lossy().replace('/', "\\"))
        .output()
        .unwrap();
    assert!(
        junction.status.success(),
        "{}",
        String::from_utf8_lossy(&junction.stderr)
    );
    assert!(validate_install_dir(&alias.join("new/Manager")).is_err());
    fs::remove_dir(&alias).unwrap();
    fs::remove_dir_all(&root).unwrap();
}
