use super::diagnostics::error_category;
use super::diagnostics::safe_log_address;
use super::document::RepositorySourceDocument;
use super::document::parse_document;
use super::location::SourceLocation;
use super::location::remote_document_location;
use super::package::SourcePackage;
use crate::config::ConfigError;
use crate::http_diagnostics::HttpRequestExt;
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

/// 官方源码 ZIP 的完整传输上限。文档的预算来自 API 超时设置。
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
    let SourceLocation::Remote(url) = location else {
        return Err(ConfigError::Read("source.addressMismatch".into()));
    };
    let client = document_client(network)?;
    let (bytes, redirected) =
        download_document(&client, url.as_str(), network.api_timeout(), None).await?;
    let origin = remote_document_location(url, redirected)?;
    parse_remote_document(url.as_str(), bytes, &origin, parse)
}

pub(super) fn document_client(network: &NetworkConfig) -> Result<reqwest::Client, ConfigError> {
    releases::build_download_client(network.api_timeout())
        .map_err(|error| ConfigError::Read(error.to_string()))
}

pub(super) async fn download_document(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
    reporter: Option<&crate::archive::DownloadReporter>,
) -> Result<(Vec<u8>, url::Url), ConfigError> {
    let safe_url = safe_log_address(url);
    let started_at = std::time::Instant::now();
    let request = client
        .get(url)
        .timeout(timeout)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache");
    let response = request
        .send_logged()
        .await
        .map_err(|error| ConfigError::Read(error.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ConfigError::Read(format!("HTTP {}", status.as_u16())));
    }
    let redirected = response.url().clone();
    let length = response.content_length();
    if length.is_some_and(|size| size > MAX_SOURCE_BYTES as u64) {
        return Err(ConfigError::Invalid("source.tooLarge".into()));
    }
    let mut progress = reporter.map(|reporter| {
        let mut progress = reporter.snapshot();
        progress.total = length;
        progress
    });
    let mut body = response.bytes_stream();
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
        if let Some((reporter, progress)) = reporter.zip(progress.as_mut()) {
            reporter.received(chunk.len() as u64);
            progress.downloaded = bytes.len() as u64;
            reporter.update(progress.clone());
        }
    }
    log::debug!(
        "远程文档下载完成：地址={}，状态码={}，字节={}，耗时={}ms",
        safe_url,
        status.as_u16(),
        bytes.len(),
        started_at.elapsed().as_millis()
    );
    Ok((bytes, redirected))
}

pub(super) fn parse_remote_document<T>(
    url: &str,
    bytes: Vec<u8>,
    location: &SourceLocation,
    parse: impl FnOnce(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let byte_count = bytes.len();
    let result = String::from_utf8(bytes)
        .map_err(|error| ConfigError::Parse(error.to_string()))
        .and_then(|text| parse(&text, location));
    if let Err(error) = &result {
        log::warn!(
            "远程文档解析失败：地址={}，字节={}，错误类别={}",
            safe_log_address(url),
            byte_count,
            error_category(error)
        );
    }
    result
}

/// 读取一个源文档（远程地址或内置资源）。
///
/// 远程文本使用本次配置快照的 API 预算约束完整响应。
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
