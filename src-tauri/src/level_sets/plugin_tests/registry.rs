use super::*;
use crate::http_diagnostics::capture_test_logs;
use crate::level_sets::plugin_registry::checked_registered_plugins;

/// 登记表：读写往返、路径归一化、坏数据只丢自己、合并以新归属为准。
#[test]
fn plugin_registry_round_trips_and_normalizes_paths() {
    let (root, _game, _recycle) = workspace("registry");
    let manifests = root.join("manifests");
    assert!(
        read_registered_plugins(&manifests).is_empty(),
        "没有登记表时按空表处理"
    );

    let owner = LevelSetPluginOwner {
        name: "RW_LevelSet".into(),
        name_zh: "七池的地图".into(),
        author: "七池".into(),
    };
    write_registered_plugins(
        &manifests,
        &[RegisteredLevelSetPlugin {
            dll: "BepInEx/plugins/Fix/Fix.dll".into(),
            owner: owner.clone(),
        }],
    )
    .unwrap();

    let read = read_registered_plugins(&manifests);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].dll, "BepInEx/plugins/Fix/Fix.dll");
    assert_eq!(read[0].owner, owner);
    assert_eq!(
        registered_plugin_paths(&manifests),
        vec!["bepinex/plugins/fix/fix.dll".to_string()],
        "MOD 扫描按小写比对"
    );

    // 反斜杠与 `./` 前缀归一化；同一路径大小写不同只留一条
    std::fs::write(
        registry_path(&manifests),
        r#"{"version":1,"entries":[{"dll":".\\BepInEx\\plugins\\Fix\\Fix.dll","owner":{"name":"A"}},{"dll":"BepInEx/plugins/Fix/Fix.dll","owner":{"name":"B"}}]}"#,
    )
    .unwrap();
    let read = read_registered_plugins(&manifests);
    assert_eq!(read.len(), 1, "同一路径大小写不同应去重");
    assert_eq!(read[0].dll, "BepInEx/plugins/Fix/Fix.dll");

    // 不安全路径整条丢弃，且不拖垮整张表
    std::fs::write(
        registry_path(&manifests),
        r#"{"version":1,"entries":[{"dll":"../evil.dll"},{"dll":"BepInEx/plugins/Ok.dll"}]}"#,
    )
    .unwrap();
    let read = read_registered_plugins(&manifests);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].dll, "BepInEx/plugins/Ok.dll");

    // 同一个 DLL 重新登记时以新归属为准
    let merged = merge_registered_plugins(
        vec![RegisteredLevelSetPlugin {
            dll: "BepInEx/plugins/Fix/Fix.dll".into(),
            owner: LevelSetPluginOwner::default(),
        }],
        vec![RegisteredLevelSetPlugin {
            dll: "bepinex/plugins/fix/fix.dll".into(),
            owner: owner.clone(),
        }],
    );
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].owner, owner);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn strict_plugin_registry_rejects_a_disabled_path_claimed_by_another_map() {
    let (root, _game, _recycle) = workspace("strict-registry-disabled-alias");
    let manifests = root.join("manifests");
    let path = registry_path(&manifests);
    let active = "BepInEx/plugins/Fix/Fix.DLL";
    let inactive = format!("{active}.disabled");
    let mut entries = vec![serde_json::json!({"dll": active, "owner": {"name": "A"}})];
    let write = |entries: &[serde_json::Value]| {
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({"version": 1, "entries": entries})).unwrap(),
        )
        .unwrap();
    };
    write(&entries);
    assert_eq!(
        checked_registered_plugins(&manifests, true).unwrap()[0].dll,
        active
    );

    entries.push(serde_json::json!({"dll": inactive, "owner": {"name": "B"}}));
    write(&entries);
    assert_eq!(
        checked_registered_plugins(&manifests, true).unwrap_err(),
        "modChanges.ambiguousOwnership"
    );
    assert_eq!(read_registered_plugins(&manifests).len(), 2);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn strict_plugin_registry_reports_failure_stages_without_untrusted_values() {
    let (root, _game, _recycle) = workspace("strict-registry-log");
    let manifests = root.join("manifests");
    let path = registry_path(&manifests);
    let root_name = root.file_name().unwrap().to_str().unwrap();
    let capture = capture_test_logs("app_lib::level_sets::plugin_registry", root_name);
    assert!(
        checked_registered_plugins(&manifests, true)
            .unwrap()
            .is_empty()
    );

    for record in [
        r#"{"version":1,"entries":[{"dll":"../raw-marker.dll"}]}"#,
        r#"{"version":1,"entries":[{"dll":".\\BepInEx\\raw-marker.dll"},{"dll":"BepInEx/RAW-MARKER.dll"}]}"#,
        r#"{"version":"raw-marker"}"#,
        r#"{"version":2,"entries":[{"dll":"raw-marker.dll"}]}"#,
    ] {
        std::fs::write(&path, record).unwrap();
        assert_eq!(
            checked_registered_plugins(&manifests, true).unwrap_err(),
            "modChanges.ambiguousOwnership"
        );
    }
    std::fs::write(&path, b"\xffraw-marker").unwrap();
    assert!(checked_registered_plugins(&manifests, true).is_err());

    let messages = capture.messages();
    let evidence = [
        "stage=entries reason=unsafe_relative_path",
        "stage=entries reason=duplicate_normalized_path",
        "stage=parse reason=invalid_json",
        "stage=version reason=unsupported_version",
        "stage=read reason=unreadable",
    ];
    assert_eq!(messages.len(), evidence.len(), "{messages:?}");
    for (message, expected) in messages.iter().zip(evidence) {
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("level-set-plugins.json"), "{message}");
        assert!(
            !message.to_ascii_lowercase().contains("raw-marker"),
            "{message}"
        );
    }
    assert!(messages[0].contains("entry=entries[0].dll"));
    assert!(messages[1].contains("entry=entries[1].dll first_entry=entries[0].dll"));
    assert!(messages[2].contains("category=Data line=1 column="));
    assert!(messages[3].contains("version=2 supported_version=1"));
    assert!(messages[4].contains("io_kind=InvalidData"));

    std::fs::remove_dir_all(&root).unwrap();
}
