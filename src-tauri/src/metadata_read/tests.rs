fn clear_dll_details_cache() {
    if let Ok(mut cache) = dll_details_cache().lock() {
        cache.clear();
    }
}

use super::*;
use crate::concurrency::cores;
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("oc2-metadata-read-{tag}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("应能创建临时目录");
    dir
}

/// 测试用的最小可解析 DLL：真实文件 + BepInPlugin 元数据。
fn write_fake_dll(path: &Path, guid: &str, version: &str) {
    let mut bytes = b"MZ".to_vec();
    bytes.extend_from_slice(&crate::mods::plugin_blob(guid, "Fake Mod", version));
    fs::write(path, bytes).expect("应能写入假 DLL");
}

#[test]
fn batch_returns_one_outcome_per_request_in_order_and_isolates_errors() {
    let dir = temp_dir("order");
    let good = dir.join("good.dll");
    write_fake_dll(&good, "com.example.good", "1.2.3");
    let missing = dir.join("missing.dll");
    let requests = vec![
        // `read_dll_details` 对缺失文件不报错（按空字节降级），
        // 用返回 `Result` 的 info 读取来验「失败只错自己」。
        MetadataRequest {
            path: missing.clone(),
            display_name: String::new(),
            kind: MetadataKind::LevelSetInfo,
        },
        MetadataRequest {
            path: good.clone(),
            display_name: "good.dll".into(),
            kind: MetadataKind::ModDll,
        },
    ];
    let outcomes = read_metadata_batch(&requests, 2, &|_, _| {});
    assert_eq!(outcomes.len(), 2, "长度必须与请求一一对应");
    assert!(outcomes[0].error.is_some(), "缺失文件只错自己");
    assert!(
        outcomes[1].mod_dll().is_some(),
        "另一个文件不受影响：{:?}",
        outcomes[1].error
    );
    assert_eq!(
        outcomes[1].value.as_ref().and_then(MetadataValue::version),
        Some("1.2.3"),
        "版本号走统一出口"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_requests_read_the_file_once() {
    let dir = temp_dir("dedupe");
    let dll = dir.join("dup.dll");
    write_fake_dll(&dll, "com.example.dup", "2.0.0");
    let request = |display: &str| MetadataRequest {
        path: dll.clone(),
        display_name: display.into(),
        kind: MetadataKind::ModDll,
    };
    let seen = AtomicUsize::new(0);
    let outcomes = read_metadata_batch(&[request("dup.dll"), request("dup.dll")], 2, &|_, _| {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(seen.load(Ordering::SeqCst), 1, "同一文件只读一次");
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|outcome| outcome.mod_dll().is_some()));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dll_cache_hits_until_the_file_changes() {
    let dir = temp_dir("cache");
    let dll = dir.join("cached.dll");
    write_fake_dll(&dll, "com.example.cached", "1.0.0");
    clear_dll_details_cache();
    let requests = [MetadataRequest {
        path: dll.clone(),
        display_name: "cached.dll".into(),
        kind: MetadataKind::ModDll,
    }];
    let first = read_metadata_batch(&requests, 1, &|_, _| {});
    assert_eq!(
        first[0].value.as_ref().and_then(MetadataValue::version),
        Some("1.0.0")
    );
    // 覆盖成新版本：mtime 变化后必须重读，不能命中旧缓存。
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_fake_dll(&dll, "com.example.cached", "9.9.9");
    let second = read_metadata_batch(&requests, 1, &|_, _| {});
    assert_eq!(
        second[0].value.as_ref().and_then(MetadataValue::version),
        Some("9.9.9"),
        "文件变了必须重新解析"
    );
    clear_dll_details_cache();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_batch_returns_no_outcomes() {
    let outcomes = read_metadata_batch(&[], 4, &|_, _| {});
    assert!(outcomes.is_empty());
}

/// 真实地图目录的端到端基准（`#[ignore]`，本地手动跑）。
///
/// 进程内缓存是冷的，所以要用两个进程对比：
///
/// ```text
/// OC2_BENCH_WORKERS=1 cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture bench_real_metadata_batch
/// OC2_BENCH_WORKERS=4 cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture bench_real_metadata_batch
/// ```
///
/// 目录用 `OC2_BENCH_LEVELS` 覆盖，默认取开发者本机的游戏安装。磁盘缓存
/// （`cache/level-info`）两边都是热的，所以量的是「之后每次启动」的成本；
/// 首次导入的解析成本见 `asset_bundle::bench_real_level_sets`。
#[test]
#[ignore]
fn bench_real_metadata_batch() {
    use std::time::Instant;

    let root = PathBuf::from(std::env::var("OC2_BENCH_LEVELS").unwrap_or_else(|_| {
        "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel".into()
    }));
    if !root.is_dir() {
        eprintln!("跳过：没有关卡集目录（设置 OC2_BENCH_LEVELS 指向 OC2DIYLevel）");
        return;
    }
    let workers = std::env::var("OC2_BENCH_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(cores);
    let mut requests = Vec::new();
    for side in ["levels", "disabled"] {
        let Ok(entries) = fs::read_dir(root.join(side)) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                if !name.starts_with("info") {
                    continue;
                }
                requests.push(MetadataRequest {
                    path: file.path(),
                    display_name: String::new(),
                    kind: MetadataKind::LevelSetInfo,
                });
                break;
            }
        }
    }
    if requests.is_empty() {
        eprintln!("跳过：{} 下一个 info 都没找到", root.display());
        return;
    }
    let total_bytes: u64 = requests
        .iter()
        .filter_map(|request| fs::metadata(&request.path).ok())
        .map(|meta| meta.len())
        .sum();
    let start = Instant::now();
    let outcomes = read_metadata_batch(&requests, workers, &|_, _| {});
    let elapsed = start.elapsed();
    let failed = outcomes
        .iter()
        .filter(|outcome| outcome.error.is_some())
        .count();
    for (request, outcome) in requests.iter().zip(&outcomes) {
        if let Some(error) = &outcome.error {
            eprintln!("失败：{}（{}）", request.path.display(), error);
        }
    }
    eprintln!(
        "元数据批读：文件={} 合计={:.1} MB 并行度={} 耗时={:.2} s 失败={}",
        requests.len(),
        total_bytes as f64 / (1024.0 * 1024.0),
        workers,
        elapsed.as_secs_f64(),
        failed
    );
    assert_eq!(outcomes.len(), requests.len());
}
