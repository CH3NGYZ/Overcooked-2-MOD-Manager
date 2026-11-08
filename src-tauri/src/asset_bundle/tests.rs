use super::*;

/// 用真实 info 文件验证解析结果。
///
/// 期望值来自 UnityPy（社区标准的 Unity 资源解析库）独立解析的结果，
/// 写死在测试里，运行时不依赖 Python。
///
/// 路径来自开发者本机的游戏安装；找不到文件时跳过（CI 上没有游戏），
/// 本地跑就能立刻发现解析回归。
fn game_info(name: &str) -> Option<Vec<u8>> {
    let path = format!(
        "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel/levels/{name}/info_{name}"
    );
    std::fs::read(&path).ok()
}

#[test]
fn parses_test_level_metadata() {
    let Some(bytes) = game_info("test_level") else {
        eprintln!("跳过：本机没有 test_level（CI 环境）");
        return;
    };
    let parsed = parse_level_set(&bytes).expect("应能解析 test_level");
    assert_eq!(parsed.name, "Test");
    assert_eq!(parsed.name_zh.as_deref(), Some("测试"));
    assert_eq!(parsed.author, "GUA");
    assert_eq!(parsed.version, "0.5.1");
    assert_eq!(parsed.uid, "24f57368-9ec4-a216-ec44-824ffdc33d66");
    assert_eq!(parsed.levels.len(), 5);
    assert_eq!(parsed.levels[0].name, "Test 1");
    assert_eq!(parsed.levels[0].name_zh.as_deref(), Some("测试 1"));
    assert_eq!(parsed.levels[0].scene_name, "s_test_level");
    assert_eq!(parsed.levels[0].min_order_count, 2);
    assert_eq!(parsed.levels[0].max_order_count, 5);
}

#[test]
fn parses_oc1_story_metadata() {
    let Some(bytes) = game_info("oc1_story") else {
        eprintln!("跳过：本机没有 oc1_story（CI 环境）");
        return;
    };
    let parsed = parse_level_set(&bytes).expect("应能解析 oc1_story");
    assert_eq!(parsed.name, "Overcooked 1 - Story");
    assert_eq!(parsed.name_zh.as_deref(), Some("胡闹厨房 1 - 主线"));
    // 作者留空：官方导出的关卡集没有填 author
    assert_eq!(parsed.author, "");
    assert_eq!(parsed.version, "1.0.1");
    assert_eq!(parsed.levels.len(), 28);
    assert_eq!(parsed.levels[0].scene_name, "s_oc1_story_1_1");
}

#[test]
fn rejects_non_bundle_data() {
    assert!(matches!(
        parse_level_set(b"not an asset bundle at all"),
        Err(ParseError::NotABundle)
    ));
}

/// 真实目录的端到端耗时基准（`#[ignore]`，只在本地手动跑）。
///
/// ```text
/// cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture bench_real_level_sets
/// ```
///
/// 存在的意义是把「首次解析慢」拆成三段可比的数字：读盘、SHA-256、解压+解对象表。
/// 换哈希算法、换 LZMA 实现（纯 Rust `lzma-rs` ↔ liblzma 绑定）、改并行度时
/// 前后各跑一次，才谈得上「快了多少」。目录用 `OC2_BENCH_LEVELS` 覆盖，
/// 默认取开发者本机的游戏安装；没有目录就跳过（CI 上没有游戏）。
#[test]
#[ignore]
fn bench_real_level_sets() {
    use sha2::{Digest, Sha256};
    use std::time::{Duration, Instant};

    let root = std::path::PathBuf::from(std::env::var("OC2_BENCH_LEVELS").unwrap_or_else(|_| {
        "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel".into()
    }));
    if !root.is_dir() {
        eprintln!("跳过：没有关卡集目录（设置 OC2_BENCH_LEVELS 指向 OC2DIYLevel）");
        return;
    }

    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    for side in ["levels", "disabled"] {
        let Ok(entries) = std::fs::read_dir(root.join(side)) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                if name.starts_with("info") {
                    targets.push(file.path());
                    break;
                }
            }
        }
    }
    targets.sort();
    if targets.is_empty() {
        eprintln!("跳过：{} 下一个关卡集都没找到", root.display());
        return;
    }

    let (mut read_all, mut hash_all, mut parse_all) =
        (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for path in &targets {
        let start = Instant::now();
        let bytes = std::fs::read(path).expect("读取 info 失败");
        let read = start.elapsed();

        let start = Instant::now();
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let _ = hasher.finalize();
        let hash = start.elapsed();

        let start = Instant::now();
        let parsed = parse_level_set(&bytes);
        let parse = start.elapsed();

        read_all += read;
        hash_all += hash;
        parse_all += parse;
        let label = path
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        eprintln!(
            "{label:>16} {:>7.1} MiB  读 {:>7.1} ms  哈希 {:>7.1} ms  解析 {:>8.1} ms  {}",
            bytes.len() as f64 / (1024.0 * 1024.0),
            read.as_secs_f64() * 1000.0,
            hash.as_secs_f64() * 1000.0,
            parse.as_secs_f64() * 1000.0,
            if parsed.is_ok() { "ok" } else { "err" },
        );
    }
    eprintln!(
        "合计 {} 份：读 {:.0} ms  哈希 {:.0} ms  解析 {:.0} ms  → 串行总计 {:.0} ms",
        targets.len(),
        read_all.as_secs_f64() * 1000.0,
        hash_all.as_secs_f64() * 1000.0,
        parse_all.as_secs_f64() * 1000.0,
        (read_all + hash_all + parse_all).as_secs_f64() * 1000.0,
    );
}

/// 用真实发布的地图 ZIP 里的 info 验证解析（RW 地图 v0.73）。
///
/// 这条比内置的两份更适合当回归基线：它是**线上真实分发**的资源，
/// 作者用的是另一套导出环境。文件放在 `.tmp/info_rw`（下载后提取），
/// 不存在时跳过。
#[test]
fn parses_published_rw_level_set() {
    let Ok(bytes) = std::fs::read(".tmp/info_rw") else {
        eprintln!("跳过：没有 .tmp/info_rw");
        return;
    };
    let parsed = parse_level_set(&bytes).expect("应能解析 RW 地图");
    assert_eq!(parsed.name, "RW_LevelSet");
    assert_eq!(parsed.name_zh.as_deref(), Some("RW_关卡合集"));
    assert_eq!(parsed.author, "RW");
    assert_eq!(parsed.version, "0.73");
    assert_eq!(
        parsed.base_uid.as_deref(),
        Some("2f39ebdb-529c-4894-b732-dc76f953ce13")
    );
    assert_eq!(parsed.levels.len(), 10);
}
