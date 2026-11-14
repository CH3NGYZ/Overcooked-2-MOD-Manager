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
        "OC2ModManager/.",
    ] {
        assert!(validate_install_dir(&root.join(suffix)).is_ok(), "{suffix}");
    }
}

#[test]
fn installer_distinguishes_a_forbidden_directory_from_an_unavailable_check() {
    let root = std::env::temp_dir();
    let allowed = validate_install_dir(&root.join("OC2ModManager"));
    assert_eq!(allowed.map_or_else(|error| error.exit_code(), |()| 0), 0);
    let forbidden = validate_install_dir(&root.join("game/BepInEx/OC2ModManager")).unwrap_err();
    assert_eq!(forbidden.exit_code(), 1);
    assert!(matches!(forbidden, InstallDirectoryError::Forbidden));
    assert_eq!(forbidden.to_string(), "directory inside BepInEx");

    // Filesystem resolution can fail independently of BepInEx containment.
    // Such a failure must not authorize the installer's legacy-path migration.
    let unavailable =
        validate_install_dir(&root.join("invalid\0directory/OC2ModManager")).unwrap_err();
    assert_eq!(unavailable.exit_code(), 2);
    assert!(matches!(unavailable, InstallDirectoryError::Unavailable(_)));
    assert!(unavailable.to_string().contains("cannot resolve"));
    let tag: String = unavailable.into();
    assert_eq!(
        tag, ERROR,
        "ordinary app callers retain the existing error tag"
    );
}

#[test]
fn invalid_installer_input_is_unavailable_without_claiming_bepinex_containment() {
    for directory in ["", "relative/path", "BepInEx/plugins"] {
        let error = validate_install_dir(Path::new(directory)).unwrap_err();
        assert_eq!(error.exit_code(), 2, "{directory}");
        assert!(matches!(error, InstallDirectoryError::Unavailable(_)));
        assert!(error.to_string().contains("nonempty absolute path"));
        let tag: String = error.into();
        assert_eq!(tag, ERROR, "app callers must still reject invalid input");
    }
}

#[test]
fn installer_messages_cover_all_five_languages_without_showing_app_tags() {
    for language in ["2052", "1028", "1033", "1042", "1041"] {
        let forbidden = error_message(language, &InstallDirectoryError::Forbidden);
        let unavailable = error_message(
            language,
            &InstallDirectoryError::Unavailable("OS error".into()),
        );
        assert!(forbidden.contains("BepInEx"), "{language}");
        assert!(!forbidden.contains("plugins"), "{language}");
        assert!(!forbidden.starts_with("config."), "{language}");
        assert!(!unavailable.contains("BepInEx"), "{language}");
        assert_ne!(forbidden, unavailable, "{language}");
    }
    assert_ne!(
        error_message("1041", &InstallDirectoryError::Forbidden),
        error_message("1033", &InstallDirectoryError::Forbidden),
    );
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
