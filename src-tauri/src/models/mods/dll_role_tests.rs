use super::super::levels::ZipLevelSetPlugin;
use super::*;

#[test]
fn dll_role_and_plugin_fields_keep_their_json_names() {
    assert_eq!(serde_json::to_string(&DllRole::Mod).unwrap(), "\"mod\"");
    assert_eq!(
        serde_json::to_string(&DllRole::Dependency).unwrap(),
        "\"dependency\""
    );

    let plugin = ZipLevelSetPlugin {
        path: "pkg/plugins/fix/Fix.dll".into(),
        file_name: "Fix.dll".into(),
        install_path: None,
        info: DllPluginInfo {
            role: DllRole::Dependency,
            name: Some("Fix".into()),
            guid: None,
            version: None,
            title: None,
            product_name: None,
            company: None,
            original_filename: None,
        },
    };
    let json = serde_json::to_value(&plugin).unwrap();
    assert_eq!(json["role"], "dependency");
    assert_eq!(json["path"], "pkg/plugins/fix/Fix.dll");
    assert_eq!(json["file_name"], "Fix.dll");
    assert_eq!(json["name"], "Fix");
    // 读不出的字段直接省略，前端按可选字段处理（不是空串/null）
    assert!(json.get("guid").is_none());
    assert!(json.get("title").is_none());
}
