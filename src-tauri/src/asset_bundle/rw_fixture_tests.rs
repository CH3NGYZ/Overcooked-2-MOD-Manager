use super::*;

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
