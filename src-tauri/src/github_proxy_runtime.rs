use crate::download::planning::ordered_asset_download_candidates;
use crate::download::{DownloadCandidate, DownloadOutput, try_download_candidates};
use crate::github_proxies;
use crate::models::{GitHubProxyConfig, ProxyMode};
use crate::repository_sources::{self, SourceLocation};
use std::sync::RwLock;

/// A process-local snapshot, shared by ConfigService clones but never serialized.
pub(crate) struct ProxyRuntime {
    current: RwLock<GitHubProxyConfig>,
    refresh: tokio::sync::Mutex<()>,
}

impl Default for ProxyRuntime {
    fn default() -> Self {
        Self {
            current: RwLock::new(github_proxies::bundled_defaults()),
            refresh: tokio::sync::Mutex::new(()),
        }
    }
}

impl ProxyRuntime {
    pub(crate) fn snapshot(&self) -> GitHubProxyConfig {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) async fn refresh(&self, region: &str) -> Result<(), String> {
        // Only refreshes serialize here; readers and unrelated settings saves never wait on HTTP.
        let _guard = self.refresh.lock().await;
        let client = crate::releases::build_client(repository_sources::SOURCE_DOCUMENT_TIMEOUT)
            .map_err(|_| "proxy.validationFailed")?;
        let mut last_error = "proxy.validationFailed".to_string();
        for candidates in manifest_routes(region, &self.snapshot())? {
            match self.refresh_from(&client, candidates).await {
                Ok(()) => return Ok(()),
                Err(error) => last_error = error,
            }
        }
        log::warn!("线上代理清单读取失败，保留本进程当前快照");
        Err(last_error)
    }

    async fn refresh_from(
        &self,
        client: &reqwest::Client,
        candidates: Vec<DownloadCandidate>,
    ) -> Result<(), String> {
        let config = try_download_candidates(
            candidates,
            || false,
            |candidate, reporter| async move {
                let bytes = repository_sources::fetch_document_bytes(
                    client,
                    &candidate.url,
                    &reporter,
                    None,
                    None,
                )
                .await?;
                let text = std::str::from_utf8(&bytes).map_err(|_| "proxy.invalidUtf8")?;
                // Validate before declaring a winner: a fast HTML/error response must not
                // cancel another route that can return the actual manifest.
                let mut config =
                    github_proxies::parse_manifest(text).map_err(|_| "proxy.validationFailed")?;
                for proxy in config
                    .release_proxies
                    .iter_mut()
                    .chain(config.raw_proxies.iter_mut())
                    .chain(config.github_api_proxies.iter_mut())
                    .chain(config.gitee_api_proxies.iter_mut())
                    .chain(config.avatar_proxies.iter_mut())
                {
                    proxy.builtin = false;
                    proxy.remote = true;
                }
                Ok(config)
            },
            None,
        )
        .await?;
        log::info!(
            "线上代理清单整份切换：Release={}，Raw={}，GitHub API={}，Gitee API={}，头像={}；不写磁盘",
            config.release_proxies.len(),
            config.raw_proxies.len(),
            config.github_api_proxies.len(),
            config.gitee_api_proxies.len(),
            config.avatar_proxies.len()
        );
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = config;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn replace_for_test(&self, config: GitHubProxyConfig) {
        *self.current.write().unwrap() = config;
    }
}

impl DownloadOutput for GitHubProxyConfig {
    fn discard(&self) {}
}

fn manifest_routes(
    region: &str,
    snapshot: &GitHubProxyConfig,
) -> Result<Vec<Vec<DownloadCandidate>>, String> {
    let github = SourceLocation::parse(repository_sources::DEFAULT_INDEX_GITHUB_URL)
        .map_err(|_| "proxy.validationFailed")?;
    let github = SourceLocation::resolve("repositories-proxies.toml", Some(&github))
        .map_err(|_| "proxy.validationFailed")?
        .address();
    let proxies = github_proxies::proxy_urls(&snapshot.raw_proxies, &github, true, None);
    let mut routes = Vec::new();
    if region == crate::config::REGION_CHINESE_MAINLAND {
        let gitee = SourceLocation::parse(repository_sources::default_index_url(region))
            .map_err(|_| "proxy.validationFailed")?;
        let gitee = SourceLocation::resolve("repositories-proxies.toml", Some(&gitee))
            .map_err(|_| "proxy.validationFailed")?
            .address();
        routes.push(ordered_asset_download_candidates(
            ProxyMode::Direct,
            &gitee,
            Vec::new(),
        ));
    }
    routes.push(ordered_asset_download_candidates(
        ProxyMode::AutoDirect,
        &github,
        proxies,
    ));
    Ok(routes)
}

#[cfg(test)]
#[path = "github_proxy_runtime_tests.rs"]
mod tests;
