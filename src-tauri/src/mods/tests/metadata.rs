use super::*;

#[test]
fn local_version_only_uses_bepinplugin_and_preserves_zero_versions() {
    for version in ["0.10.0", "1.2.3.4", "0.0.0", "0.0.0.0"] {
        assert_eq!(
            super::choose_local_version(Some(version)),
            (Some(version.into()), "BepInPlugin")
        );
    }
    assert_eq!(
        super::choose_local_version(Some(" 0.10.0 ")),
        (Some("0.10.0".into()), "BepInPlugin")
    );
    for missing in [None, Some(""), Some(" \t\r\n")] {
        assert_eq!(super::choose_local_version(missing), (None, "unknown"));
    }
}

#[test]
fn bepin_plugin_metadata_provides_local_version() {
    assert_eq!(
        super::parse_bepin_plugin_metadata(&plugin_blob(
            "dev.gua.overcooked.diylevel",
            "Overcooked2 DIYLevel Plugin",
            "0.10.0"
        ))
        .and_then(|item| item.version),
        Some("0.10.0".into())
    );
    assert_eq!(
        super::parse_bepin_plugin_metadata(&plugin_blob(
            "com.example.mod",
            "HostUtilities",
            "1.2.3"
        ))
        .and_then(|item| item.version),
        Some("1.2.3".into())
    );
    assert_eq!(
        super::parse_bepin_plugin_metadata(&plugin_blob(
            "{12345678-1234-1234-1234-123456789abc}",
            "HostUtilities",
            "1.2.3.4"
        ))
        .and_then(|item| item.guid),
        Some("12345678-1234-1234-1234-123456789abc".into())
    );
    assert_eq!(
        super::parse_bepin_plugin_metadata(&plugin_blob("HostUtilities", "HostUtilities", "1.2.3"))
            .and_then(|item| item.guid),
        Some("HostUtilities".into())
    );
    assert_eq!(
        super::parse_bepin_plugin_metadata(&plugin_blob(
            "com.example.mod",
            "HostUtilities",
            "0.0.0"
        ))
        .and_then(|item| item.version),
        Some("0.0.0".into())
    );
}

#[test]
fn dll_details_keep_role_and_name_fallbacks_in_one_place() {
    let root = std::env::temp_dir().join(format!("oc2-dll-details-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();

    // 有 BepInPlugin：角色是 mod，名称/版本/GUID 都取自插件元数据
    let plugin_dll = root.join("Fancy.Plugin.dll");
    std::fs::write(
        &plugin_dll,
        plugin_blob("com.example.fancy", "Fancy Plugin", "2.3.4"),
    )
    .unwrap();
    let details = read_dll_details(&plugin_dll, "Fancy.Plugin.dll");
    assert_eq!(details.role, DllRole::Mod);
    assert_eq!(details.name.as_deref(), Some("Fancy Plugin"));
    assert_eq!(details.guid.as_deref(), Some("com.example.fancy"));
    assert_eq!(details.version.as_deref(), Some("2.3.4"));

    // 读不出 BepInPlugin：角色是依赖，展示名退回文件名（去扩展名）
    let plain_dll = root.join("LevelEditorStub.dll");
    std::fs::write(&plain_dll, b"not a PE").unwrap();
    let details = read_dll_details(&plain_dll, "LevelEditorStub.dll");
    assert_eq!(details.role, DllRole::Dependency);
    assert_eq!(details.name.as_deref(), Some("LevelEditorStub"));
    assert!(details.guid.is_none() && details.version.is_none());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn file_stem_name_strips_extension_as_install_display_fallback() {
    // 本地安装提示的兜底名：文件名去扩展名，去掉 .dll 后缀
    assert_eq!(super::file_stem_name("FastThrow.dll"), "FastThrow");
    assert_eq!(super::file_stem_name("MyMod.zip"), "MyMod");
    assert_eq!(super::file_stem_name("no-extension"), "no-extension");
    // 多段文件名只去掉最后一段扩展名
    assert_eq!(super::file_stem_name("a.b.dll"), "a.b");
}
