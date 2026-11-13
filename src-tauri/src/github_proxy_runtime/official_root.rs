use super::{ProxyRuntime, RuntimeSnapshot};
use crate::models::DataProvider;
use crate::repository_sources::{RepositorySourceDocument, SourceLocation, load_document};

#[derive(Clone)]
pub(crate) struct OfficialRootSnapshot {
    pub(crate) selection: (DataProvider, u64),
    pub(crate) document: RepositorySourceDocument,
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
    pub(crate) async fn in_selection<T>(
        &self,
        selection: (DataProvider, u64),
        work: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let mut changed = self.selection_changed.subscribe();
        if self.selection() != selection {
            return Err("source.addressMismatch".into());
        }
        tokio::select! {
            biased;
            _ = changed.changed() => Err("source.addressMismatch".into()),
            result = work => {
                if self.selection() != selection { Err("source.addressMismatch".into()) } else { result }
            }
        }
    }

    pub(crate) async fn official_root(&self, force: bool) -> Result<OfficialRootSnapshot, String> {
        self.official_root_from(force, |provider| self.official_address(provider))
            .await
    }

    pub(super) fn official_address(&self, provider: DataProvider) -> String {
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
        provider.document("mods.toml")
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
        let selection = self.selection();
        // Builtin selection is immediate even while an old remote holds the gate.
        if selection.0 == DataProvider::Builtin {
            let location = SourceLocation::parse("mods.toml").map_err(|e| e.to_string())?;
            let document = load_document(&location, &Default::default())
                .await
                .map_err(|e| e.to_string())?;
            return Ok(OfficialRootSnapshot {
                selection,
                document,
            });
        }
        let _guard = self
            .in_selection(selection, async { Ok(self.root_refresh.lock().await) })
            .await?;
        let mut expected = selection;
        loop {
            if self.selection() != expected {
                return Err("source.addressMismatch".into());
            }
            if !force {
                let snapshot = self.current.read().unwrap_or_else(|e| e.into_inner());
                if let Some(document) = snapshot.official_root.clone() {
                    return Ok(OfficialRootSnapshot {
                        selection: expected,
                        document,
                    });
                }
            }
            let location =
                SourceLocation::parse(&address(expected.0)).map_err(|e| e.to_string())?;
            let fetched = self
                .in_selection(expected, async {
                    load_document(&location, &Default::default())
                        .await
                        .map_err(|e| e.to_string())
                })
                .await;
            let mut snapshot = self.current.write().unwrap_or_else(|e| e.into_inner());
            if (snapshot.provider, snapshot.revision) != expected {
                return Err("source.addressMismatch".into());
            }
            match fetched {
                Ok(document) => {
                    snapshot.official_root = Some(document.clone());
                    return Ok(OfficialRootSnapshot {
                        selection: expected,
                        document,
                    });
                }
                Err(error) => {
                    let Some(provider) = expected.0.fallback() else {
                        return Err(error);
                    };
                    log::warn!(
                        "官方 MOD 根文档不可用，切换数据来源：{:?} → {:?}",
                        expected.0,
                        provider
                    );
                    *snapshot = RuntimeSnapshot {
                        provider,
                        revision: expected.1 + 1,
                        manual_revision: snapshot.manual_revision,
                        groups: crate::github_proxies::bundled_defaults(),
                        policy: None,
                        official_root: None,
                    };
                    expected = (provider, snapshot.revision);
                    self.selection_changed.send_replace(snapshot.revision);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
