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
mod tests;
