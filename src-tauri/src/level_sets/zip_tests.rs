use crate::asset_bundle::LevelSetMetadata;
use crate::framework::DIYLEVEL_DIR;
use crate::level_sets::identity::MAX_DIR_NAME_CHARS;
use crate::level_sets::identity::allocate_dir_name;
use crate::level_sets::install::install_from_zip;
use crate::level_sets::paths::LEVELS_DIR;
use crate::level_sets::paths::collect_dir_names;
use crate::level_sets::scan::scan_level_sets;
use crate::level_sets::zip_info::is_info_file;
use crate::level_sets::zip_inspect::inspect_zip;
use crate::level_sets::zip_inspect::inspect_zip_with_progress;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

fn rw_zip() -> Option<PathBuf> {
    // cargo test 的 cwd 是 src-tauri/，用 manifest 目录回到项目根
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
    path.is_file().then_some(path)
}

#[test]
#[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
fn inspects_the_published_rw_zip() {
    let Some(zip) = rw_zip() else {
        eprintln!("跳过：没有 .tmp/rw.zip");
        return;
    };
    let sets = inspect_zip(&zip).expect("应能识别 RW 地图 ZIP");
    assert_eq!(sets.len(), 1, "RW 的 ZIP 只有一个关卡集");
    let set = &sets[0];
    assert_eq!(set.metadata.name, "RW_LevelSet");
    assert_eq!(set.metadata.author, "RW");
    assert_eq!(set.metadata.version, "0.73");
    assert_eq!(set.metadata.levels.len(), 10);
    // 目录名来自集名而不是 ZIP 结构
    assert_eq!(set.dir_name, "RW_LevelSet");
    // 文件数 = info + 10 个关卡
    assert_eq!(set.files.len(), 11);
    // info 在 ZIP 里叫 info_rw，落盘时要变成固定名 `info`
    assert!(
        set.files
            .iter()
            .any(|(entry, relative)| { entry == "info_rw" && relative == "info_rw" })
    );
}

#[test]
#[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
fn installs_the_zip_into_levels_with_fixed_info_name() {
    let Some(zip) = rw_zip() else {
        eprintln!("跳过：没有 .tmp/rw.zip");
        return;
    };
    let root = std::env::temp_dir().join(format!("oc2-lv-zip-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(DIYLEVEL_DIR)).unwrap();

    let installed = install_from_zip(
        &zip,
        &root,
        &root.join("manifests"),
        &["RW_LevelSet".to_string()],
    )
    .expect("应能安装");
    assert_eq!(installed, vec!["RW_LevelSet".to_string()]);

    let destination = root.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("RW_LevelSet");
    // DLL 只认固定名 `info`，不能保留 ZIP 里的 `info_rw`
    assert!(destination.join("info").is_file(), "info 应落成固定名");
    assert!(
        !destination.join("info_rw").exists(),
        "不应保留带前缀的名字"
    );
    // 关卡文件保持原名：游戏按 sceneName 找它们
    assert!(destination.join("s_rw_1").is_file());
    assert!(destination.join("s_rw_10").is_file());

    // 装完之后再扫一遍，元数据应能读回来
    let sets = scan_level_sets(&root);
    assert_eq!(sets.len(), 1);
    assert!(sets[0].enabled);
    assert_eq!(sets[0].metadata.as_ref().unwrap().name, "RW_LevelSet");
    assert_eq!(sets[0].metadata.as_ref().unwrap().levels.len(), 10);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
#[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
fn refuses_to_install_over_an_existing_set() {
    let Some(zip) = rw_zip() else {
        eprintln!("跳过：没有 .tmp/rw.zip");
        return;
    };
    let root = std::env::temp_dir().join(format!("oc2-lv-dup-{}", uuid::Uuid::new_v4()));
    let existing = root.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("RW_LevelSet");
    std::fs::create_dir_all(&existing).unwrap();

    // 覆盖会静默丢掉用户已有的关卡集（可能是自己改过的版本）
    let result = install_from_zip(
        &zip,
        &root,
        &root.join("manifests"),
        &["RW_LevelSet".to_string()],
    );
    assert!(result.is_err(), "目标已存在时应拒绝安装");
    std::fs::remove_dir_all(&root).unwrap();
}

/// 只用于目录名分配测试的元数据。
fn metadata(name: &str, name_zh: Option<&str>, author: &str, uid: &str) -> LevelSetMetadata {
    LevelSetMetadata {
        name: name.into(),
        name_zh: name_zh.map(str::to_string),
        author: author.into(),
        version: "1.0.0".into(),
        uid: uid.into(),
        base_uid: None,
        levels: Vec::new(),
    }
}

#[test]
fn allocate_dir_name_disambiguates_with_the_chinese_name() {
    /*
     * 真实样本：同一个作者的两张地图，info 里英文集名与作者都写 GAODA，
     * 只有中文名不同（胡闹糕中 / 胡闹达学）。它们必须落进不同目录，
     * 否则后装的会把先装的当旧版本回收掉。
     */
    let mut taken = std::collections::HashSet::new();
    let first = allocate_dir_name(
        "GAODA",
        &metadata("GAODA", Some("胡闹糕中"), "GAODA", "6040602f-aaaa"),
        &mut taken,
        false,
    );
    let second = allocate_dir_name(
        "GAODA",
        &metadata("GAODA", Some("胡闹达学"), "GAODA", "4b519ea0-bbbb"),
        &mut taken,
        false,
    );
    // 同一批共用一个集名时，两张都带后缀，谁也不独占「干净」名字
    assert_eq!(first, "GAODA_胡闹糕中");
    assert_eq!(second, "GAODA_胡闹达学");
    assert_ne!(first.to_lowercase(), second.to_lowercase());
}

#[test]
fn allocate_dir_name_treats_case_only_differences_as_collisions() {
    // Windows 与 macOS 的默认文件系统都不区分大小写：`GAODA` 与 `gaoda`
    // 会落进同一个目录，只比原串会漏判。
    let mut taken = std::collections::HashSet::new();
    let first = allocate_dir_name("GAODA", &metadata("GAODA", None, "", ""), &mut taken, true);
    let second = allocate_dir_name("gaoda", &metadata("gaoda", None, "", ""), &mut taken, true);
    assert_eq!(first, "GAODA");
    assert_ne!(second.to_lowercase(), first.to_lowercase());
}

#[test]
fn allocate_dir_name_falls_back_to_author_then_uid_then_number() {
    let mut taken = std::collections::HashSet::new();
    // 第一个直接拿到基础名
    assert_eq!(
        allocate_dir_name(
            "S",
            &metadata("S", None, "A1", "aaaa1111-2222"),
            &mut taken,
            true
        ),
        "S"
    );
    // 没中文名 → 用作者
    assert_eq!(
        allocate_dir_name(
            "S",
            &metadata("S", None, "A1", "bbbb2222-3333"),
            &mut taken,
            true
        ),
        "S_A1"
    );
    // 作者也撞了 → 用 uid 前 8 位
    assert_eq!(
        allocate_dir_name(
            "S",
            &metadata("S", None, "A1", "cccc3333-4444"),
            &mut taken,
            true
        ),
        "S_cccc3333"
    );
    // 三段全撞 → 数字兜底
    assert_eq!(
        allocate_dir_name(
            "S",
            &metadata("S", None, "A1", "cccc3333-4444"),
            &mut taken,
            true
        ),
        "S_2"
    );
}

#[test]
fn allocate_dir_name_keeps_names_usable_as_directories() {
    // 集名里的非法字符要先清理，超长要截断，否则建不目录
    let mut taken = std::collections::HashSet::new();
    let name = allocate_dir_name(
        "a/b:c",
        &metadata("a/b:c", Some("x/y"), "aut hor", ""),
        &mut taken,
        true,
    );
    assert_eq!(name, "a_b_c");
    let long = "x".repeat(200);
    let mut taken = std::collections::HashSet::new();
    let name = allocate_dir_name(&long, &metadata(&long, None, "", ""), &mut taken, true);
    assert!(name.chars().count() <= MAX_DIR_NAME_CHARS);
}

#[test]
fn collects_existing_directory_names_for_collision_checks() {
    let root = std::env::temp_dir().join(format!("oc2-lv-names-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("GAODA")).unwrap();
    std::fs::create_dir_all(root.join("Other")).unwrap();
    std::fs::write(root.join("file.txt"), b"x").unwrap();
    let names = collect_dir_names(&root);
    assert!(names.contains("gaoda"), "应收集到目录名并转小写");
    assert!(names.contains("other"));
    assert!(!names.contains("file.txt"), "文件不算占用目录名");
    std::fs::remove_dir_all(&root).unwrap();
}

/// 造一个 ZIP：`entries` 是 (条目名, 内容) 列表。
fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    use std::io::Write as _;
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

/// 从 `.tmp/rw.zip` 里读出一份真实 `info` 的字节。
fn real_info_bytes() -> Option<Vec<u8>> {
    let zip = rw_zip()?;
    let file = std::fs::File::open(zip).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).ok()?;
        if is_info_file(entry.name()) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).ok()?;
            return Some(bytes);
        }
    }
    None
}

/// 并行解析 `info` 时，每份候选都要**先上报进度再解析**。
///
/// 用一份「全是垃圾 info」的 ZIP：识别整体失败，但进度回调必须已经走完
/// 三份候选 —— 界面上「正在读取哪个 info / 读到第几份」就靠这条链。
#[test]
fn reports_progress_for_every_info_candidate() {
    let root = std::env::temp_dir().join(format!("oc2-lv-progress-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let zip_path = root.join("broken.zip");
    write_zip(
        &zip_path,
        &[
            ("A/info_a", b"not a bundle"),
            ("B/info_b", b"not a bundle"),
            ("C/info_c", b"not a bundle"),
        ],
    );
    let seen = std::sync::Mutex::new(Vec::<(usize, usize, String)>::new());
    let error = inspect_zip_with_progress(&zip_path, 2, 2, &|done, total, current| {
        seen.lock()
            .unwrap()
            .push((done, total, current.to_string()));
    })
    .expect_err("三份 info 都不是关卡集，识别应整体失败");
    assert!(
        error.contains("levelSet.zipInfoInvalid"),
        "错误应带 tag：{error}"
    );
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 3, "每份候选都应上报一次进度");
    assert!(
        seen.iter()
            .all(|(done, total, _)| *total == 3 && *done < *total),
        "已完成数应始终小于总数：{seen:?}"
    );
    let mut names = seen
        .iter()
        .map(|(_, _, current)| current.clone())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["A/info_a", "B/info_b", "C/info_c"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
#[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
fn picks_the_parsable_info_when_a_directory_has_several() {
    /*
     * 回归：「04_前置地图合集」的 `宜/` 目录里 `info_yi` 是真关卡集，
     * `info_yil` 只是随包带进来的资源（没有 LevelSetInfoSO 对象）。
     * 旧实现按目录只留一个 info，后者覆盖前者，于是整包报
     * missingLevelSetInfo。现在必须逐个试解析，取能读出来的那个。
     */
    let Some(info) = real_info_bytes() else {
        eprintln!("跳过：没有 .tmp/rw.zip");
        return;
    };
    let zip_path = std::env::temp_dir().join(format!("oc2-lv-multi-{}.zip", uuid::Uuid::new_v4()));
    // 坏 info 故意排在后面：旧实现取最后一个，正好会踩到这个
    write_zip(
        &zip_path,
        &[
            ("Set/info_good", &info),
            ("Set/info_bogus", b"not a unity bundle"),
            ("Set/scene1", b"scene"),
        ],
    );

    let sets = inspect_zip(&zip_path).expect("同目录多个 info 时应取能解析的那个，而不是整包失败");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].metadata.name, "RW_LevelSet");
    assert_eq!(sets[0].dir_name, "RW_LevelSet");
    assert_eq!(sets[0].files.len(), 3, "同目录的文件都要归到这个关卡集");

    std::fs::remove_file(&zip_path).unwrap();
}

#[test]
#[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
fn reports_the_reason_when_every_info_in_a_directory_fails() {
    // 全部候选都解析不出关卡集时才报错，而且错误里要有解析原因
    let zip_path = std::env::temp_dir().join(format!("oc2-lv-bad-{}.zip", uuid::Uuid::new_v4()));
    write_zip(
        &zip_path,
        &[("Set/info_a", b"junk-a"), ("Set/info_b", b"junk-b")],
    );
    let error = inspect_zip(&zip_path).expect_err("全都解析不了时应失败");
    assert!(error.starts_with("levelSet.zipInfoInvalid"), "实际={error}");
    std::fs::remove_file(&zip_path).unwrap();
}

/// 用真实地图 ZIP 做端到端验证，路径由环境变量给出（仓库里不放几百 MB 的包）。
///
/// `OC2_LEVEL_SET_REAL_ZIP=<zip 路径> cargo test --lib real_zip -- --ignored --nocapture`
#[test]
#[ignore = "需要 OC2_LEVEL_SET_REAL_ZIP 指向真实地图 ZIP"]
fn inspects_a_real_zip_from_the_environment() {
    let Some(path) = std::env::var_os("OC2_LEVEL_SET_REAL_ZIP").map(PathBuf::from) else {
        eprintln!("跳过：没有 OC2_LEVEL_SET_REAL_ZIP");
        return;
    };
    let sets = inspect_zip(&path).expect("应能识别这个 ZIP");
    assert!(!sets.is_empty(), "至少识别出一个关卡集");
    let mut seen = std::collections::HashSet::new();
    for set in &sets {
        eprintln!(
            "目录={} 集名={} 中文={:?} 作者={} 版本={} 关卡={} 文件={}",
            set.dir_name,
            set.metadata.name,
            set.metadata.name_zh,
            set.metadata.author,
            set.metadata.version,
            set.metadata.levels.len(),
            set.files.len()
        );
        assert!(
            seen.insert(set.dir_name.to_lowercase()),
            "同一批里的目录名必须互不相同：{}",
            set.dir_name
        );
    }
}
