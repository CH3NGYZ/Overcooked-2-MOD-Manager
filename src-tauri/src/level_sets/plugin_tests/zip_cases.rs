use super::*;

#[test]
fn classifies_map_packages_mod_packages_and_junk() {
    let root = std::env::temp_dir().join(format!("oc2-lv-kind-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();

    // 地图包：含 info，即使里面还有 DLL 也算地图包
    let map_zip = root.join("map.zip");
    write_test_zip(
        &map_zip,
        &[
            ("pack/levels包/set/info_x", b"info"),
            ("pack/levels包/set/s_1", b"s"),
            ("pack/plugins包/fix/Fix.dll", b"dll"),
        ],
    );
    assert_eq!(
        classify_zip(&map_zip).unwrap(),
        ZipContents {
            map_sets: 1,
            plugins: 1
        }
    );

    // MOD 包：只有 DLL，没有 info
    let mod_zip = root.join("mod.zip");
    write_test_zip(
        &mod_zip,
        &[("Plugins/MyMod.dll", b"dll"), ("readme.txt", b"r")],
    );
    assert_eq!(
        classify_zip(&mod_zip).unwrap(),
        ZipContents {
            map_sets: 0,
            plugins: 1
        }
    );

    // 什么都没有：既不是地图也不是 MOD
    let junk_zip = root.join("junk.zip");
    write_test_zip(&junk_zip, &[("readme.txt", b"r")]);
    assert_eq!(
        classify_zip(&junk_zip).unwrap(),
        ZipContents {
            map_sets: 0,
            plugins: 0
        }
    );

    // macOS 影子条目不算地图集，也不算插件
    let mac_zip = root.join("mac.zip");
    write_test_zip(
        &mac_zip,
        &[
            ("__MACOSX/pack/._info", b"junk"),
            ("__MACOSX/pack/._x.dll", b"junk"),
        ],
    );
    assert_eq!(
        classify_zip(&mac_zip).unwrap(),
        ZipContents {
            map_sets: 0,
            plugins: 0
        }
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 第一级绑定：DLL 与 info 同处一个父目录 → 名字毫不相干也绑这张图。
///
/// 这是作者把「地图 + 它自己的修复 DLL」平铺进同一个集目录时的形态，
/// 也是管理器导出地图包时的落点，所以必须是确定绑定而不是猜名字。
#[test]
fn dll_beside_info_binds_to_that_level_set_without_name_matching() {
    let Some(info) = real_info_bytes() else {
        eprintln!("跳过：没有 .tmp/rw.zip 可取真实 info");
        return;
    };
    let root = std::env::temp_dir().join(format!("oc2-lv-plugin-sibling-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let zip = root.join("pack.zip");
    write_test_zip(
        &zip,
        &[
            ("作者包/levels包/myset/info_x", &info),
            ("作者包/levels包/myset/s_x_1", b"scene"),
            // 名字与集名毫无关系：只靠「同父目录」绑定
            ("作者包/levels包/myset/ZZZ_Unrelated.dll", b"dll"),
        ],
    );

    let sets = inspect_zip(&zip).expect("应能识别这个包");
    assert_eq!(sets.len(), 1, "一个 info 就是一个关卡集");
    let set = &sets[0];
    assert_eq!(
        set.plugins
            .iter()
            .map(|plugin| plugin.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["ZZZ_Unrelated.dll"],
        "同目录的 DLL 应当绑到这个集"
    );
    // 它本来就在集目录里，`files` 已经收过；再当插件 push 一次会让安装
    // 事务重复写同一个文件、`file_count` 也跟着虚高
    let copies = set
        .files
        .iter()
        .filter(|(source, _)| source.ends_with("ZZZ_Unrelated.dll"))
        .count();
    assert_eq!(copies, 1, "同一个文件只能出现在 files 里一次");

    std::fs::remove_dir_all(&root).unwrap();
}

/// 随包插件要带出 BepInPlugin 元数据：前端把它当一个 MOD 展示安装详情。
#[test]
fn bundled_plugin_carries_its_bepin_plugin_metadata() {
    let Some(info) = real_info_bytes() else {
        eprintln!("跳过：没有 .tmp/rw.zip 可取真实 info");
        return;
    };
    let root = std::env::temp_dir().join(format!("oc2-lv-plugin-meta-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let zip = root.join("pack.zip");
    let dll = crate::mods::plugin_blob(
        "com.example.jonnylevelloader",
        "Jonny Level Loader",
        "3.4.1",
    );
    write_test_zip(
        &zip,
        &[
            ("作者包/levels包/myset/info_x", &info),
            ("作者包/plugins包/fix/JonnyLevelLoaderPlugin.dll", &dll),
        ],
    );

    let sets = inspect_zip(&zip).expect("应能识别这个包");
    assert_eq!(sets.len(), 1);
    let plugin = &sets[0].plugins[0];
    assert_eq!(
        plugin.path, "作者包/plugins包/fix/JonnyLevelLoaderPlugin.dll",
        "path 是 ZIP 内条目路径，file_name 才是落盘文件名"
    );
    assert_eq!(plugin.file_name, "JonnyLevelLoaderPlugin.dll");
    assert_eq!(plugin.info.role, DllRole::Mod, "读出 BepInPlugin 就是 mod");
    assert_eq!(plugin.info.name.as_deref(), Some("Jonny Level Loader"));
    assert_eq!(
        plugin.info.guid.as_deref(),
        Some("com.example.jonnylevelloader")
    );
    assert_eq!(plugin.info.version.as_deref(), Some("3.4.1"));

    std::fs::remove_dir_all(&root).unwrap();
}

/// 真实包验收：作者随包的修复 DLL 必须归到对应的地图集。
///
/// `OC2_LEVEL_SET_PLUGIN_ZIPS="a.zip;b.zip" cargo test --lib real_packages -- --ignored --nocapture`
#[test]
#[ignore = "需要 OC2_LEVEL_SET_PLUGIN_ZIPS 指向真实的地图+插件包"]
fn real_packages_carry_their_own_plugin_dlls() {
    let Some(raw) = std::env::var_os("OC2_LEVEL_SET_PLUGIN_ZIPS") else {
        eprintln!("跳过：没有 OC2_LEVEL_SET_PLUGIN_ZIPS");
        return;
    };
    let raw = raw.to_string_lossy().to_string();
    for path in raw.split(';').filter(|part| !part.trim().is_empty()) {
        let path = PathBuf::from(path.trim());
        let contents = classify_zip(&path).expect("应能读条目");
        assert!(contents.map_sets > 0, "真实包应含 info：{}", path.display());
        assert!(
            contents.plugins > 0,
            "真实包应带修复 DLL：{}",
            path.display()
        );
        let sets = inspect_zip(&path).expect("应能识别");
        let total: usize = sets.iter().map(|set| set.plugins.len()).sum();
        eprintln!("包={} 关卡集={} 插件={}", path.display(), sets.len(), total);
        for set in &sets {
            eprintln!(
                "  集={} 插件={:?} 文件={}",
                set.dir_name,
                set.plugins
                    .iter()
                    .map(|plugin| plugin.file_name.as_str())
                    .collect::<Vec<_>>(),
                set.files.len()
            );
        }
        assert_eq!(total, contents.plugins, "每个 DLL 都应恰好归属一次");
    }
}
