use super::*;

#[test]
fn package_unit_root_follows_the_levels_directory() {
    // 实测形状：<包名>/levels包/<集名>
    assert_eq!(
        package_unit_root("jonny职业鼠鼠的图/levels包/jonnylevelset3.4.1"),
        "jonny职业鼠鼠的图"
    );
    // 英文 levels + 大小写
    assert_eq!(package_unit_root("young_0.5/Levels/young_0.5"), "young_0.5");
    // 没有 levels 层：单位根退回集目录的父级
    assert_eq!(package_unit_root("pack/set"), "pack");
    // ZIP 根下直接是集目录
    assert_eq!(package_unit_root("set"), "");
}

#[test]
fn plugin_unit_root_only_accepts_dlls_under_plugins() {
    assert_eq!(
        plugin_unit_root("SHZ_Hogwaffles_v0.4/plugins包/SHZ_Hogwaffles_dll/SHZTargetDrop.dll"),
        Some("SHZ_Hogwaffles_v0.4".to_string())
    );
    // 没有 plugins 层：归属无从判断，宁可忽略也不猜
    assert_eq!(plugin_unit_root("pack/loose.dll"), None);
    // 第二级（猜归属）不认 levels 下的 DLL：那种由第一级「与 info 同父目录」
    // 直接绑定，不靠这里猜
    assert_eq!(plugin_unit_root("pack/levels/set/x.dll"), None);
}

#[test]
fn picks_the_level_set_a_plugin_belongs_to() {
    let sets = vec![
        (
            "unit/levels/alpha".to_string(),
            metadata("Alpha", None),
            "alpha".to_string(),
        ),
        (
            "unit/levels/beta".to_string(),
            metadata("Beta", None),
            "beta".to_string(),
        ),
    ];
    let member = [0usize, 1usize];
    // 父目录名与集名共享前缀 → 归集名匹配的那张
    assert_eq!(
        pick_level_set_for_plugin("unit/plugins/BetaFix/BetaPatch.dll", &sets, &member),
        1
    );
    assert_eq!(
        pick_level_set_for_plugin("unit/plugins/AlphaFix/AlphaPatch.dll", &sets, &member),
        0
    );
    // 名字完全无关：归排序第一张（不复制给多张，重复落盘会被加载两次）
    assert_eq!(
        pick_level_set_for_plugin("unit/plugins/xyz/zzz.dll", &sets, &member),
        0
    );
    // 单集包直接归它
    assert_eq!(
        pick_level_set_for_plugin("unit/plugins/xyz/zzz.dll", &sets, &[1usize]),
        1
    );
}

/// 端到端：`levels + plugins` 包里，插件要跟着地图落进同一个集目录。
#[test]
fn assigns_plugins_from_the_same_package_unit() {
    let Some(info) = real_info_bytes() else {
        eprintln!("跳过：没有 .tmp/rw.zip 可取真实 info");
        return;
    };
    let root = std::env::temp_dir().join(format!("oc2-lv-plugin-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let zip = root.join("pack.zip");
    write_test_zip(
        &zip,
        &[
            ("作者包/levels包/myset/info_x", &info),
            ("作者包/levels包/myset/s_x_1", b"scene"),
            ("作者包/plugins包/fix/First.dll", b"dll"),
            ("作者包/plugins包/fix/Second.dll", b"dll"),
        ],
    );

    let sets = inspect_zip(&zip).expect("应能识别这个包");
    assert_eq!(sets.len(), 1, "一个 info 就是一个关卡集");
    let set = &sets[0];
    let plugin_names: Vec<&str> = set
        .plugins
        .iter()
        .map(|plugin| plugin.file_name.as_str())
        .collect();
    assert_eq!(
        plugin_names,
        vec!["First.dll", "Second.dll"],
        "两个插件都应归到这个集（按文件名排序）"
    );
    // 假 DLL 读不出 BepInPlugin：降级成依赖 + 文件名兜底，识别本身不能失败
    for plugin in &set.plugins {
        assert_eq!(plugin.info.role, DllRole::Dependency);
        assert_eq!(
            plugin.info.name.as_deref(),
            plugin.file_name.strip_suffix(".dll"),
            "读不出元数据时展示名退回文件名（去扩展名）"
        );
        assert!(
            plugin.path.starts_with("作者包/plugins包/fix/"),
            "path 保留 ZIP 内条目路径：{}",
            plugin.path
        );
    }
    // 落盘相对路径是纯文件名 → 写进集目录根，随目录一起开关
    for name in ["First.dll", "Second.dll"] {
        assert!(
            set.files
                .iter()
                .any(|(entry, relative)| entry.ends_with(name) && relative == name),
            "{name} 应作为集目录根下的文件被复制"
        );
    }

    std::fs::remove_dir_all(&root).unwrap();
}
