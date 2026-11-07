use super::*;

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
