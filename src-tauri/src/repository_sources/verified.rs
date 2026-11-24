use super::SourceLocation;
use super::diagnostics::safe_log_address;
use super::transport::{document_client, download_document, parse_remote_document};
use crate::config::ConfigError;
use crate::download::planning::RepositoryDownloadPlan;
use crate::download::{DownloadContext, DownloadOutput};
use crate::models::NetworkConfig;

#[derive(Default)]
pub(crate) struct VerifiedDocument {
    pub expected_size: Option<u64>,
    pub digest: Option<String>,
}

struct LoadedDocument<T>(T);

impl<T> DownloadOutput for LoadedDocument<T> {
    fn discard(&self) {}
}

pub(crate) async fn load_verified_text_document<T>(
    location: &SourceLocation,
    plan: RepositoryDownloadPlan,
    verified: &VerifiedDocument,
    network: &NetworkConfig,
    parse: impl Fn(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let client = document_client(network)?;
    let timeout = network.api_timeout();
    let parse = &parse;
    let LoadedDocument(document) = crate::download::try_download_candidates(
        plan.candidates,
        || false,
        |candidate, reporter| {
            let client = client.clone();
            async move {
                let (bytes, _) =
                    download_document(&client, &candidate.url, timeout, Some(&reporter))
                        .await
                        .map_err(|error| error.to_string())?;
                crate::archive::validate_download_bytes(
                    &bytes,
                    verified.expected_size,
                    verified.digest.as_deref(),
                )?;
                parse_remote_document(&candidate.url, bytes, location, parse)
                    .map(LoadedDocument)
                    .map_err(|error| error.to_string())
            }
        },
        network,
        Some(DownloadContext {
            execution: Some(plan.execution),
            label: "repository-source-document",
            file_name: None,
            expected_size: verified.expected_size,
            race_session: None,
        }),
    )
    .await
    .map_err(|error| {
        log::warn!(
            "文档候选均失败：地址={}，错误类别={}",
            safe_log_address(&location.address()),
            crate::locale_tag::diagnostic_tag(&error)
        );
        ConfigError::Read(error)
    })?;
    Ok(document)
}

#[cfg(test)]
pub(crate) async fn fetch_document_bytes(
    client: &reqwest::Client,
    url: &str,
    reporter: &crate::archive::DownloadReporter,
    expected_size: Option<u64>,
    digest: Option<&str>,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    let (bytes, _) = download_document(client, url, timeout, Some(reporter))
        .await
        .map_err(|error| error.to_string())?;
    crate::archive::validate_download_bytes(&bytes, expected_size, digest)?;
    Ok(bytes)
}
