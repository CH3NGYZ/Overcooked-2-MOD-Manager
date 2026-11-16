use super::diagnostics::error_category;
use super::diagnostics::safe_log_address;
use super::document::RepositorySourceDocument;
use super::document::parse_document;
use super::location::SourceLocation;
use super::location::remote_document_location;
use super::package::SourcePackage;
use crate::config::ConfigError;
use crate::models::NetworkConfig;
use crate::releases;
use futures_util::StreamExt;
use std::time::Duration;

pub(crate) fn package_document_path(location: &SourceLocation) -> Option<String> {
    match location {
        SourceLocation::Bundled(path) => Some(path.clone()),
        SourceLocation::Remote(url) => {
            crate::models::DataProvider::official_document_path(url.as_str())
        }
    }
}

/// 官方源码 ZIP 与自定义 HTTP 源共用的请求超时。
/// 完整传输固定 30 秒，连接与首个非空数据另受用户首字节期限约束。
/// 官方各级 TOML 从已获取的同一包读取，不再逐份等待网络；
/// 需要人工操作的 WebView 下载由调用方单独设置交互期限。
pub const SOURCE_DOCUMENT_TIMEOUT: u64 = 30;

pub(super) const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// MOD、地图和网络文档共用的读取入口。
/// 官方/内置逻辑路径从当前不可变源码包读取，缺文件直接失败；
/// 自定义绝对 HTTP 地址保留有界下载、重定向基准与错误分类。
pub(crate) async fn load_text_document<T>(
    location: &SourceLocation,
    network: &NetworkConfig,
    parse: impl Fn(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    if let Some(path) = package_document_path(location) {
        let package = SourcePackage::active()
            .or_else(|| matches!(location, SourceLocation::Bundled(_)).then(SourcePackage::bundled))
            .ok_or_else(|| ConfigError::Read("source.addressMismatch".into()))?;
        let text = package.read(&path)?;
        log::debug!("读取当前源码包文档：路径={}，字节={}", path, text.len());
        return parse(text, location);
    }
    let timeout_seconds = SOURCE_DOCUMENT_TIMEOUT;
    let SourceLocation::Remote(url) = location else {
        return Err(ConfigError::Read("source.addressMismatch".into()));
    };
    let started_at = std::time::Instant::now();
    let safe_url = safe_log_address(url.as_str());
    log::info!(
        "开始加载远程文档：地址={}，超时={}秒",
        safe_url,
        timeout_seconds
    );
    let first_byte_timeout = network.first_byte_timeout();
    let client = releases::build_download_client(first_byte_timeout).map_err(|error| {
        log::error!(
            "创建文档 HTTP 客户端失败：地址={}，原因={}",
            safe_url,
            error
        );
        ConfigError::Read(error.to_string())
    })?;
    let deadline = tokio::time::Instant::now() + first_byte_timeout;
    let request = client
        .get(url.clone())
        .timeout(Duration::from_secs(timeout_seconds))
        .header("User-Agent", releases::APP_USER_AGENT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache");
    let response = crate::archive::send_before_first_byte_deadline(request, Some(deadline))
        .await
        .map_err(|error| {
            log::error!("请求远程文档失败：地址={}，原因={}", safe_url, error);
            ConfigError::Read(error)
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ConfigError::Read(format!("HTTP {}", status.as_u16())));
    }
    // 普通直链以最终地址为相对基准；可识别的 Raw 请求保留原仓库 ref，
    // 避免 CDN 重定向丢掉分支结构。两条读取入口都交同一实现判定。
    let resolved = remote_document_location(url, response.url().clone())?;
    let mut body = response.bytes_stream();
    let first = crate::archive::first_nonempty_chunk(&mut body, Some(deadline))
        .await
        .map_err(ConfigError::Read)?;
    let mut body = futures_util::stream::iter(first.into_iter().map(Ok)).chain(body);
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await.transpose().map_err(|error| {
        log::error!(
            "读取远程文档响应失败：地址={}，已读取={}字节",
            safe_url,
            bytes.len()
        );
        ConfigError::Read(error.without_url().to_string())
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
/// 索引与子源的完整传输上限统一为 30 秒；首字节期限来自本次配置快照，
/// 不使用较短的 API 总超时限制文档正文传输。
pub async fn load_document(
    location: &SourceLocation,
    inherited: &crate::models::NetworkPlanSelection,
    network: &NetworkConfig,
) -> Result<RepositorySourceDocument, ConfigError> {
    load_text_document(location, network, |text, origin| {
        parse_document(text, origin, inherited)
    })
    .await
}

#[cfg(test)]
mod tests;
