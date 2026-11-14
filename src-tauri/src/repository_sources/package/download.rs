use super::{MAX_ARCHIVE_BYTES, SourcePackage, invalid, too_large};
use crate::config::ConfigError;
use crate::http_diagnostics::HttpRequestExt;
use crate::models::DataProvider;
use crate::repository_sources::SourceLocation;
use std::sync::Arc;

const USER_AGENT: &str = concat!("Overcooked 2 MOD Manager/", env!("CARGO_PKG_VERSION"));

pub(crate) async fn fetch(
    provider: DataProvider,
    archive_address: &str,
    browser: Option<&tauri::AppHandle>,
) -> Result<Arc<SourcePackage>, String> {
    if provider == DataProvider::Builtin {
        return Ok(SourcePackage::bundled());
    }
    let bytes = if provider == DataProvider::Gitee {
        browser_bytes(browser, archive_address, provider).await?
    } else {
        match Box::pin(fetch_archive(archive_address)).await {
            Ok(bytes) => bytes,
            Err(ArchiveError::Verification) if provider == DataProvider::Gitcode => {
                browser_bytes(browser, archive_address, provider).await?
            }
            Err(ArchiveError::Verification) => {
                return Err(invalid("Source archive response is HTML").to_string());
            }
            Err(ArchiveError::Request(error)) => return Err(error.to_string()),
        }
    };
    tokio::task::spawn_blocking(move || SourcePackage::from_zip(&bytes).map(Arc::new))
        .await
        .map_err(|_| invalid("Source archive worker failed").to_string())?
        .map_err(|error| error.to_string())
}

async fn browser_bytes(
    browser: Option<&tauri::AppHandle>,
    address: &str,
    provider: DataProvider,
) -> Result<Vec<u8>, String> {
    let app = browser.ok_or("source.webDownloadFailed")?;
    let url = url::Url::parse(address).map_err(|_| "source.webDownloadAddressInvalid")?;
    let (title, hosts) = match provider {
        DataProvider::Gitee => ("Gitee", vec!["gitee.com"]),
        DataProvider::Gitcode => (
            "GitCode",
            vec!["gitcode.com", "huaweicloud.com", "myhuaweicloud.com"],
        ),
        _ => return Err("source.webDownloadAddressInvalid".into()),
    };
    log::info!("数据来源需要网页验证：来源={provider:?}；关闭验证窗口将尝试下一来源");
    crate::webview_download::download(
        app,
        crate::webview_download::Request {
            url,
            user_agent: USER_AGENT.to_owned(),
            max_bytes: MAX_ARCHIVE_BYTES,
            title: title.to_owned(),
            timeout: std::time::Duration::from_secs(120),
            allowed_download_hosts: hosts.into_iter().map(str::to_owned).collect(),
            allowed_download_paths: if provider == DataProvider::Gitee {
                vec!["/ch3ngyz/Overcooked-2-MOD-Manager-Repos/repository/download".to_owned()]
            } else {
                Vec::new()
            },
        },
    )
    .await
}

enum ArchiveError {
    Verification,
    Request(ConfigError),
}

impl From<ConfigError> for ArchiveError {
    fn from(error: ConfigError) -> Self {
        Self::Request(error)
    }
}

async fn fetch_archive(address: &str) -> Result<Vec<u8>, ArchiveError> {
    let SourceLocation::Remote(url) = SourceLocation::parse(address)? else {
        return Err(ConfigError::Invalid("source.mustBeHttp".into()).into());
    };
    // This client has no API authorization and uses no download race. The
    // request timeout also bounds the complete streamed response body.
    let client = crate::releases::build_client(super::super::transport::SOURCE_DOCUMENT_TIMEOUT)
        .map_err(|error| ConfigError::Read(error.to_string()))?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache")
        .send_logged()
        .await
        .map_err(|error| ConfigError::Read(error.without_url().to_string()))?;
    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/html"))
    {
        return Err(ArchiveError::Verification);
    }
    if !response.status().is_success() {
        return Err(ConfigError::Read(format!("HTTP {}", response.status().as_u16())).into());
    }
    let bytes = response_bytes(response, MAX_ARCHIVE_BYTES).await?;
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let prefix = prefix
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    if prefix.starts_with("<!doctype html") || prefix.starts_with("<html") {
        return Err(ArchiveError::Verification);
    }
    Ok(bytes)
}

pub(super) async fn response_bytes(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ConfigError> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ConfigError::Read(error.without_url().to_string()))?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
