use crate::models::Channel;
use crate::models::InstalledMod;
use crate::models::ModDownloadMode;
use crate::models::ModRepositoryConfig;
use std::collections::HashMap;

pub(super) fn select_repository<'a>(
    candidates: &[&'a ModRepositoryConfig],
    installed_version: Option<&str>,
    source: Option<&ModRepositoryConfig>,
) -> Option<&'a ModRepositoryConfig> {
    // A real Raw install cannot silently become a Release install because the
    // same plugin GUID also occurs in a repository with a newer cached tag.
    if let Some(source) =
        source.filter(|repo| repo.download_mode == ModDownloadMode::RepositoryFile)
    {
        return candidates.iter().copied().find(|repo| repo.id == source.id);
    }
    /*
     * 本地装的是哪一个渠道，按「版本号是否预发布」判断，不按段数。
     *
     * 判定统一交给 `releases::is_prerelease_version`：四段版本号是 beta
     * 序号（`1.8.7.23` ≡ `1.8.7-beta.23`），各种 beta 写法
     * （`-beta.4`、`-beta1`、`.beta4`、`-pre.01`）也都算预发布。
     * 在这里另写一套数段逻辑会和它分叉 —— 比如 `1.8.7.beta4` 按 `.`
     * 数会得到 4 段，看起来像四段 beta 序号，其实是三段 + 标识。
     */
    let preferred_channel = installed_version.map(|version| {
        if crate::releases::is_prerelease_version(version) {
            Channel::Beta
        } else {
            Channel::Stable
        }
    });
    let compatible_release = |repo: &&ModRepositoryConfig| {
        repo.download_mode == ModDownloadMode::Release
            && preferred_channel
                .as_ref()
                .is_none_or(|channel| repo.channel == *channel || repo.channel == Channel::All)
    };
    if let Some(source) = source {
        if let Some(repo) = candidates
            .iter()
            .copied()
            .filter(compatible_release)
            .find(|repo| repo.id == source.id)
        {
            return Some(repo);
        }
    }
    let release = preferred_channel
        .as_ref()
        .and_then(|channel| {
            candidates.iter().copied().find(|repo| {
                repo.download_mode == ModDownloadMode::Release && repo.channel == *channel
            })
        })
        .or_else(|| candidates.iter().copied().find(compatible_release));
    // Raw has no Release channel, regardless of the number of version parts.
    release.or_else(|| {
        candidates
            .iter()
            .copied()
            .find(|repo| repo.download_mode == ModDownloadMode::RepositoryFile)
    })
}

/// 同目录下读不出 GUID 的 DLL，继承同目录里第一个已绑定仓库的插件的归属。
///
/// 场景：用户自己把 ZIP 解压到 `BepInEx/plugins/SomeMod/`，里面既有能读出
/// BepInPlugin 的主插件，也有读不出元数据的依赖 DLL。主插件能按 GUID 绑到
/// 在线仓库，依赖绑不上，于是：
///   - 前端分组会把它当成「本地安装」/「未绑定」，与主插件分成两组，
///     永远折不成同一个 MOD 包；
///   - 仓库更新时按仓库身份回收，也认不出这个依赖该一起删。
///
/// 这里给它们补上同一个仓库身份。**只在同一目录内传播**：跨目录没有可靠
/// 依据（plugins 根目录下各插件互不相干），不能仅凭「都在 plugins 里」就
/// 认为属于同一个包。
pub(super) fn inherit_repository_from_sibling(mods: &mut [InstalledMod]) {
    /// DLL 所在目录（游戏相对路径去掉文件名，小写）。
    fn parent_dir(dll_file: &str) -> Option<String> {
        let normalized = dll_file.replace('\\', "/");
        let (dir, _) = normalized.rsplit_once('/')?;
        let dir = dir.trim_end_matches('/');
        if dir.is_empty() {
            return None;
        }
        /*
         * BepInEx 的系统容器不算「包目录」：plugins / patchers / config 下
         * 平铺的 DLL 只是碰巧放在一起，彼此互不相干，不能互相继承仓库身份。
         */
        const SHARED_CONTAINERS: [&str; 3] = ["plugins", "patchers", "config"];
        let last = dir.rsplit_once('/').map_or(dir, |(_, tail)| tail);
        if SHARED_CONTAINERS.contains(&last.to_ascii_lowercase().as_str()) {
            return None;
        }
        Some(dir.to_lowercase())
    }

    // 先收集每个目录里第一个能绑定仓库的成员，作为该目录的代表。
    let mut by_dir: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for item in mods.iter() {
        if item.is_dependency || item.repository_ids.is_empty() {
            continue;
        }
        let Some(dir) = parent_dir(&item.dll_file) else {
            continue;
        };
        by_dir.entry(dir).or_insert_with(|| {
            (
                item.repository_id.clone().unwrap_or_default(),
                item.repository_ids.clone(),
            )
        });
    }
    if by_dir.is_empty() {
        return;
    }

    for item in mods.iter_mut() {
        // 只补「自己完全没有仓库身份」的项，不覆盖已知归属
        if !item.repository_ids.is_empty() || item.repository_binding.is_some() {
            continue;
        }
        let Some(dir) = parent_dir(&item.dll_file) else {
            continue;
        };
        let Some((repository_id, repository_ids)) = by_dir.get(&dir) else {
            continue;
        };
        item.repository_ids = repository_ids.clone();
        if item.repository_id.is_none() && !repository_id.is_empty() {
            item.repository_id = Some(repository_id.clone());
        }
        log::debug!(
            "同目录插件继承仓库归属：文件={}，目录={}，仓库={:?}",
            item.dll_file,
            dir,
            repository_ids.first()
        );
    }
}

pub(super) fn repository_matches_mod(
    repo: &crate::models::ModRepositoryConfig,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
    installed_version: Option<&str>,
) -> bool {
    let identity_matches = [
        (guid, repo.plugin_guid.as_str()),
        (product_name, repo.local_product_name.as_str()),
        (original_filename, repo.local_original_filename.as_str()),
    ]
    .into_iter()
    .any(|(actual, configured)| {
        let configured = configured.trim();
        !configured.is_empty()
            && actual.is_some_and(|value| value.trim().eq_ignore_ascii_case(configured))
    });
    if !identity_matches {
        return false;
    }
    let pattern = repo.version_regex.trim();
    if pattern.is_empty() {
        return true;
    }
    installed_version
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| {
            regex::Regex::new(pattern)
                .ok()
                .map(|regex| regex.is_match(value))
        })
        .unwrap_or(false)
}

/// Match either a scan result or an existing local snapshot against the current
/// source configuration. Manifest identity wins over GUID/PE fallback.
pub(super) fn match_repository<'a>(
    config: &'a crate::models::AppConfig,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
    installed_version: Option<&str>,
    binding: Option<&crate::models::ModRepositoryBinding>,
    source_id: Option<&str>,
) -> (Option<&'a ModRepositoryConfig>, Vec<String>) {
    let source_repository =
        source_id.and_then(|id| config.repositories.iter().find(|repo| repo.id == id));
    let candidates = config
        .repositories
        .iter()
        .filter(|repo| {
            crate::config::mod_repository_is_available(
                config,
                repo,
                crate::config::current_user_region(),
            ) && if let Some(binding) = binding {
                binding.matches_repository(repo)
            } else {
                repository_matches_mod(
                    repo,
                    guid,
                    product_name,
                    original_filename,
                    installed_version,
                )
            }
        })
        .collect::<Vec<_>>();
    let repository = if let Some(binding) = binding {
        candidates
            .iter()
            .copied()
            .find(|repo| source_repository.is_some_and(|source| source.id == repo.id))
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|repo| repo.channel == binding.channel)
            })
            .or_else(|| candidates.first().copied())
    } else {
        select_repository(&candidates, installed_version, source_repository)
    };
    let ids = repository
        .map(|bound| {
            candidates
                .iter()
                .filter(|repo| repo.download_mode == bound.download_mode)
                .map(|repo| repo.id.clone())
                .collect()
        })
        .unwrap_or_default();
    (repository, ids)
}

/// Rebind already parsed DLL facts; no filesystem scan, metadata read or network.
/// Source arrival and local scan completion both call this same implementation.
pub(crate) fn rebind_installed_mods(config: &crate::models::AppConfig, mods: &mut [InstalledMod]) {
    for item in mods.iter_mut() {
        let binding = item
            .repository_binding
            .as_ref()
            .filter(|binding| binding.is_valid());
        let (repository, ids) = match_repository(
            config,
            item.guid.as_deref(),
            item.product_name.as_deref(),
            item.original_filename.as_deref(),
            item.version.as_deref(),
            binding,
            item.package_id.as_deref().or(item.repository_id.as_deref()),
        );
        let (update, status) =
            super::status::installed_update_state(repository, binding, item.version.as_deref());
        item.repository_id = repository.map(|repo| repo.id.clone());
        item.repository_ids = ids;
        item.update_available = update;
        item.status = status.into();
        if let Some(repo) = repository {
            if item.group.is_empty() {
                item.group = repo.group.clone();
            }
            if item.group_i18n.is_empty() {
                item.group_i18n = repo.group_i18n.clone();
            }
        }
    }
    inherit_repository_from_sibling(mods);
    super::identity::mark_duplicate_guids(mods);
    super::status::refresh_status_labels(mods);
}
