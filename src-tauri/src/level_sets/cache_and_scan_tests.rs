use crate::asset_bundle::LevelSetMetadata;
use crate::level_sets::info_cache::clear_info_disk_cache;
use crate::level_sets::info_cache::content_sha256;
use crate::level_sets::info_cache::read_info_disk_cache;
use crate::level_sets::info_cache::write_info_disk_cache;
use crate::level_sets::paths::disabled_root;
use crate::level_sets::paths::levels_root;
use crate::level_sets::scan::scan_level_sets_with_progress;
use std::path::PathBuf;
use std::sync::Mutex;

/// 磁盘缓存往返：同一份内容写进去再读出来必须一模一样。
///
/// 这条覆盖的是「跨进程命中」的核心机制 —— 进程内缓存用 (len, mtime)，
/// 重启就没了；磁盘这份用内容 SHA-256，重启后仍然命中，这正是用户要的
/// 「文件没变就直接读缓存」。哈希算法选 SHA-256 的依据见 `content_sha256`。
#[test]
fn info_disk_cache_round_trips_by_content_hash() {
    let dir = std::env::temp_dir().join(format!(
        "oc2-info-disk-cache-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let metadata = LevelSetMetadata {
        name: "RoundTrip".into(),
        name_zh: Some("往返测试".into()),
        author: "Lead".into(),
        version: "1.0".into(),
        uid: "uid-1".into(),
        base_uid: Some("base-1".into()),
        levels: vec![crate::asset_bundle::LevelMetadata {
            name: "L1".into(),
            name_zh: None,
            scene_name: "Scene".into(),
            min_order_count: 1,
            max_order_count: 4,
        }],
    };
    let payload = Result::<LevelSetMetadata, String>::Ok(metadata.clone());
    let sha = content_sha256(b"payload");

    assert!(
        read_info_disk_cache(&dir, &sha).is_none(),
        "空目录必须未命中"
    );
    write_info_disk_cache(&dir, &sha, &payload);
    assert_eq!(
        read_info_disk_cache(&dir, &sha),
        Some(payload),
        "写入后应能读回同一份结果"
    );

    // 哈希对不上（文件被改过）时不认
    assert!(read_info_disk_cache(&dir, "deadbeef").is_none());

    // 失败结果同样缓存：坏 info 不该每次扫描都重付解析代价
    let failure = Result::<LevelSetMetadata, String>::Err("assetBundle.notABundle".into());
    write_info_disk_cache(&dir, &sha, &failure);
    assert_eq!(read_info_disk_cache(&dir, &sha), Some(failure));

    let _ = std::fs::remove_dir_all(&dir);
}

/// 清缓存只清本模块的文件，别把 `cache/` 里邻居的缓存一起删了。
///
/// 这条同时钉住返回值语义：前端拿它显示「已清除 N 份」，数字必须等于
/// 真正删掉的缓存份数（`*.json` 与写一半的 `*.json.tmp`），不含目录里的
/// 其它文件。
#[test]
fn clear_info_disk_cache_removes_only_our_files() {
    let dir = std::env::temp_dir().join(format!(
        "oc2-info-clear-cache-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let payload = Result::<LevelSetMetadata, String>::Err("assetBundle.notABundle".into());
    let first = content_sha256(b"first");
    let second = content_sha256(b"second");
    write_info_disk_cache(&dir, &first, &payload);
    write_info_disk_cache(&dir, &second, &payload);
    // 模拟进程被杀在写一半时留下的临时文件
    std::fs::write(dir.join("halfwritten.json.tmp"), b"{").unwrap();
    // 邻居模块的缓存：不能被顺手删掉
    std::fs::write(dir.join("keep-me.bin"), b"neighbour").unwrap();

    let removed = clear_info_disk_cache(&dir).unwrap();
    assert_eq!(removed, 3, "应删掉两份缓存与一个写一半的临时文件");
    assert!(
        read_info_disk_cache(&dir, &first).is_none(),
        "清完之后必须未命中"
    );
    assert!(read_info_disk_cache(&dir, &second).is_none());
    assert!(
        dir.join("keep-me.bin").is_file(),
        "目录里其它模块的文件不能删"
    );

    // 目录已空追删不是错误，返回 0
    assert_eq!(clear_info_disk_cache(&dir).unwrap(), 0);
    // 目录不存在同样返回 0，不报错
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(clear_info_disk_cache(&dir).unwrap(), 0);
}

/// 并行扫描必须保持串行时的顺序，进度必须是「真实完成数」。
///
/// 真实目录里各份 `info` 大小差几十倍（0.3 MB 与 65 MB 并存），谁先算完
/// 完全不确定 —— 结果按原下标归位这件事只能靠断言钉住。这里不往目录里放
/// `info`：解析失败的集照样要出现在列表里（卡片仍能按目录名开关），顺带
/// 覆盖「坏集不阻塞扫描」。
#[test]
fn parallel_scan_keeps_order_and_reports_progress() {
    let root = std::env::temp_dir().join(format!(
        "oc2-level-scan-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    // 刻意乱序创建：目录项的读取顺序不代表扫描输出的顺序
    let plan: [(PathBuf, &[&str]); 2] = [
        (
            levels_root(&crate::level_sets::paths::test_roots(&root)).unwrap(),
            &["zeta", "alpha", "mike"],
        ),
        (disabled_root(&root), &["beta", "yankee"]),
    ];
    for (side, names) in plan {
        for &name in names {
            std::fs::create_dir_all(side.join(name)).unwrap();
        }
    }

    let seen: Mutex<Vec<(usize, usize, String)>> = Mutex::new(Vec::new());
    let sets = scan_level_sets_with_progress(
        &crate::level_sets::paths::test_roots(&root),
        None,
        crate::concurrency::cores(),
        &|done, total, current| {
            seen.lock()
                .unwrap()
                .push((done, total, current.to_string()));
        },
    );

    let order: Vec<&str> = sets.iter().map(|set| set.dir_name.as_str()).collect();
    assert_eq!(
        order,
        vec!["alpha", "mike", "zeta", "beta", "yankee"],
        "并行不能打乱「先启用根再 cache 停用根、各自按名排序」"
    );
    assert!(
        sets.iter()
            .all(|set| set.error.as_deref() == Some("levelSet.infoMissing")),
        "没有 info 的集要保留在列表里并带上原因"
    );

    let progress = seen.lock().unwrap();
    assert_eq!(
        progress
            .first()
            .map(|(done, total, current)| (*done, *total, current.as_str())),
        Some((0, 5, "")),
        "第一句进度是「还没开始，共 5 份」"
    );
    assert_eq!(
        progress.last().map(|(done, total, _)| (*done, *total)),
        Some((5, 5)),
        "最后一句进度必须等于总数"
    );
    let mut done_values: Vec<usize> = progress.iter().map(|(done, _, _)| *done).collect();
    done_values.sort_unstable();
    assert_eq!(
        done_values,
        vec![0, 1, 2, 3, 4, 5],
        "完成数必须逐份递增，不能跳号也不能重复"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scan_view_preserves_bilingual_level_names_and_defaults_old_payloads() {
    let levels = vec![crate::asset_bundle::LevelMetadata {
        name: "BigBurger".into(),
        name_zh: Some("巨无霸汉堡".into()),
        ..Default::default()
    }];
    let info = crate::level_sets::view::level_set_to_info(crate::level_sets::LevelSetOnDisk {
        dir_name: "test".into(),
        enabled: true,
        path: PathBuf::from("test"),
        metadata: Some(LevelSetMetadata {
            levels: levels.clone(),
            ..Default::default()
        }),
        error: None,
        required_runtime: None,
        plugins: vec![],
    });
    assert_eq!(info.levels, levels);
    assert_eq!(info.level_count, 1);
    let mut old = serde_json::to_value(&info).unwrap();
    old.as_object_mut().unwrap().remove("levels");
    let restored: crate::models::LevelSetInfo = serde_json::from_value(old).unwrap();
    assert!(restored.levels.is_empty());
}
