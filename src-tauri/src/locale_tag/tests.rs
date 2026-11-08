use super::*;

#[test]
fn keeps_readable_path_characters() {
    // Windows 路径里的盘符、反斜杠、点号都保留，日志与调试时能直接读
    assert_eq!(
        encode(r"D:\backups\BepInEx.backup-1"),
        r"D:\backups\BepInEx.backup-1"
    );
    assert_eq!(encode("plain-text_1.2-3~4"), "plain-text_1.2-3~4");
}

#[test]
fn encodes_query_delimiters_so_params_cannot_break_parsing() {
    // 这些字符若原样保留，前端会把一个参数拆成两个
    assert_eq!(encode("a&b"), "a%26b");
    assert_eq!(encode("a?b"), "a%3Fb");
    assert_eq!(encode("a=b"), "a%3Db");
    assert_eq!(encode("100%"), "100%25");
}

#[test]
fn encodes_non_ascii_as_utf8_bytes() {
    // 中文按 UTF-8 逐字节编码，前端 decodeURIComponent 能还原
    assert_eq!(encode("失败"), "%E5%A4%B1%E8%B4%A5");
}

#[test]
fn with_params_omits_question_mark_when_empty() {
    assert_eq!(with_params("error.x.y", &[]), "error.x.y");
}

#[test]
fn with_params_joins_multiple_pairs() {
    assert_eq!(
        with_params(
            "error.rename.nameTooLong",
            &[("bytes", "240"), ("limit", "238")]
        ),
        "error.rename.nameTooLong?bytes=240&limit=238"
    );
}

#[test]
fn with_params_encodes_values_but_not_keys() {
    assert_eq!(
        with_params(
            "error.backup.recycleFailed",
            &[("reason", "a&b"), ("path", r"D:\x")]
        ),
        r"error.backup.recycleFailed?reason=a%26b&path=D:\x"
    );
}
