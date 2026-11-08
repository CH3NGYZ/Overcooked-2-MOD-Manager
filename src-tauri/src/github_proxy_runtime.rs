#[cfg(test)]
use crate::download::planning::ordered_asset_download_candidates;
#[cfg(test)]
use crate::download::{DownloadCandidate, DownloadOutput, try_download_candidates};
use crate::github_proxies;
use crate::models::GitHubProxyConfig;
#[cfg(test)]
use crate::models::ProxyMode;
#[cfg(test)]
use crate::repository_sources::{self, SourceLocation};
use std::sync::RwLock;

/// A process-local snapshot, shared by ConfigService clones but never serialized.
pub(crate) struct ProxyRuntime {
    current: RwLock<RuntimeSnapshot>,
    refresh: tokio::sync::Mutex<()>,
}

struct RuntimeSnapshot {
    groups: GitHubProxyConfig,
    policy: Option<std::sync::Arc<crate::network_policy::NetworkPolicy>>,
}

impl Default for ProxyRuntime {
    fn default() -> Self {
        Self {
            current: RwLock::new(RuntimeSnapshot {
                groups: github_proxies::bundled_defaults(),
                policy: None,
            }),
            refresh: tokio::sync::Mutex::new(()),
        }
    }
}

impl ProxyRuntime {
    pub(crate) fn snapshot(&self) -> GitHubProxyConfig {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .groups
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn selected_policy(
        &self,
        proxies: bool,
        plans: bool,
    ) -> Result<std::sync::Arc<crate::network_policy::NetworkPolicy>, String> {
        let policy = self
            .current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .clone();
        let current = policy
            .map(Ok)
            .unwrap_or_else(crate::network_policy::NetworkPolicy::active)?;
        current.select(proxies, plans).map(std::sync::Arc::new)
    }

    pub(crate) fn configuration_snapshot(
        &self,
        proxies: bool,
        plans: bool,
    ) -> Result<
        (
            GitHubProxyConfig,
            std::sync::Arc<crate::network_policy::NetworkPolicy>,
        ),
        String,
    > {
        let snapshot = self.current.read().unwrap_or_else(|e| e.into_inner());
        let bundled = crate::network_policy::NetworkPolicy::shared()?;
        let selected = snapshot
            .policy
            .as_deref()
            .unwrap_or(bundled)
            .select(proxies, plans)?;
        let mut groups = if snapshot.policy.is_some() {
            selected.bundled_proxy_groups()?
        } else if proxies {
            github_proxies::bundled_defaults()
        } else {
            snapshot.groups.clone()
        };
        if snapshot.policy.is_some() {
            for entry in groups
                .release_proxies
                .iter_mut()
                .chain(groups.raw_proxies.iter_mut())
                .chain(groups.github_api_proxies.iter_mut())
                .chain(groups.gitee_api_proxies.iter_mut())
                .chain(groups.gitcode_api_proxies.iter_mut())
                .chain(groups.avatar_proxies.iter_mut())
            {
                entry.builtin = proxies;
                entry.remote = !proxies;
            }
        }
        Ok((groups, std::sync::Arc::new(selected)))
    }

    pub(crate) async fn refresh_policy(
        &self,
        region: &str,
        proxies: bool,
        plans: bool,
    ) -> Result<(), String> {
        let _guard = self.refresh.lock().await;
        let fresh =
            crate::network_policy::NetworkPolicy::refresh_remote(region, proxies, plans).await?;
        self.publish_policy(fresh, proxies)
    }

    fn publish_policy(
        &self,
        fresh: crate::network_policy::NetworkPolicy,
        proxies: bool,
    ) -> Result<(), String> {
        // Parse the complete closure before publishing either view. No config or
        // business lock is held while documents are being downloaded.
        // Every switch combination must remain usable after publication.
        for proxies in [false, true] {
            for plans in [false, true] {
                fresh.select(proxies, plans)?;
            }
        }
        let mut groups = fresh.bundled_proxy_groups()?;
        for proxy in groups
            .release_proxies
            .iter_mut()
            .chain(groups.raw_proxies.iter_mut())
            .chain(groups.github_api_proxies.iter_mut())
            .chain(groups.gitee_api_proxies.iter_mut())
            .chain(groups.gitcode_api_proxies.iter_mut())
            .chain(groups.avatar_proxies.iter_mut())
        {
            proxy.builtin = proxies;
            proxy.remote = !proxies;
        }
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = RuntimeSnapshot {
            groups,
            policy: Some(std::sync::Arc::new(fresh)),
        };
        Ok(())
    }

    #[cfg(test)]
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

    #[cfg(test)]
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
                    .chain(config.gitcode_api_proxies.iter_mut())
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
            "线上代理清单整份切换：Release={}，Raw={}，GitHub API={}，Gitee API={}，GitCode API={}，头像={}；不写磁盘",
            config.release_proxies.len(),
            config.raw_proxies.len(),
            config.github_api_proxies.len(),
            config.gitee_api_proxies.len(),
            config.gitcode_api_proxies.len(),
            config.avatar_proxies.len()
        );
        self.current
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .groups = config;
        Ok(())
    }
}

#[cfg(test)]
impl DownloadOutput for GitHubProxyConfig {
    fn discard(&self) {}
}

#[cfg(test)]
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
mod test_support;
#[cfg(test)]
#[path = "github_proxy_runtime_tests.rs"]
mod tests;
