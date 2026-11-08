use super::*;

/// panic 兜底路径必须能在目录不存在时自建父目录并追加写入：
/// 这条路径是「启动崩溃却没有任何日志」的唯一补救手段。
#[test]
fn appends_panic_lines_and_creates_missing_directories() {
    let base = std::env::temp_dir().join(format!("oc2-panic-log-{}", uuid::Uuid::new_v4()));
    let path = base.join("nested/crash.log");
    append_panic_line(&path, "first").unwrap();
    append_panic_line(&path, "second").unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("first"), "{content}");
    assert!(content.contains("second"), "{content}");
    assert_eq!(content.matches("[panic]").count(), 2, "{content}");
    fs::remove_dir_all(base).unwrap();
}
