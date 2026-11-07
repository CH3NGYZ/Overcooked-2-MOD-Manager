//! 按仓库汇总已安装 MOD 的更新状态。
use crate::models::*;
use crate::mods;

pub(crate) fn collect_update_status(
    config: &AppConfig,
    installed: &[InstalledMod],
) -> Vec<UpdateStatus> {
    config
        .repositories
        .iter()
        // 不可用的仓库不产生更新状态：它不刷新，缓存的 Release 也就不再可信
        .filter(|repo| {
            crate::config::repository_is_available(
                config,
                repo,
                crate::config::current_user_region(),
            )
        })
        .map(|repo| {
            let local_mod = installed.iter().find(|item| {
                (repo.download_mode == RepositoryDownloadMode::RepositoryFile
                    || (item.status != "not_release_asset"
                        && item.status != "duplicate_lower_version"))
                    && (item.repository_ids.iter().any(|id| id == &repo.id)
                        || item.repository_id.as_deref() == Some(repo.id.as_str()))
            });
            let local = local_mod.and_then(mods::local_update_version);
            if repo.download_mode == RepositoryDownloadMode::RepositoryFile {
                return UpdateStatus {
                    repository_id: repo.id.clone(),
                    local_version: local,
                    remote_version: None,
                    state: "not_release_asset".into(),
                };
            }
            if repo.last_error.is_some() {
                return UpdateStatus {
                    repository_id: repo.id.clone(),
                    local_version: local,
                    remote_version: None,
                    state: "repository_unavailable".into(),
                };
            }
            let remote = mods::remote_update_version(repo, local_mod);
            let state = local_mod
                .map(|installed| mods::mod_update_state(repo, installed).1)
                .unwrap_or("unknown");
            UpdateStatus {
                repository_id: repo.id.clone(),
                local_version: local,
                remote_version: remote,
                state: state.into(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
