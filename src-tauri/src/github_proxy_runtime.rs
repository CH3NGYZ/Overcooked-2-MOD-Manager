#[cfg(test)]
use crate::download::planning::ordered_asset_download_candidates;
#[cfg(test)]
use crate::download::{DownloadCandidate, DownloadOutput, try_download_candidates};
use crate::github_proxies;
#[cfg(test)]
use crate::models::ProxyMode;
use crate::models::{DataProvider, GitHubProxyConfig};
#[cfg(test)]
use crate::repository_sources::{self, SourceLocation};
use std::sync::RwLock;

/// A process-local snapshot, shared by ConfigService clones but never serialized.
pub(crate) struct ProxyRuntime {
    current: RwLock<RuntimeSnapshot>,
    #[cfg(test)]
    refresh: tokio::sync::Mutex<()>,
    root_refresh: tokio::sync::Mutex<()>,
    source_browser: RwLock<Option<tauri::AppHandle>>,
    selection_changed: tokio::sync::watch::Sender<u64>,
    #[cfg(test)]
    official_addresses: RwLock<Vec<(DataProvider, String)>>,
}

struct RuntimeSnapshot {
    provider: DataProvider,
    revision: u64,
    manual_revision: u64,
    groups: GitHubProxyConfig,
    policy: Option<std::sync::Arc<crate::network_policy::NetworkPolicy>>,
    official_root: Option<crate::repository_sources::RepositorySourceDocument>,
    package: Option<std::sync::Arc<crate::repository_sources::SourcePackage>>,
}

impl Default for ProxyRuntime {
    fn default() -> Self {
        Self::new(DataProvider::default())
    }
}

impl ProxyRuntime {
    pub(crate) fn new(provider: DataProvider) -> Self {
        Self {
            current: RwLock::new(RuntimeSnapshot {
                provider,
                revision: 0,
                manual_revision: 0,
                groups: github_proxies::bundled_defaults(),
                policy: None,
                official_root: None,
                package: None,
            }),
            #[cfg(test)]
            refresh: tokio::sync::Mutex::new(()),
            root_refresh: tokio::sync::Mutex::new(()),
            source_browser: RwLock::new(None),
            selection_changed: tokio::sync::watch::channel(0).0,
            #[cfg(test)]
            official_addresses: RwLock::new(Vec::new()),
        }
    }

    pub(crate) fn provider(&self) -> DataProvider {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .provider
    }

    pub(crate) fn select_provider(&self, provider: DataProvider) {
        let mut snapshot = self.current.write().unwrap_or_else(|e| e.into_inner());
        if snapshot.provider != provider {
            // A different provider starts from the shipped closure. Never expose
            // the previous provider's plans while the new one is being fetched.
            *snapshot = RuntimeSnapshot {
                provider,
                revision: snapshot.revision + 1,
                manual_revision: snapshot.revision + 1,
                groups: github_proxies::bundled_defaults(),
                policy: None,
                official_root: None,
                package: None,
            };
            self.selection_changed.send_replace(snapshot.revision);
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> GitHubProxyConfig {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .groups
            .clone()
    }

    pub(crate) fn configuration_snapshot(
        &self,
    ) -> Result<
        (
            DataProvider,
            GitHubProxyConfig,
            std::sync::Arc<crate::network_policy::NetworkPolicy>,
        ),
        String,
    > {
        let snapshot = self.current.read().unwrap_or_else(|e| e.into_inner());
        let policy = snapshot.policy.clone().map(Ok).unwrap_or_else(|| {
            crate::network_policy::NetworkPolicy::shared()
                .map(|policy| std::sync::Arc::new(policy.clone()))
        })?;
        Ok((snapshot.provider, snapshot.groups.clone(), policy))
    }

    pub(crate) fn set_source_browser(&self, browser: tauri::AppHandle) {
        *self
            .source_browser
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(browser);
    }

    pub(crate) async fn refresh_policy(&self) -> Result<(), String> {
        // The package, MOD root and complete network closure are a single
        // publication. Startup/settings consumers share the same download.
        self.official_root(false).await.map(|_| ())
    }

    #[cfg(test)]
    fn publish_policy(
        &self,
        fresh: crate::network_policy::NetworkPolicy,
        provider: DataProvider,
        revision: u64,
    ) -> Result<(), String> {
        // Parse the complete closure before publishing either view. A late
        // response cannot overwrite a newer selection, including A -> B -> A.
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
            proxy.builtin = false;
            proxy.remote = true;
        }
        let mut snapshot = self.current.write().unwrap_or_else(|e| e.into_inner());
        if snapshot.provider == provider && snapshot.revision == revision {
            snapshot.groups = groups;
            snapshot.policy = Some(std::sync::Arc::new(fresh));
        }
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
    let github = SourceLocation::parse(&DataProvider::Github.document("mods.toml"))
        .map_err(|_| "proxy.validationFailed")?;
    let github = SourceLocation::resolve("repositories-proxies.toml", Some(&github))
        .map_err(|_| "proxy.validationFailed")?
        .address();
    let proxies = github_proxies::proxy_urls(&snapshot.raw_proxies, &github, true, None);
    let mut routes = Vec::new();
    if region == crate::config::REGION_CHINESE_MAINLAND {
        let primary =
            SourceLocation::parse(&DataProvider::for_region(region).document("mods.toml"))
                .map_err(|_| "proxy.validationFailed")?;
        let primary = SourceLocation::resolve("repositories-proxies.toml", Some(&primary))
            .map_err(|_| "proxy.validationFailed")?
            .address();
        routes.push(ordered_asset_download_candidates(
            ProxyMode::Direct,
            &primary,
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

mod official_root;
#[cfg(test)]
mod test_support;
pub(crate) use official_root::OfficialRootSnapshot;
#[cfg(test)]
#[path = "github_proxy_runtime_tests.rs"]
mod tests;
