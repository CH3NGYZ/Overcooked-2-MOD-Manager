//! 本地版本、绑定 Release 与卡片标签；按仓库汇总沿用同一判定。
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

pub(super) fn repository_remote_version(repo: &RepositoryConfig) -> Option<String> {
    if repo.download_mode == RepositoryDownloadMode::RepositoryFile || repo.last_error.is_some() {
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
    repo: Option<&RepositoryConfig>,
    installed_version: Option<&str>,
) -> (bool, &'static str) {
    let Some(repo) = repo else {
        return (false, "unbound");
    };
    if repo.download_mode == RepositoryDownloadMode::RepositoryFile {
        return (false, "not_release_asset");
    }
    if repo.last_error.is_some() {
        return (false, "repository_unavailable");
    }
    let remote = repository_remote_version(repo);
    match (
        installed_version.and_then(crate::releases::parse_version),
        remote.as_deref().and_then(crate::releases::parse_version),
    ) {
        (Some(local), Some(remote)) if remote > local => (true, "update_available"),
        (Some(_), Some(_)) => (false, "latest"),
        _ => (false, "unknown"),
    }
}

pub(super) fn bound_release<'a>(
    repo: &'a RepositoryConfig,
    binding: &RepositoryBinding,
) -> Option<&'a crate::models::ReleaseInfo> {
    if repo.last_error.is_some() || !binding.matches_repository(repo) {
        return None;
    }
    repo.cached_releases.iter().find(|release| {
        !release.draft
            && match binding.channel {
                Channel::All => true,
                Channel::Stable => !release.prerelease,
                Channel::Beta => release.prerelease,
            }
    })
}

/// 版本比较用的本地版本：清单记录的安装 tag 与 DLL 自报版本取较高者。
///
/// 更新检测**以安装清单的 tag 为准**（它记录「这一包装的是哪个 Release」），
/// DLL 自报的 BepInPlugin 版本只用来抑制误报：
///   - 手动降级（DLL 低于清单 tag）：装的时候就是线上最新版，降级是用户自己
///     的选择，不该被当成「待更新」；线上真的出了更高版本仍会照常提示；
///   - 手动升级（DLL 高于清单 tag）：本地其实已经是线上那个版本，只按清单
///     tag 比会一直提示更新。
///
/// 取较高者之后，DLL 版本只能**抑制**更新，绝不会凭空制造一条更新。
/// 前端 `statusTags.ts` 的 `localCompareVersion` 是同一规则的镜像实现。
pub(super) fn effective_local_version(binding_tag: &str, dll_version: Option<&str>) -> String {
    let Some(dll) = dll_version.filter(|value| !value.trim().is_empty()) else {
        return binding_tag.to_string();
    };
    match crate::releases::compare_release_versions(dll, binding_tag) {
        Some(std::cmp::Ordering::Greater) => dll.to_string(),
        _ => binding_tag.to_string(),
    }
}

pub(super) fn binding_update_state(
    repo: Option<&RepositoryConfig>,
    binding: &RepositoryBinding,
    dll_version: Option<&str>,
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
    if release.tag_name == binding.tag {
        return (false, "latest");
    }
    // 本地有效版本：清单 tag 与 DLL 版本取较高者，DLL 只能抑制更新
    let local = effective_local_version(&binding.tag, dll_version);
    if release.tag_name == local {
        return (false, "latest");
    }
    // Exact tags also work for non-SemVer releases; comparable versions must
    // never turn an installed newer tag into a downgrade notification.
    // 沿用 Release 比较器：四段数字按 beta 序号处理，不套本地 DLL 的数字段排序。
    let update = match crate::releases::compare_release_versions(&release.tag_name, &local) {
        Some(ordering) => ordering == std::cmp::Ordering::Greater,
        _ => true,
    };
    if update {
        (true, "update_available")
    } else {
        (false, "latest")
    }
}

pub(crate) fn local_update_version(installed: &InstalledMod) -> Option<String> {
    installed
        .repository_binding
        .as_ref()
        .map(|binding| binding.tag.clone())
        .or_else(|| installed.version.clone())
}

pub(crate) fn remote_update_version(
    repo: &RepositoryConfig,
    installed: Option<&InstalledMod>,
) -> Option<String> {
    match installed.and_then(|installed| installed.repository_binding.as_ref()) {
        Some(binding) => bound_release(repo, binding).map(|release| release.tag_name.clone()),
        None => repository_remote_version(repo),
    }
}

pub(crate) fn mod_update_state(
    repo: &RepositoryConfig,
    installed: &InstalledMod,
) -> (bool, &'static str) {
    match installed.repository_binding.as_ref() {
        Some(binding) => binding_update_state(Some(repo), binding, installed.version.as_deref()),
        None => repository_update_state(Some(repo), installed.version.as_deref()),
    }
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
        if item.status == "duplicate_lower_version" {
            labels.push(InstalledModStatusLabel::DuplicateLowerVersion);
        } else if item.update_available {
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
        if item.load_warning.is_some() && item.status != "duplicate_lower_version" {
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
