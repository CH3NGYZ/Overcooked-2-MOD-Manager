use super::diagnostics::error_category;
use super::diagnostics::safe_log_address;
use super::location::SourceLocation;
use super::transport::MAX_SOURCE_BYTES;
use super::transport::SOURCE_DOCUMENT_TIMEOUT;
use super::transport::load_text_document;
use crate::config::ConfigError;
use crate::http_diagnostics::HttpRequestExt;
use crate::releases;

/// 一份需要「先问 API、再下载比对」的远程文档。
///
/// 第三方地图源把整份 TOML 放进 Release 资产里
/// （`.../releases/download/maps/map-sources.toml`）。这个位置比 raw 强的地方
/// 在于：sha256 由 Release API 给出（GitHub 的 `digest` 字段），下载还能复用
/// 直连/代理竞速。`load_text_document` 只会裸 GET 一次，所以另开这一条。
///
/// 校验信息必须来自**同一个** assets 条目：先读 API、再按 API 说的 size/digest
/// 比对，顺序反过来（先下载）就没有可信的期望值了。
pub(crate) struct VerifiedDocument {
    /// Release API 报告的字节数（Gitee 也会给）。
    pub expected_size: Option<u64>,
    /// Release API 报告的摘要，形如 `sha256:<hex>`；Gitee 没有 → `None`。
    pub digest: Option<String>,
    /// 下载候选（直连 + 代理），顺序由代理模式决定。
    pub candidates: Vec<crate::download::DownloadCandidate>,
}

/// 下载并**校验**一个远程文本文档，再交给调用方解析。
///
/// 非远程地址或空候选沿普通读取。非空候选一旦开始下载，下载或完整性
/// 失败必须返回错误，不能降级成未经同样校验的请求。
pub(crate) async fn load_verified_text_document<T>(
    location: &SourceLocation,
    verified: &VerifiedDocument,
    parse: impl Fn(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let SourceLocation::Remote(url) = location else {
        return load_text_document(location, parse).await;
    };
    if verified.candidates.is_empty() {
        log::warn!(
            "校验文档没有可用下载候选，退回单次直连：地址={}",
            safe_log_address(url.as_str())
        );
        return load_text_document(location, parse).await;
    }
    let safe_url = safe_log_address(url.as_str());
    let started_at = std::time::Instant::now();
    log::info!(
        "开始加载校验文档：地址={}，候选={}，期望字节={:?}，期望摘要={}",
        safe_url,
        verified.candidates.len(),
        verified.expected_size,
        if verified.digest.is_some() {
            "有"
        } else {
            "无"
        }
    );
    let client = releases::build_client(SOURCE_DOCUMENT_TIMEOUT).map_err(|error| {
        log::error!(
            "创建文档 HTTP 客户端失败：地址={}，原因={}",
            safe_url,
            error
        );
        ConfigError::Read(error.to_string())
    })?;
    let expected_size = verified.expected_size;
    let digest = verified.digest.clone();
    let bytes = crate::download::try_download_candidates(
        verified.candidates.clone(),
        || false,
        |candidate, reporter| {
            let client = client.clone();
            let digest = digest.clone();
            async move {
                fetch_document_bytes(
                    &client,
                    &candidate.url,
                    &reporter,
                    expected_size,
                    digest.as_deref(),
                )
                .await
            }
        },
        None,
    )
    .await
    .map_err(|error| {
        log::error!("校验文档下载失败：地址={}，错误={}", safe_url, error);
        ConfigError::Read(error)
    })?;
    let byte_count = bytes.len();
    let text = String::from_utf8(bytes).map_err(|error| {
        log::error!("远程文档不是有效 UTF-8：地址={}，原因={}", safe_url, error);
        ConfigError::Parse(error.to_string())
    })?;
    /*
     * 基准用**请求地址**而不是重定向后的地址：Release 资产会跳到签名 CDN，
     * 那里的路径不再有仓库结构，相对子源只会解析到错误位置。真正的规矩是
     * 「Release 资产文档不许声明相对子源」（`resolve_remote_path` 直接报错），
     * 所以这里传什么都一样，传请求地址至少日志可读。
     */
    let result = parse(&text, location);
    match &result {
        Ok(_) => log::info!(
            "校验文档加载完成：地址={}，字节={}，耗时={}ms",
            safe_url,
            byte_count,
            started_at.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "校验文档解析失败：地址={}，字节={}，错误类别={}",
            safe_url,
            byte_count,
            error_category(error)
        ),
    }
    result
}

/// 竞速分支里的单次下载：读字节、报进度、按 API 给的 size/digest 校验。
pub(crate) async fn fetch_document_bytes(
    client: &reqwest::Client,
    url: &str,
    reporter: &crate::archive::DownloadReporter,
    expected_size: Option<u64>,
    digest: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .header("User-Agent", releases::APP_USER_AGENT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache")
        .send_logged()
        .await
        .map_err(|error| error.without_url().to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{} {}", safe_log_address(url), status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_SOURCE_BYTES as u64)
    {
        return Err("source.tooLarge".into());
    }
    let mut content = Vec::new();
    let mut progress = reporter.snapshot();
    progress.total = expected_size.or(response.content_length());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.without_url().to_string())?
    {
        if content.len() + chunk.len() > MAX_SOURCE_BYTES {
            return Err("source.tooLarge".into());
        }
        content.extend_from_slice(&chunk);
        reporter.received(chunk.len() as u64);
        progress.downloaded = content.len() as u64;
        reporter.update(progress.clone());
    }
    /*
     * 与资产下载同一套校验：大小不符 / 摘要不符都说明「下到的不是 API 说的那份」。
     * 只有字节数是真的能对上的时候才比 —— 有的代理会剥掉 Content-Length，
     * 但 `expected_size` 来自 API，恒可比。
     */
    crate::archive::validate_download_bytes(&content, expected_size, digest)?;
    Ok(content)
}
