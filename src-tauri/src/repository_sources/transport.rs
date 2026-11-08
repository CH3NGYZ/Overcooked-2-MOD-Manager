use super::diagnostics::error_category;
use super::diagnostics::safe_log_address;
use super::document::RepositorySourceDocument;
use super::document::parse_document;
use super::location::SourceLocation;
use super::location::remote_document_location;
use crate::config::ConfigError;
use crate::releases;

/// 读取仓库源文档（raw 索引与各级子源）的访问超时。
///
/// 固定 30 秒，**不跟随** `network.timeout_seconds`：
///   · 那个设置是给 Release API / 下载用的，用户为了「下载更快失败」
///     调到 5 秒时，不该把源文档也一起压到 5 秒 —— 国内访问 Gitee raw
///     首次握手常常就要好几秒，压到 5 秒会让整棵树随机失败；
///   · 源文档阶段是串行递归（索引 → 子源），一层的超时过短会让后面
///     所有子源根本没有机会加载。
///
/// 这与仓库说明（README）的 30 秒保持同一量级：两者都是「读一个不大的
/// 文本文件」，不是大文件下载。
pub const SOURCE_DOCUMENT_TIMEOUT: u64 = 30;

pub(super) const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// 下载（或读取内置）一个源文档，再交给调用方解析。
///
/// 抽出来是因为地图源要读**同样形态**的文本文档（`maps.toml`），而下载这条
/// 路上每一步都踩过坑：30 秒固定超时、禁缓存头、大小上限、重定向后的相对
/// 地址解析、错误分类。复制一份只会得到两套迟早各自漂移的规则。
///
/// 返回解析后的结果，日志记录尽力而为 —— 解析失败时仍把状态码与字节数写进
/// 日志，否则远程文档「下载成功但解析失败」这种情况无从区分。
pub(crate) async fn load_text_document<T>(
    location: &SourceLocation,
    parse: impl Fn(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let timeout_seconds = SOURCE_DOCUMENT_TIMEOUT;
    let SourceLocation::Remote(url) = location else {
        let text = location.bundled_text()?;
        log::info!(
            "内置文档加载完成：地址={}",
            safe_log_address(&location.address())
        );
        return parse(text, location);
    };
    let started_at = std::time::Instant::now();
    let safe_url = safe_log_address(url.as_str());
    log::info!(
        "开始加载远程文档：地址={}，超时={}秒",
        safe_url,
        timeout_seconds
    );
    let client = releases::build_client(timeout_seconds).map_err(|error| {
        log::error!(
            "创建文档 HTTP 客户端失败：地址={}，原因={}",
            safe_url,
            error
        );
        ConfigError::Read(error.to_string())
    })?;
    let mut response = client
        .get(url.clone())
        .header("User-Agent", releases::APP_USER_AGENT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache")
        .send()
        .await
        .map_err(|error| {
            log::error!(
                "请求远程文档失败：地址={}，超时={}，连接错误={}",
                safe_url,
                error.is_timeout(),
                error.is_connect()
            );
            ConfigError::Read(error.to_string())
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ConfigError::Read(format!("HTTP {}", status.as_u16())));
    }
    // 普通直链以最终地址为相对基准；可识别的 Raw 请求保留原仓库 ref，
    // 避免 CDN 重定向丢掉分支结构。两条读取入口都交同一实现判定。
    let resolved = remote_document_location(url, response.url().clone())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        log::error!(
            "读取远程文档响应失败：地址={}，已读取={}字节",
            safe_url,
            bytes.len()
        );
        ConfigError::Read(error.to_string())
    })? {
        if bytes.len() + chunk.len() > MAX_SOURCE_BYTES {
            log::error!("远程文档超过大小限制：地址={}", safe_url);
            return Err(ConfigError::Invalid("source.tooLarge".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let byte_count = bytes.len();
    let text = String::from_utf8(bytes).map_err(|error| {
        log::error!("远程文档不是有效 UTF-8：地址={}，原因={}", safe_url, error);
        ConfigError::Parse(error.to_string())
    })?;
    let result = parse(&text, &resolved);
    match &result {
        Ok(_) => log::info!(
            "远程文档加载完成：地址={}，状态码={}，字节={}，耗时={}ms",
            safe_url,
            status.as_u16(),
            byte_count,
            started_at.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "远程文档解析失败：地址={}，状态码={}，字节={}，错误类别={}",
            safe_url,
            status.as_u16(),
            byte_count,
            error_category(error)
        ),
    }
    result
}

/// 读取一个源文档（远程地址或内置资源）。
///
/// 远程文档固定使用 `SOURCE_DOCUMENT_TIMEOUT`（30 秒）：索引与各级子源都走
/// 这里，超时口径一致。不再由调用方传入 —— 这个阶段和 Release API 的
/// 5 秒、下载的连接超时是三种不同性质的请求，混用一个配置会让
/// 「调快下载」顺带把源文档读取弄坏。
pub async fn load_document(
    location: &SourceLocation,
) -> Result<RepositorySourceDocument, ConfigError> {
    if matches!(location, SourceLocation::Bundled(_)) {
        return location.bundled_document();
    }
    load_text_document(location, parse_document).await
}

#[cfg(test)]
mod tests;
