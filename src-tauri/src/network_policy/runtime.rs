use super::*;
use crate::repository_sources::{SourceLocation, load_text_document};
use std::sync::Arc;

tokio::task_local! { static ACTIVE: Arc<NetworkPolicy>; }

impl NetworkPolicy {
    pub(crate) fn active() -> Result<Arc<Self>, String> {
        ACTIVE
            .try_with(Arc::clone)
            .or_else(|_| Self::shared().map(|policy| Arc::new(policy.clone())))
    }

    pub(crate) fn with_snapshot<T>(policy: Arc<Self>, work: impl FnOnce() -> T) -> T {
        ACTIVE.sync_scope(policy, work)
    }

    pub(crate) async fn in_snapshot<T>(
        policy: Arc<Self>,
        work: impl std::future::Future<Output = T>,
    ) -> T {
        ACTIVE.scope(policy, work).await
    }

    pub(crate) fn select(
        &self,
        bundled_proxies: bool,
        bundled_plans: bool,
    ) -> Result<Self, String> {
        let bundled = Self::shared()?;
        let mut selected = self.clone();
        if bundled_proxies {
            selected.proxies = bundled.proxies.clone();
            selected.proxy_order = bundled.proxy_order.clone();
        }
        if bundled_plans {
            selected.plans = bundled.plans.clone();
            selected.defaults = bundled.defaults.clone();
        }
        for plan in selected.plans.values() {
            validation::plan(&selected, plan)?;
        }
        Ok(selected)
    }

    pub(crate) async fn refresh_remote(
        region: &str,
        bundled_proxies: bool,
        bundled_plans: bool,
    ) -> Result<Self, String> {
        let mut bases = vec![crate::repository_sources::default_index_url(region)];
        if region == crate::config::REGION_CHINESE_MAINLAND {
            bases.push(crate::repository_sources::DEFAULT_INDEX_GITHUB_URL);
        }
        for address in bases {
            let base = SourceLocation::parse(address).map_err(|_| "proxy.validationFailed")?;
            if let Ok(policy) = Self::refresh_from(&base, bundled_proxies, bundled_plans).await {
                return Ok(policy);
            }
        }
        Err("proxy.validationFailed".into())
    }

    pub(crate) async fn refresh_from(
        base: &SourceLocation,
        bundled_proxies: bool,
        bundled_plans: bool,
    ) -> Result<Self, String> {
        async fn read(base: &SourceLocation, path: &str, bundled: bool) -> Result<String, String> {
            let location = if bundled {
                SourceLocation::parse(path)
            } else {
                SourceLocation::resolve(path, Some(base))
            }
            .map_err(|_| "proxy.validationFailed")?;
            if let SourceLocation::Remote(url) = &location {
                if let Some(provider) =
                    crate::download::planning::provider_for_remote_url(url.as_str())
                {
                    let candidates = crate::download::planning::plan_repository_download(
                        &provider,
                        RepositoryKind::Mod,
                        &Default::default(),
                        crate::config::current_user_region(),
                        url.as_str(),
                        Some(crate::github_proxies::RawProxyContext {
                            branch: "",
                            file_path: "",
                        }),
                        None,
                        None,
                    )?
                    .candidates;
                    let verified = crate::repository_sources::VerifiedDocument {
                        candidates,
                        expected_size: None,
                        digest: None,
                    };
                    return crate::repository_sources::load_verified_text_document(
                        &location,
                        &verified,
                        |text, _| {
                            let _: toml::Value = toml::from_str(text).map_err(|_| {
                                crate::config::ConfigError::Invalid("proxy.validationFailed".into())
                            })?;
                            Ok(text.to_owned())
                        },
                    )
                    .await
                    .map_err(|_| "proxy.validationFailed".into());
                }
            }
            load_text_document(&location, |text, _| Ok(text.to_owned()))
                .await
                .map_err(|_| "proxy.validationFailed".into())
        }
        let index = read(base, "proxies.toml", bundled_proxies).await?;
        let mut documents = BTreeMap::new();
        for path in proxy_documents(&index)? {
            documents.insert(path.clone(), read(base, &path, bundled_proxies).await?);
        }
        let plans = read(base, "network-plans.toml", bundled_plans).await?;
        Self::parse(&index, &documents, &plans)
    }
}
