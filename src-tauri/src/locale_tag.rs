//! 错误 tag 的构造工具。
//!
//! 后端不再返回中文文案，只返回 `error.<域>.<原因>` 形式的 tag，由前端按
//! 当前界面语言翻译（见 `src/locale/backend.ts`）。
//!
//! ## 为什么参数放在查询串里
//!
//! 带参数的错误需要把动态内容（路径、原因、数量）传给前端插值。两种做法：
//!   1. 定义结构体 + serde 序列化 —— 要改 `Result<T, String>` 的签名、
//!      要处理 Tauri 命令的错误类型、每一处 `?` 都得转换；
//!   2. tag 后面挂查询串 —— 一个 `format!` 就够，`Result<T, String>` 不用动。
//!
//! 选 2：迁移成本决定成败。约 1500 条中文串，签名一改就没有增量迁移的可能。
//!
//! ## 为什么要编码
//!
//! 参数里常有 Windows 路径（`D:\backups\x`）、错误原文（可能含 `&` `?` `%`），
//! 直接拼进查询串会让前端解析错位。这里只保留 `[A-Za-z0-9._~-]`，其余一律
//! 百分号编码，前端 `decodeURIComponent` 原样还原。

/// 把参数值编码成可安全放进查询串的形式。
///
/// 不用 `urlencoded` crate：只需要这一种编码，且必须与前端
/// `decodeURIComponent` 完全对称，自己实现更容易保证一致。
pub(crate) fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            // unreserved 字符原样保留；这些都出现在常见路径里，保留可读性
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'.'
            | b'_'
            | b'~'
            | b'-'
            | b'/'
            | b':'
            | b'\\' => out.push(*byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 构造带参数的 tag：`base?k1=v1&k2=v2`。
///
/// `pairs` 为空时直接返回 `base`，不产生多余的 `?`。
pub(crate) fn with_params(base: &str, pairs: &[(&str, &str)]) -> String {
    if pairs.is_empty() {
        return base.to_string();
    }
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}

#[cfg(test)]
mod tests {
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
}
