use super::*;

/// 拍平：嵌套集搬成 `levels/<规范名>/`，info 改名、资源与插件跟着走、
/// 空壳进回收站。
#[test]
fn flattening_moves_nested_level_sets_into_canonical_directories() {
    let (root, game, recycle) = workspace("flatten");
    let manifests = root.join("manifests");
    let levels = levels_root(&crate::level_sets::paths::test_roots(&game)).unwrap();

    let pear = levels.join("pear9.5关卡/levels包/PearSet");
    std::fs::create_dir_all(&pear).unwrap();
    std::fs::write(pear.join("info"), b"PearSet").unwrap();
    std::fs::write(pear.join("shared.assets"), b"assets").unwrap();
    let other = levels.join("pear9.5关卡/levels包/OtherSet");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("info"), b"OtherSet").unwrap();
    let plugin_dir = levels.join("pear9.5关卡/plugins包/PearSetFix");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("PearSetFix.dll"), b"dll").unwrap();
    std::fs::write(plugin_dir.join("PearSetFix.cfg"), b"cfg").unwrap();
    // 顶层已有 info 的目录扫描认得出，不该进候选
    let healthy = levels.join("HealthySet");
    std::fs::create_dir_all(&healthy).unwrap();
    std::fs::write(healthy.join("info"), b"HealthySet").unwrap();

    let parse = |path: &Path| -> Result<LevelSetMetadata, String> {
        let name = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Ok(metadata(&name, None))
    };

    let report = flatten_level_sets_with(
        &crate::level_sets::paths::test_roots(&game),
        &recycle,
        &manifests,
        true,
        &parse,
    )
    .unwrap();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].dir_name, "pear9.5关卡");
    assert_eq!(report.candidates[0].nested_infos, 2);
    assert_eq!(report.flattened, 0);
    assert!(pear.join("info").is_file(), "dry_run 不许动磁盘");

    let report = flatten_level_sets_with(
        &crate::level_sets::paths::test_roots(&game),
        &recycle,
        &manifests,
        false,
        &parse,
    )
    .unwrap();
    assert_eq!(report.flattened, 2);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(levels.join("PearSet/info").is_file(), "info 落成固定名");
    assert!(levels.join("PearSet/shared.assets").is_file());
    assert!(levels.join("OtherSet/info").is_file());
    assert!(
        levels.join("PearSet/PearSetFix.dll").is_file(),
        "插件按名字归到 PearSet"
    );
    assert!(
        levels.join("PearSet/PearSetFix.cfg").is_file(),
        "插件同目录资源一起搬"
    );
    assert!(
        !levels.join("pear9.5关卡").exists(),
        "清空的嵌套壳应进回收站"
    );
    assert!(healthy.join("info").is_file(), "正常目录不受影响");

    std::fs::remove_dir_all(&root).unwrap();
}

/// 探测有上限：`levels/<外层>` 下超过 3 层的 info 不当候选。
#[test]
fn flatten_probe_is_bounded_to_three_levels() {
    let (root, game, recycle) = workspace("flatten-depth");
    let manifests = root.join("manifests");
    let levels = levels_root(&crate::level_sets::paths::test_roots(&game)).unwrap();
    let within = levels.join("within/l1/l2/l3");
    std::fs::create_dir_all(&within).unwrap();
    std::fs::write(within.join("info"), b"x").unwrap();
    let beyond = levels.join("beyond/l1/l2/l3/l4");
    std::fs::create_dir_all(&beyond).unwrap();
    std::fs::write(beyond.join("info"), b"x").unwrap();

    let report = flatten_level_sets_with(
        &crate::level_sets::paths::test_roots(&game),
        &recycle,
        &manifests,
        true,
        &|_: &Path| -> Result<LevelSetMetadata, String> { Err("skip".into()) },
    )
    .unwrap();
    let names: Vec<&str> = report
        .candidates
        .iter()
        .map(|candidate| candidate.dir_name.as_str())
        .collect();
    assert_eq!(names, vec!["within"], "第 4 层及更深的 info 不该被探测到");

    std::fs::remove_dir_all(&root).unwrap();
}

/// 同一外层目录里后一张集失败时，前面已经搬走的文件必须整体还原。
#[test]
fn flatten_rolls_back_the_whole_group_on_failure() {
    let (root, game, recycle) = workspace("flatten-rollback");
    let manifests = root.join("manifests");
    let levels = levels_root(&crate::level_sets::paths::test_roots(&game)).unwrap();
    let alpha = levels.join("broken/levels包/AlphaSet");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::write(alpha.join("info"), b"AlphaSet").unwrap();
    let beta = levels.join("broken/levels包/BetaSet");
    std::fs::create_dir_all(&beta).unwrap();
    std::fs::write(beta.join("info"), b"BetaSet").unwrap();
    // 用一个同名**文件**占住 BetaSet：`collect_dir_names` 只收目录，换名
    // 阶段发现不了，执行到「目标已存在」时才失败 —— 正是要验的中间失败
    std::fs::write(levels.join("BetaSet"), b"blocker").unwrap();

    let parse = |path: &Path| -> Result<LevelSetMetadata, String> {
        let name = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Ok(metadata(&name, None))
    };
    let report = flatten_level_sets_with(
        &crate::level_sets::paths::test_roots(&game),
        &recycle,
        &manifests,
        false,
        &parse,
    )
    .unwrap();

    assert_eq!(report.flattened, 0);
    assert_eq!(report.failed.len(), 1, "失败要逐条记录");
    assert!(report.failed[0].contains("broken"), "{:?}", report.failed);
    assert!(alpha.join("info").is_file(), "已搬走的文件要还原");
    assert!(beta.join("info").is_file());
    assert!(
        !levels.join("AlphaSet").exists(),
        "回滚要删掉本次新建的目录"
    );

    std::fs::remove_dir_all(&root).unwrap();
}
