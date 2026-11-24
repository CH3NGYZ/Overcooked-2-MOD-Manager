use super::*;
use crate::repository_sources::{SourceLocation, load_text_document};
use std::sync::Arc;

#[cfg(test)]
mod tests;

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

    // Box before returning a future: an async fn would still capture the full
    // work in its initial state and propagate it through Tauri's release IPC.
    pub(crate) fn in_snapshot<T>(
        policy: Arc<Self>,
        work: impl std::future::Future<Output = T>,
    ) -> impl std::future::Future<Output = T> {
        ACTIVE.scope(policy, Box::pin(work))
    }

    pub(crate) async fn refresh_from(
        base: &SourceLocation,
        region: &str,
        network: &crate::models::NetworkConfig,
    ) -> Result<Self, String> {
        async fn read(
            base: &SourceLocation,
            path: &str,
            region: &str,
            network: &crate::models::NetworkConfig,
        ) -> Result<String, String> {
            let location =
                SourceLocation::resolve(path, Some(base)).map_err(|_| "proxy.validationFailed")?;
            if crate::repository_sources::package_document_path(&location).is_none()
                && let SourceLocation::Remote(url) = &location
            {
                if let Some(provider) =
                    crate::download::planning::provider_for_remote_url(url.as_str())
                {
                    let plan = crate::download::planning::plan_source_document(
                        &provider,
                        RepositoryKind::Mod,
                        &Default::default(),
                        region,
                        url.as_str(),
                        true,
                    )?;
                    return crate::repository_sources::load_verified_text_document(
                        &location,
                        plan,
                        &Default::default(),
                        network,
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
            load_text_document(&location, network, |text, _| Ok(text.to_owned()))
                .await
                .map_err(|_| "proxy.validationFailed".into())
        }
        let index = read(base, "proxies.toml", region, network).await?;
        let mut documents = BTreeMap::new();
        for path in proxy_documents(&index)? {
            documents.insert(path.clone(), read(base, &path, region, network).await?);
        }
        let plans = read(base, "network-plans.toml", region, network).await?;
        Self::parse(&index, &documents, &plans)
    }
}
