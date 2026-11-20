//! 本地版本、绑定 Release 与卡片标签；按仓库汇总沿用同一判定。
use crate::models::*;
use std::cmp::Ordering;

pub(crate) fn collect_update_status(
    config: &AppConfig,
    installed: &[InstalledMod],
) -> Vec<UpdateStatus> {
    config
        .repositories
        .iter()
        // 不可用的仓库不产生更新状态：它不刷新，缓存的 Release 也就不再可信
        .filter(|repo| crate::config::mod_repository_is_available(config, repo, &config.region))
        .map(|repo| {
            let local_mod = installed
                .iter()
                .filter(|item| {
                    (repo.download_mode == ModDownloadMode::RepositoryFile
                        || (item.status != "not_release_asset"
                            && !item.is_dependency
                            && !item.release_companion
                            && !item.duplicate_cleanup_available
                            && item.status != "duplicate_lower_version"))
                        && (item.repository_ids.iter().any(|id| id == &repo.id)
                            || item.repository_id.as_deref() == Some(repo.id.as_str()))
                })
                .reduce(|best, item| {
                    let compared = local_update_version(item)
                        .zip(local_update_version(best))
                        .and_then(|(item, best)| {
                            crate::releases::compare_release_versions(item, best)
                        });
                    if compared == Some(Ordering::Greater)
                        || (local_update_version(best).is_none()
                            && local_update_version(item).is_some())
                    {
                        item
                    } else {
                        best
                    }
                });
            let local = local_mod.and_then(local_update_version).map(str::to_owned);
            if repo.download_mode == ModDownloadMode::RepositoryFile {
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
            let remote = remote_update_version(repo, local_mod);
            let state = local_mod
                .map(|installed| mod_update_state(Some(repo), installed).1)
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

pub(super) fn repository_remote_version(repo: &ModRepositoryConfig) -> Option<String> {
    if repo.download_mode == ModDownloadMode::RepositoryFile || repo.last_error.is_some() {
        return None;
    }
    repo.cached_releases.first().and_then(|release| {
        release
            .version
            .clone()
            .or_else(|| Some(release.tag_name.clone()))
    })
}

pub(super) fn repository_update_state(
    repo: Option<&ModRepositoryConfig>,
    installed_version: Option<&str>,
) -> (bool, &'static str) {
    let Some(repo) = repo else {
        return (false, "unbound");
    };
    if repo.download_mode == ModDownloadMode::RepositoryFile {
        return (false, "not_release_asset");
    }
    if repo.last_error.is_some() {
        return (false, "repository_unavailable");
    }
    let remote = repository_remote_version(repo);
    match remote
        .as_deref()
        .zip(installed_version)
        .and_then(|(remote, local)| crate::releases::compare_release_versions(remote, local))
    {
        Some(Ordering::Greater) => (true, "update_available"),
        Some(_) => (false, "latest"),
        None => (false, "unknown"),
    }
}

pub(super) fn bound_release<'a>(
    repo: &'a ModRepositoryConfig,
    binding: &ModRepositoryBinding,
) -> Option<&'a crate::models::ReleaseInfo> {
    if repo.last_error.is_some() || !binding.matches_repository(repo) {
        return None;
    }
    repo.cached_releases.iter().find(|release| {
        let prerelease = crate::releases::resolve_prerelease(
            release.api_prerelease,
            release.prerelease,
            Some(&release.tag_name),
        );
        !release.draft
            && match binding.channel {
                Channel::All => true,
                Channel::Stable => !prerelease,
                Channel::Beta => prerelease,
            }
    })
}

/// Preserve a Release's version for known unchanged DLL metadata. For legacy
/// manifests, a comparable disk version takes precedence in either direction;
/// there is no safe way to reconstruct what those packages originally contained.
pub(super) fn effective_local_version<'a>(
    binding_tag: &'a str,
    dll_version: Option<&'a str>,
    installed_dll_version: Option<&str>,
) -> Option<&'a str> {
    let same = |a: &str, b: &str| {
        a == b || crate::releases::compare_release_versions(a, b) == Some(Ordering::Equal)
    };
    let keep_tag = if let Some(baseline) = installed_dll_version.map(str::trim) {
        dll_version.map_or(baseline.is_empty(), |actual| same(baseline, actual))
    } else {
        dll_version.is_none_or(|dll| {
            crate::releases::compare_release_versions(dll, binding_tag)
                .is_none_or(|order| order == Ordering::Equal)
        })
    };
    if keep_tag {
        Some(binding_tag)
    } else {
        dll_version
    }
}

pub(super) fn binding_update_state(
    repo: Option<&ModRepositoryConfig>,
    binding: &ModRepositoryBinding,
    local: Option<&str>,
) -> (bool, &'static str) {
    let Some(repo) = repo else {
        return (false, "bound_repository_missing");
    };
    if repo.last_error.is_some() {
        return (false, "repository_unavailable");
    }
    let Some(release) = bound_release(repo, binding) else {
        return (false, "unknown");
    };
    let Some(local) = local else {
        return (false, "unknown");
    };
    if release.tag_name == local {
        return (false, "latest");
    }
    // Exact tags also work for non-SemVer releases; comparable versions must
    // never turn an installed newer tag into a downgrade notification.
    // 沿用 Release 比较器：四段数字按 beta 序号处理，不套本地 DLL 的数字段排序。
    let update = match crate::releases::compare_release_versions(&release.tag_name, local) {
        Some(ordering) => ordering == Ordering::Greater,
        None if local == binding.tag => true,
        None => return (false, "unknown"),
    };
    if update {
        (true, "update_available")
    } else {
        (false, "latest")
    }
}

pub(crate) fn local_update_version(installed: &InstalledMod) -> Option<&str> {
    installed
        .update_version
        .as_deref()
        .or(installed.version.as_deref())
}

fn remote_update_version(
    repo: &ModRepositoryConfig,
    installed: Option<&InstalledMod>,
) -> Option<String> {
    match installed.and_then(|installed| installed.repository_binding.as_ref()) {
        Some(binding) => bound_release(repo, binding).map(|release| release.tag_name.clone()),
        None => repository_remote_version(repo),
    }
}

pub(super) fn mod_update_state(
    repo: Option<&ModRepositoryConfig>,
    installed: &InstalledMod,
) -> (bool, &'static str) {
    if repo.is_some_and(|repo| repo.download_mode == ModDownloadMode::RepositoryFile) {
        return (false, "not_release_asset");
    }
    if installed.release_companion {
        return (false, "installed");
    }
    let local = local_update_version(installed);
    match installed
        .repository_binding
        .as_ref()
        .filter(|binding| binding.is_valid())
    {
        Some(binding) => binding_update_state(repo, binding, local),
        None => repository_update_state(repo, local),
    }
}

pub(super) fn apply_mod_update_state(repo: Option<&ModRepositoryConfig>, item: &mut InstalledMod) {
    // Manifest coordinates cover every file in the package. When the source
    // identifies a primary plugin, a different GUID cannot stand in for it.
    item.release_companion = item.repository_binding.is_some()
        && repo.is_some_and(|repo| {
            if !repo.plugin_guid.trim().is_empty() && item.guid.is_some() {
                item.repository_identity_match != Some(RepositoryIdentityMatch::Guid)
            } else {
                (!repo.local_product_name.trim().is_empty()
                    || !repo.local_original_filename.trim().is_empty())
                    && item.repository_identity_match.is_none()
            }
        });
    let (update, status) = mod_update_state(repo, item);
    item.update_available = update;
    item.status = status.into();
}

pub(super) fn refresh_status_labels(mods: &mut [InstalledMod]) {
    for item in mods {
        let mut labels = Vec::new();
        /*
         * 纯依赖 DLL（读不出 BepInPlugin）只标「依赖」，不再套用
         * 最新版/可更新/未绑定这些 MOD 语义 —— 它没有版本可比，
         * 把它当 MOD 展示会让包内文件数对不上。
         */
        if item.is_dependency {
            labels.push(InstalledModStatusLabel::Dependency);
            if !item.enabled {
                labels.push(InstalledModStatusLabel::Disabled);
            }
            item.update_available = false;
            item.status = "dependency".into();
            item.labels = labels;
            continue;
        }
        if item.update_available {
            labels.push(InstalledModStatusLabel::UpdateAvailable);
        } else {
            match item.status.as_str() {
                "latest" => labels.push(InstalledModStatusLabel::Latest),
                "not_release_asset" => labels.push(InstalledModStatusLabel::NotReleaseAsset),
                "unbound" => labels.push(InstalledModStatusLabel::Unbound),
                "bound_repository_missing" => {
                    labels.push(InstalledModStatusLabel::BoundRepositoryMissing)
                }
                "repository_unavailable" => {
                    labels.push(InstalledModStatusLabel::RepositoryUnavailable)
                }
                "unknown" => labels.push(InstalledModStatusLabel::Unknown),
                _ => {}
            }
        }
        // 同 GUID 的多个副本一律标「重复」（含被高版本取代的那一份），
        // 被取代的那份再额外带「低版本」，两条记录各自都能看出重复关系。
        if item.duplicate_guid {
            labels.push(InstalledModStatusLabel::Duplicate);
        }
        if item.duplicate_cleanup_available || item.status == "duplicate_lower_version" {
            labels.push(InstalledModStatusLabel::DuplicateLowerVersion);
        }
        if item.load_warning.as_deref().is_some_and(|warning| {
            warning != "mods.higherVersionPresent" || !item.duplicate_cleanup_available
        }) {
            labels.push(InstalledModStatusLabel::LoadWarning);
        }
        if !item.enabled {
            labels.push(InstalledModStatusLabel::Disabled);
        }
        item.labels = labels;
    }
}

#[cfg(test)]
mod tests;
