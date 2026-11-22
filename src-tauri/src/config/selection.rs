//! 持久地区与进程数据来源的保存边界。

use super::{ConfigError, ConfigService, normalize_repository_region};
use crate::models::{AppConfig, DataProvider};

impl ConfigService {
    /// A region change restarts automatic selection, even when the default
    /// provider happens to be the current manual provider. Commit runtime state
    /// only after disk succeeds so a rejected save cannot cancel active work.
    pub(crate) fn save_with_selection(
        &self,
        config: &mut AppConfig,
        region: Option<&str>,
        provider: Option<DataProvider>,
    ) -> Result<(), ConfigError> {
        let region = region
            .map(|value| {
                let normalized = normalize_repository_region(Some(value));
                if normalized.is_empty() {
                    log::warn!("[config] 拒绝保存地区和来源：原因=config.invalidRegion");
                    Err(ConfigError::Invalid("config.invalidRegion".into()))
                } else {
                    Ok(normalized)
                }
            })
            .transpose()?;
        let guard = self
            .storage_lock
            .lock()
            .map_err(|_| ConfigError::Write("lock.poisoned".into()))?;
        let mut selected = self.with_runtime(config.clone());
        let previous_provider = selected.data_provider;
        let changed_region = region.filter(|region| region != &selected.region);
        if let Some(region) = &changed_region {
            selected.region.clone_from(region);
            selected.data_provider = DataProvider::for_region(region);
        } else if let Some(provider) = provider {
            selected.data_provider = provider;
        }
        if changed_region.is_some() || provider.is_some() {
            super::synchronize_builtin_repository_sources(&mut selected);
        }
        self.save_locked(&selected, &guard).inspect_err(|error| {
            if changed_region.is_some() || provider.is_some() {
                log::warn!(
                    "[config] 选择未生效，保留运行时来源：当前={previous_provider:?}，请求来源={:?}，请求地区={}，原因={}",
                    selected.data_provider, selected.region, error.diagnostic_code()
                );
            }
        })?;
        if changed_region.is_some() || provider.is_some() {
            self.proxies
                .select_provider(selected.data_provider, Some(&selected.region));
            log::info!(
                "[config] 已保存会话选择：原来源={previous_provider:?}，新来源={:?}，地区={}，重新按地区选择={}",
                selected.data_provider,
                selected.region,
                changed_region.is_some()
            );
        }
        *config = self.with_runtime(selected);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
