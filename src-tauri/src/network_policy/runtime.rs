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

    pub(crate) async fn refresh_remote(
        provider: crate::models::DataProvider,
    ) -> Result<Self, String> {
        let base = SourceLocation::parse(&provider.document("mods.toml"))
            .map_err(|_| "proxy.validationFailed")?;
        Self::refresh_from(&base).await
    }

    pub(crate) async fn refresh_from(base: &SourceLocation) -> Result<Self, String> {
        async fn read(base: &SourceLocation, path: &str) -> Result<String, String> {
            let location =
                SourceLocation::resolve(path, Some(base)).map_err(|_| "proxy.validationFailed")?;
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
        let index = read(base, "proxies.toml").await?;
        let mut documents = BTreeMap::new();
        for path in proxy_documents(&index)? {
            documents.insert(path.clone(), read(base, &path).await?);
        }
        let plans = read(base, "network-plans.toml").await?;
        Self::parse(&index, &documents, &plans)
    }
}
