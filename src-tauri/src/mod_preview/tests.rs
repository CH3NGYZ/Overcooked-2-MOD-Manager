use super::analysis::{apply_repository_origin, build_preview};
use super::commands::safe_preview_id;
use super::conflicts::{find_install_conflicts, same_version};
use super::model::{ModPreviewEntry, PreviewOrigin};
use super::staging::PreviewOriginRecord;
use crate::models::{DllRole, InstalledMod};
use uuid::Uuid;

fn entry(role: DllRole) -> ModPreviewEntry {
    ModPreviewEntry {
        path: "x.dll".into(),
        role,
        name: Some("x".into()),
        guid: None,
        version: None,
        title: None,
        product_name: None,
        company: None,
        copyright: None,
        original_filename: None,
    }
}

#[test]
fn single_dll_is_a_single_mod() {
    let preview = build_preview(vec![entry(DllRole::Mod)], "a.zip");
    assert_eq!(preview.kind, "single");
    assert_eq!(preview.total_dlls, 1);
    assert_eq!(preview.mod_count, 1);
    assert_eq!(preview.dependency_count, 0);
    assert_eq!(preview.package_name, "a");
}

#[test]
fn repository_origin_replaces_file_name_with_repository_name() {
    // 资产文件名是技术命名，用户认的是仓库名
    let mut preview = build_preview(
        vec![entry(DllRole::Mod)],
        "IMGUI.HiDPI.Patcher.Mono-v0.4.4.zip",
    );
    assert_eq!(preview.package_name, "IMGUI.HiDPI.Patcher.Mono-v0.4.4");

    apply_repository_origin(
        &mut preview,
        "BepInEx 配置管理器 - HiDPI 缩放修复",
        "v0.4.4",
        "## Changes",
    );
    assert_eq!(preview.origin, PreviewOrigin::Repository);
    assert_eq!(preview.package_name, "BepInEx 配置管理器 - HiDPI 缩放修复");
    assert_eq!(preview.release_tag, "v0.4.4");
    assert_eq!(preview.release_body, "## Changes");
}

#[test]
fn empty_repository_name_keeps_filename_as_title() {
    // 仓库名为空时不能把标题清空，否则界面只剩一个空白标题
    let mut preview = build_preview(vec![entry(DllRole::Mod)], "fallback.zip");
    apply_repository_origin(&mut preview, "   ", "", "");
    assert_eq!(preview.package_name, "fallback");
}

#[test]
fn manual_origin_keeps_zip_name_as_title() {
    // 手动安装（本地文件 / 直链）不经过 apply_repository_origin，标题就是文件名
    let preview = build_preview(vec![entry(DllRole::Mod)], "MyLocalModPack-v1.2.zip");
    assert_eq!(preview.origin, PreviewOrigin::Manual);
    assert_eq!(preview.package_name, "MyLocalModPack-v1.2");
    assert!(preview.release_tag.is_empty());
}

#[test]
fn one_plugin_plus_one_dependency_is_still_a_package() {
    // DIYLevel 那种情况：真正的插件 + 纯依赖，必须按包处理，
    // 「能否解析出 BepInPlugin」不决定它是不是包。
    let preview = build_preview(
        vec![entry(DllRole::Mod), entry(DllRole::Dependency)],
        "DIYLevel.zip",
    );
    assert_eq!(preview.kind, "package");
    assert_eq!(preview.total_dlls, 2);
    assert_eq!(preview.mod_count, 1);
    assert_eq!(preview.dependency_count, 1);
}

#[test]
fn multiple_plugins_are_a_package() {
    let preview = build_preview(vec![entry(DllRole::Mod), entry(DllRole::Mod)], "pack.zip");
    assert_eq!(preview.kind, "package");
    assert_eq!(preview.mod_count, 2);
    assert_eq!(preview.dependency_count, 0);
}

#[test]
fn zero_dlls_is_a_package_without_mods() {
    let preview = build_preview(Vec::new(), "assets.zip");
    assert_eq!(preview.kind, "package");
    assert_eq!(preview.total_dlls, 0);
    assert_eq!(preview.mod_count, 0);
    assert_eq!(preview.package_name, "assets");
}

#[test]
fn preview_id_must_be_a_uuid() {
    assert!(safe_preview_id("../escape").is_err());
    assert!(safe_preview_id("not-a-uuid").is_err());
    let id = Uuid::new_v4().to_string();
    assert_eq!(safe_preview_id(&id).unwrap(), id);
}

#[test]
fn origin_record_round_trips_repository_identity() {
    // 确认安装时要靠这份记录回到仓库安装路径：
    // 丢了它就只能走 install_manual_mod，同仓库旧包永远不被回收。
    let record = PreviewOriginRecord {
        repository_id: "repo-1".into(),
        release_tag: "v1.8.7.23".into(),
        prerelease: true,
    };
    let text = serde_json::to_string(&record).unwrap();
    let parsed: PreviewOriginRecord = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed.repository_id, "repo-1");
    assert_eq!(parsed.release_tag, "v1.8.7.23");
    assert!(parsed.prerelease);
}

#[test]
fn origin_record_tolerates_missing_optional_fields() {
    // 只带 repository_id 的最简记录也要能读：老预览或手工构造的情况
    let parsed: PreviewOriginRecord =
        serde_json::from_str(r#"{"repository_id":"repo-2"}"#).unwrap();
    assert_eq!(parsed.repository_id, "repo-2");
    assert!(parsed.release_tag.is_empty());
    assert!(!parsed.prerelease);
}

/// 本地已装 MOD 的最小构造：默认字段与真实扫描一致，测试只写关心的字段。
fn installed_mod(dll_file: &str, extra: serde_json::Value) -> InstalledMod {
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

fn preview_entry(
    path: &str,
    role: DllRole,
    name: Option<&str>,
    guid: Option<&str>,
    version: Option<&str>,
) -> ModPreviewEntry {
    ModPreviewEntry {
        path: path.into(),
        role,
        name: name.map(str::to_string),
        guid: guid.map(str::to_string),
        version: version.map(str::to_string),
        title: None,
        product_name: None,
        company: None,
        copyright: None,
        original_filename: None,
    }
}

#[test]
fn conflicts_match_by_guid_ignoring_case_and_whitespace() {
    // 同一个 GUID 就是同一个插件：名字被改过也算冲突，卸载句柄用本地那份。
    let installed = vec![installed_mod(
        "BepInEx/plugins/FastThrow.dll",
        serde_json::json!({
            "guid": "com.example.fastthrow", "version": "1.2.0",
            "package_id": "repo-fast-throw", "package_name": "快速投掷修复包",
        }),
    )];
    let entries = vec![preview_entry(
        "FastThrow.dll",
        DllRole::Mod,
        Some("另一个显示名"),
        Some("  COM.Example.FastThrow "),
        Some("1.2.0"),
    )];

    let conflicts = find_install_conflicts(&installed, &entries);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].name, "快速投掷修复包");
    assert_eq!(conflicts[0].version, "1.2.0");
    assert_eq!(conflicts[0].handle, "BepInEx/plugins/FastThrow.dll");
    assert!(conflicts[0].enabled);
    assert!(conflicts[0].same_version);
}

#[test]
fn conflicts_fall_back_to_name_when_either_side_lacks_a_guid() {
    // 读不出 BepInPlugin 的 DLL 只有名字可比；名字两边都空时不能算同一份。
    let installed = vec![installed_mod(
        "BepInEx/plugins/Plain.dll",
        serde_json::json!({ "name": "PlainMod" }),
    )];
    assert_eq!(
        find_install_conflicts(
            &installed,
            &[preview_entry(
                "Plain.dll",
                DllRole::Mod,
                Some(" plainmod "),
                Some("com.example.plain"),
                None,
            )],
        )
        .len(),
        1
    );
    assert!(
        find_install_conflicts(
            &installed,
            &[preview_entry("Plain.dll", DllRole::Mod, None, None, None)],
        )
        .is_empty()
    );
}

#[test]
fn differing_guids_are_not_conflicts_even_with_the_same_name() {
    // 不同作者可能起同名 DLL，但不会共用 GUID：GUID 都在时以 GUID 为准。
    let installed = vec![installed_mod(
        "BepInEx/plugins/Shared.dll",
        serde_json::json!({ "guid": "com.author-a.shared", "name": "Shared" }),
    )];
    let entries = vec![preview_entry(
        "Shared.dll",
        DllRole::Mod,
        Some("Shared"),
        Some("com.author-b.shared"),
        None,
    )];
    assert!(find_install_conflicts(&installed, &entries).is_empty());
}

#[test]
fn dependency_entries_never_count_as_conflicts() {
    // 依赖是随包带的 DLL，装成依赖不会顶掉本地同一个插件，不该提示卸载。
    let installed = vec![installed_mod(
        "BepInEx/plugins/Runtime.dll",
        serde_json::json!({ "guid": "com.example.runtime", "version": "2.0.0" }),
    )];
    let entries = vec![preview_entry(
        "Runtime.dll",
        DllRole::Dependency,
        Some("Runtime"),
        Some("com.example.runtime"),
        Some("2.0.0"),
    )];
    assert!(find_install_conflicts(&installed, &entries).is_empty());
}

#[test]
fn conflicts_are_deduplicated_per_installed_package() {
    // 同一个包里的两个 DLL 属于同一次卸载：只报一条，句柄取第一份命中的。
    let installed = vec![
        installed_mod(
            "BepInEx/plugins/Pack/Main.dll",
            serde_json::json!({
                "name": "Main", "guid": "com.example.pack.main", "version": "1.0.0",
                "package_id": "repo-pack", "package_name": "测试包",
            }),
        ),
        installed_mod(
            "BepInEx/plugins/Pack/Extra.dll",
            serde_json::json!({
                "name": "Extra", "guid": "com.example.pack.extra", "version": "1.0.0",
                "package_id": "repo-pack", "package_name": "测试包",
            }),
        ),
    ];
    let entries = vec![
        preview_entry(
            "Main.dll",
            DllRole::Mod,
            Some("Main"),
            Some("com.example.pack.main"),
            Some("1.0.0"),
        ),
        preview_entry(
            "Extra.dll",
            DllRole::Mod,
            Some("Extra"),
            Some("com.example.pack.extra"),
            Some("1.0.0"),
        ),
    ];

    let conflicts = find_install_conflicts(&installed, &entries);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].handle, "BepInEx/plugins/Pack/Main.dll");
}

#[test]
fn conflicts_keep_installed_order_and_report_disabled_copies() {
    // 已有重复副本要全部列出（关着的也要卸），顺序沿用本地扫描顺序。
    let installed = vec![
        installed_mod(
            "BepInEx/plugins/A.dll",
            serde_json::json!({ "name": "A", "guid": "com.example.a", "version": "1.0" }),
        ),
        installed_mod(
            "BepInEx/plugins/A-disabled.dll.disabled",
            serde_json::json!({
                "name": "A", "guid": "com.example.a", "version": "1.0", "enabled": false,
            }),
        ),
    ];
    let entries = vec![preview_entry(
        "A.dll",
        DllRole::Mod,
        Some("A"),
        Some("com.example.a"),
        Some("1.0"),
    )];

    let conflicts = find_install_conflicts(&installed, &entries);
    assert_eq!(conflicts.len(), 2);
    assert_eq!(conflicts[0].handle, "BepInEx/plugins/A.dll");
    assert!(conflicts[0].enabled);
    assert_eq!(
        conflicts[1].handle,
        "BepInEx/plugins/A-disabled.dll.disabled"
    );
    assert!(!conflicts[1].enabled);
}

#[test]
fn same_version_follows_bepinex_numeric_comparison() {
    // 「重装」还是「版本变化」必须与本地谁的版本更新判定同一把尺子：
    // 1.0 与 1.0.0 是同一个版本，1.9.8 与 1.9.8.1 不是。
    assert!(same_version(Some("1.0"), Some("1.0.0")));
    assert!(!same_version(Some("1.9.8"), Some("1.9.8.1")));
    assert!(same_version(Some(" v1.2.3 "), Some("1.2.3")));
    // 不是纯数字版本号：退回原文比较
    assert!(same_version(Some("final"), Some("final")));
    assert!(!same_version(Some("final"), Some("latest")));
    // 两边都读不出时算相同，只有一边读不出时不能声称同版本
    assert!(same_version(None, None));
    assert!(!same_version(Some("1.0"), None));
}
