use super::*;

/// 扫描侧也要看到集目录里的随包 DLL：装完再打开地图页得能显示出来。
#[test]
fn scans_bundled_plugins_from_the_installed_level_set_directory() {
    let root = std::env::temp_dir().join(format!("oc2-lv-scan-plugins-{}", uuid::Uuid::new_v4()));
    let set_dir = root.join("levels").join("MySet");
    std::fs::create_dir_all(&set_dir).unwrap();
    let dll = crate::mods::plugin_blob("com.example.bundled", "Bundled Fix", "2.1.0");
    std::fs::write(set_dir.join("BundledFix.dll"), &dll).unwrap();
    std::fs::write(set_dir.join("Disabled.dll.disabled"), &dll).unwrap();
    // 非 DLL 不参与
    std::fs::write(set_dir.join("info"), b"not an asset bundle").unwrap();

    // 走与扫描同一条装配路径：列文件 → 一次批次读元数据 → 组装。
    let target = LevelSetTarget::collect(set_dir.clone(), "MySet".to_string(), true);
    let requests: Vec<_> = target
        .plugins
        .iter()
        .map(
            |(path, file_name, _)| crate::metadata_read::MetadataRequest {
                path: path.clone(),
                display_name: file_name.clone(),
                kind: crate::metadata_read::MetadataKind::ModDll,
            },
        )
        .collect();
    let outcomes = crate::metadata_read::read_metadata_batch(&requests, 2, &|_, _| {});
    let plugins: Vec<_> = outcomes.into_iter().map(Some).collect();
    let set = assemble_level_set(target, None, &plugins, Vec::new());
    let listed: Vec<(&str, bool)> = set
        .plugins
        .iter()
        .map(|plugin| (plugin.file_name.as_str(), plugin.enabled))
        .collect();
    assert_eq!(
        listed,
        vec![("BundledFix.dll", true), ("Disabled.dll.disabled", false)],
        "按文件名排序，`.disabled` 记为未启用"
    );
    assert_eq!(set.plugins[0].info.role, DllRole::Mod);
    assert_eq!(set.plugins[0].info.name.as_deref(), Some("Bundled Fix"));
    assert_eq!(
        set.plugins[0].info.guid.as_deref(),
        Some("com.example.bundled")
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 扫描进度按「出卡数量」严格递增，收尾正好停在总数上。
///
/// 回归：补报「没有可读文件」的目标时曾把已完成的目标再报一遍（它们的
/// `pending` 计数同样归零），54 张图收尾显示成 108/54。
#[test]
fn scan_progress_counts_each_level_set_exactly_once() {
    let root = std::env::temp_dir().join(format!("oc2-lv-progress-{}", uuid::Uuid::new_v4()));
    let levels = root.join("BepInEx/plugins/OC2DIYLevel/levels");
    // 空目录与只有 requires.txt 的目录都进不了批次，只能靠补报
    std::fs::create_dir_all(levels.join("Empty")).unwrap();
    std::fs::create_dir_all(levels.join("RequiresOnly")).unwrap();
    std::fs::write(levels.join("RequiresOnly/requires.txt"), "").unwrap();
    // 有 info 就会进批次：内容无效只让解析失败，不影响进度已报一次
    std::fs::create_dir_all(levels.join("WithInfo")).unwrap();
    std::fs::write(levels.join("WithInfo/info"), b"not an asset bundle").unwrap();

    let reports = std::sync::Mutex::new(Vec::<(usize, usize)>::new());
    scan_level_sets_with_progress(
        &crate::level_sets::paths::test_roots(&root),
        None,
        2,
        &|done, total, _| {
            reports.lock().unwrap().push((done, total));
        },
    );

    let reports = reports.into_inner().unwrap();
    assert_eq!(reports.first(), Some(&(0, 3)), "首帧必须从 0/3 开始");
    assert_eq!(
        reports.last(),
        Some(&(3, 3)),
        "收尾必须正好对齐总数，实际={reports:?}"
    );
    assert_eq!(
        reports.len(),
        4,
        "每张图只报一次（含首帧 0/3）：{reports:?}"
    );
    assert!(
        reports
            .windows(2)
            .all(|pair| pair[1].0 > pair[0].0 && pair[1].1 == pair[0].1),
        "进度必须严格递增且总数不变：{reports:?}"
    );

    std::fs::remove_dir_all(&root).unwrap();
}
