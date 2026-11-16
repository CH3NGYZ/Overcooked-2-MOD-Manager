use super::{ProxyRuntime, RuntimeSnapshot};
use crate::models::DataProvider;
use crate::repository_sources::{
    RepositorySourceDocument, SourceLocation, SourcePackage, load_document,
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct OfficialRootSnapshot {
    pub(crate) selection: (DataProvider, u64),
    pub(crate) document: RepositorySourceDocument,
    // Production readers capture the package through in_selection; tests also
    // inspect the returned Arc to verify reuse and generation isolation.
    #[cfg(test)]
    pub(crate) package: Arc<SourcePackage>,
}

impl ProxyRuntime {
    pub(crate) fn selection_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.selection_changed.subscribe()
    }

    pub(crate) fn selection(&self) -> (DataProvider, u64) {
        let snapshot = self.current.read().unwrap_or_else(|e| e.into_inner());
        (snapshot.provider, snapshot.revision)
    }

    /// Dropping an overtaken request cancels its HTTP future as well as lock waits.
    pub(crate) fn in_selection<T>(
        &self,
        selection: (DataProvider, u64),
        work: impl std::future::Future<Output = Result<T, String>>,
    ) -> impl std::future::Future<Output = Result<T, String>> {
        // Keep the scope's unpolled state small, including when its caller is
        // another async wrapper. Cancellation still drops the original work.
        let work = Box::pin(work);
        let package = self
            .current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .package
            .clone();
        async move {
            let mut changed = self.selection_changed.subscribe();
            if self.selection() != selection {
                return Err("source.addressMismatch".into());
            }
            tokio::select! {
                biased;
                _ = changed.changed() => Err("source.addressMismatch".into()),
                result = async move {
                    if let Some(package) = package {
                        SourcePackage::in_snapshot(package, work).await
                    } else {
                        work.await
                    }
                } => {
                    if self.selection() != selection { Err("source.addressMismatch".into()) } else { result }
                }
            }
        }
    }

    pub(crate) fn official_root(
        &self,
        force: bool,
    ) -> impl std::future::Future<Output = Result<OfficialRootSnapshot, String>> {
        // The builtin branch bypasses in_selection; bound the complete root
        // task here so neither branch propagates its size into release IPC.
        Box::pin(self.official_root_from(force, move |provider| self.archive_address(provider)))
    }

    fn archive_address(&self, provider: DataProvider) -> String {
        #[cfg(test)]
        if let Some((_, address)) = self
            .official_addresses
            .read()
            .unwrap()
            .iter()
            .find(|(key, _)| *key == provider)
        {
            return address.clone();
        }
        provider.archive_url().unwrap_or_default().to_owned()
    }

    async fn official_root_from(
        &self,
        force: bool,
        address: impl Fn(DataProvider) -> String,
    ) -> Result<OfficialRootSnapshot, String> {
        let manual_revision = self
            .current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .manual_revision;
        let mut force = force;
        loop {
            let result = self.load_official_root(force, &address).await;
            // Parallel startup consumers follow the gate's own fallback. A user
            // selection cancels them, even if it returns to the same provider.
            if result
                .as_ref()
                .err()
                .is_some_and(|error| error == "source.addressMismatch")
                && self
                    .current
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .manual_revision
                    == manual_revision
            {
                force = false;
                continue;
            }
            return result;
        }
    }

    async fn load_official_root(
        &self,
        force: bool,
        address: &impl Fn(DataProvider) -> String,
    ) -> Result<OfficialRootSnapshot, String> {
        let (selection, manual_revision, region) = {
            let snapshot = self.current.read().unwrap_or_else(|e| e.into_inner());
            (
                (snapshot.provider, snapshot.revision),
                snapshot.manual_revision,
                snapshot.region.clone(),
            )
        };
        // Builtin remains immediate when an obsolete HTTP request holds the
        // remote gate. Force callers on the same generation coalesce too.
        let _guard = if selection.0 == DataProvider::Builtin {
            None
        } else {
            Some(
                self.in_selection(selection, async { Ok(self.root_refresh.lock().await) })
                    .await?,
            )
        };
        let mut expected = selection;
        loop {
            {
                let snapshot = self.current.read().unwrap_or_else(|e| e.into_inner());
                if snapshot.manual_revision != manual_revision {
                    return Err("source.addressMismatch".into());
                }
                if (!force || (snapshot.provider, snapshot.revision) != selection)
                    && let (Some(document), Some(_package)) =
                        (&snapshot.official_root, &snapshot.package)
                {
                    return Ok(OfficialRootSnapshot {
                        selection: (snapshot.provider, snapshot.revision),
                        document: document.clone(),
                        #[cfg(test)]
                        package: _package.clone(),
                    });
                }
                if (snapshot.provider, snapshot.revision) != expected {
                    return Err("source.addressMismatch".into());
                }
            }
            let browser = self
                .source_browser
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let fetched = self
                .in_selection(
                    expected,
                    prepare_package(expected.0, address(expected.0), &region, browser),
                )
                .await;
            let mut snapshot = self.current.write().unwrap_or_else(|e| e.into_inner());
            if (snapshot.provider, snapshot.revision) != expected {
                return Err("source.addressMismatch".into());
            }
            match fetched {
                Ok((document, package, policy)) => {
                    let mut groups = policy.bundled_proxy_groups()?;
                    if expected.0 != DataProvider::Builtin {
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
                    }
                    // Replacing a package invalidates in-flight work even if
                    // the provider name did not change. Readers capture an Arc
                    // and can never mix children from two source generations.
                    snapshot.revision += 1;
                    snapshot.official_root = Some(document.clone());
                    snapshot.package = Some(package.clone());
                    snapshot.policy = Some(Arc::new(policy));
                    snapshot.groups = groups;
                    self.selection_changed.send_replace(snapshot.revision);
                    log::info!(
                        "官方源码包快照就绪：来源={:?}，代次={}",
                        snapshot.provider,
                        snapshot.revision
                    );
                    return Ok(OfficialRootSnapshot {
                        selection: (snapshot.provider, snapshot.revision),
                        document,
                        #[cfg(test)]
                        package,
                    });
                }
                Err(error) => {
                    let Some(provider) = expected.0.fallback() else {
                        return Err(error);
                    };
                    let reason = error
                        .split('?')
                        .next()
                        .filter(|tag| {
                            tag.starts_with("source.")
                                && tag
                                    .bytes()
                                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.')
                        })
                        .unwrap_or("invalid_package");
                    log::warn!(
                        "官方源码包不可用，切换数据来源：{:?} → {:?}，原因标签={}",
                        expected.0,
                        provider,
                        reason
                    );
                    *snapshot = RuntimeSnapshot {
                        provider,
                        region: snapshot.region.clone(),
                        revision: expected.1 + 1,
                        manual_revision: snapshot.manual_revision,
                        groups: crate::github_proxies::bundled_defaults(),
                        policy: None,
                        official_root: None,
                        package: None,
                    };
                    expected = (provider, snapshot.revision);
                    self.selection_changed.send_replace(snapshot.revision);
                }
            }
        }
    }
}

async fn prepare_package(
    provider: DataProvider,
    address: String,
    region: &str,
    browser: Option<tauri::AppHandle>,
) -> Result<
    (
        RepositorySourceDocument,
        Arc<SourcePackage>,
        crate::network_policy::NetworkPolicy,
    ),
    String,
> {
    let package = if provider == DataProvider::Builtin {
        SourcePackage::bundled()
    } else {
        crate::repository_sources::fetch_source_package(provider, &address, browser.as_ref())
            .await?
    };
    // A valid package owns all four entry documents. Missing children remain
    // branch errors; they never fall through to another package's resources.
    for path in [
        "mods.toml",
        "maps.toml",
        "proxies.toml",
        "network-plans.toml",
    ] {
        package.read(path).map_err(|error| error.to_string())?;
    }
    let location =
        SourceLocation::parse(&provider.document("mods.toml")).map_err(|e| e.to_string())?;
    let (document, policy) = SourcePackage::in_snapshot(package.clone(), async {
        let policy = crate::network_policy::NetworkPolicy::refresh_from(&location, region).await?;
        let document =
            crate::network_policy::NetworkPolicy::in_snapshot(Arc::new(policy.clone()), async {
                load_document(&location, &Default::default())
                    .await
                    .map_err(|e| e.to_string())
            })
            .await?;
        Ok::<_, String>((document, policy))
    })
    .await?;
    Ok((document, package, policy))
}

#[cfg(test)]
mod task_tests;
#[cfg(test)]
mod tests;
