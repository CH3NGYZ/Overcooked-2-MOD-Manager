use crate::models::Channel;
use crate::models::InstalledMod;
use crate::models::ModDownloadMode;
use crate::models::ModRepositoryBinding;
use crate::models::ModRepositoryConfig;
use crate::models::RepositoryIdentityMatch;
use std::collections::{HashMap, HashSet};

/// An exact installed Release identity supplies its tag and platform verdict
/// before DLL metadata. Never infer this from the latest tag, GUID or another source.
pub(crate) fn installed_prerelease(
    config: &crate::models::AppConfig,
    binding: Option<&crate::models::ModRepositoryBinding>,
    legacy: bool,
    version: Option<&str>,
) -> bool {
    let binding = binding.filter(|binding| binding.is_valid());
    let api = binding.and_then(|binding| {
        let key = crate::releases::repository_cache_key(
            binding.provider.clone(),
            &binding.owner,
            &binding.repository,
        );
        let current = config
            .repositories
            .iter()
            .filter(|repo| {
                repo.download_mode == ModDownloadMode::Release && binding.matches_identity(repo)
            })
            .filter_map(|repo| {
                repo.release_prerelease_metadata
                    .get(&binding.tag)
                    .copied()
                    .or_else(|| {
                        repo.cached_releases
                            .iter()
                            .find(|release| release.tag_name == binding.tag)
                            .and_then(|release| release.api_prerelease)
                    })
                    .map(|value| (repo.releases_cached_at, value))
            });
        let staged = config
            .staged_release_cache
            .values()
            .filter(|snapshot| snapshot.repository_key.as_deref() == Some(key.as_str()))
            .filter_map(|snapshot| {
                snapshot
                    .release_prerelease_metadata
                    .get(&binding.tag)
                    .copied()
                    .map(|value| (snapshot.cached_at, value))
            });
        current
            .chain(staged)
            .max_by_key(|(time, _)| *time)
            .map(|(_, value)| value)
    });
    crate::releases::resolve_prerelease(
        api,
        legacy,
        binding.map(|binding| binding.tag.as_str()).or(version),
    )
}

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
    let diylevel_dirs = mods
        .iter()
        .filter(|item| is_diylevel_core(item))
        .filter(|item| legacy_diylevel_core_directory(&item.dll_file).is_some())
        .filter_map(|item| parent_dir(&item.dll_file))
        .collect::<HashSet<_>>();
    for item in mods.iter() {
        if item.is_dependency || item.repository_ids.is_empty() {
            continue;
        }
        let Some(dir) = parent_dir(&item.dll_file) else {
            continue;
        };
        if diylevel_dirs.contains(&dir) {
            // DIYLevel has one known GUID-less sibling. Do not convert all
            // other libraries in the directory into duplicate repository copies.
            continue;
        }
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
        // A DLL with its own plugin GUID is not a GUID-less library. Same-directory
        // placement cannot substitute for an explicit package declaration.
        if item
            .guid
            .as_deref()
            .is_some_and(|guid| !guid.trim().is_empty())
            || !item.repository_ids.is_empty()
            || item.repository_binding.is_some()
        {
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

fn is_diylevel_core(item: &InstalledMod) -> bool {
    item.guid.as_deref().is_some_and(|guid| {
        guid.trim()
            .eq_ignore_ascii_case(super::plugin_target::DIYLEVEL_GUID)
    })
}

/// Legacy DIYLevel ships a GUID-less LevelEditorStub directly beside its core.
/// This structural evidence is for grouping/switches; replacement must also
/// prove the selected asset contains identical bytes at the exact member path.
pub(crate) fn legacy_diylevel_stub(core: &str, member: &str) -> bool {
    let member = member.replace('\\', "/").to_ascii_lowercase();
    legacy_diylevel_core_directory(core)
        .is_some_and(|dir| member.rsplit_once('/') == Some((dir.as_str(), "leveleditorstub.dll")))
}

fn legacy_diylevel_core_directory(core: &str) -> Option<String> {
    let core = core.replace('\\', "/").to_ascii_lowercase();
    let Some((dir, "oc2diylevel.dll")) = core.rsplit_once('/') else {
        return None;
    };
    dir.starts_with("bepinex/plugins/").then(|| dir.to_string())
}

pub(super) fn repository_matches_mod(
    repo: &crate::models::ModRepositoryConfig,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
    installed_version: Option<&str>,
) -> bool {
    if !repository_matches_identity(repo, guid, product_name, original_filename) {
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

/// Identity evidence can exist even when a repository is filtered out, or
/// when there is no version/channel match to select it as the current source.
pub(crate) fn repository_matches_identity(
    repo: &crate::models::ModRepositoryConfig,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
) -> bool {
    repository_identity_match(Some(repo), guid, product_name, original_filename).is_some()
}

/// The matching fields are the same as repository binding, but a GUID names a
/// single plugin more precisely than PE metadata common to package members.
pub(super) fn repository_identity_match(
    repo: Option<&ModRepositoryConfig>,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
) -> Option<RepositoryIdentityMatch> {
    let repo = repo?;
    let matches = |actual: Option<&str>, configured: &str| {
        let configured = configured.trim();
        !configured.is_empty()
            && actual.is_some_and(|value| value.trim().eq_ignore_ascii_case(configured))
    };
    if matches(guid, &repo.plugin_guid) {
        return Some(RepositoryIdentityMatch::Guid);
    }
    // A declared helper may share PE metadata with the core. That evidence
    // must not bind the helper as the repository's primary installed plugin.
    if repo
        .bundled_plugin_guids
        .iter()
        .any(|member| matches(guid, member))
    {
        return None;
    }
    [
        (product_name, repo.local_product_name.as_str()),
        (original_filename, repo.local_original_filename.as_str()),
    ]
    .into_iter()
    .any(|(actual, configured)| matches(actual, configured))
    .then_some(RepositoryIdentityMatch::Metadata)
}

/// Resolve strong DLL evidence to one upstream before comparing changed GUIDs.
/// Unlike display binding, product-name matches and preferred source IDs cannot
/// choose between upstreams; several entries for the same remote are allowed.
pub(super) fn unique_release_upstream<'a>(
    config: &'a crate::models::AppConfig,
    guid: Option<&str>,
    original_filename: Option<&str>,
    version: &str,
    source_id: Option<&str>,
    channel: Channel,
) -> Option<ModRepositoryBindingRef<'a>> {
    let is_candidate = |repo: &&ModRepositoryConfig| {
        crate::config::mod_repository_is_available(config, repo, &config.region)
            && repository_matches_mod(repo, guid, None, original_filename, Some(version))
    };
    let mut candidates = config.repositories.iter().filter(is_candidate);
    let remote = ModRepositoryBindingRef::from_repository(candidates.next()?)?;
    // An additional real match may be a mirror or a Raw source. Neither can
    // prove which publishing upstream actually owns an unbound DLL.
    if !candidates.all(|repo| remote.matches(repo)) {
        return None;
    }
    let on_line =
        |repo: &&ModRepositoryConfig| repo.channel == channel || repo.channel == Channel::All;
    let mut candidates = config
        .repositories
        .iter()
        .filter(is_candidate)
        .filter(on_line);
    match source_id {
        Some(id) => candidates.find(|repo| repo.id == id),
        None => candidates.next(),
    }?;
    Some(remote)
}

/// Coordinates proven from a Release definition or the installed manifest.
pub(super) struct ModRepositoryBindingRef<'a> {
    pub provider: &'a crate::models::Provider,
    pub owner: &'a str,
    pub repository: &'a str,
}

impl<'a> ModRepositoryBindingRef<'a> {
    pub(super) fn from_binding(binding: &'a ModRepositoryBinding) -> Option<Self> {
        binding.is_valid().then(|| Self {
            provider: &binding.provider,
            owner: binding.owner.trim(),
            repository: binding.repository.trim(),
        })
    }

    fn from_repository(repo: &'a ModRepositoryConfig) -> Option<Self> {
        if repo.download_mode != ModDownloadMode::Release
            || crate::releases::repository_coordinates(&repo.username, &repo.repository).is_err()
        {
            return None;
        }
        Some(Self {
            provider: &repo.provider,
            owner: repo.username.trim(),
            repository: repo.repository.trim(),
        })
    }

    pub(super) fn matches(&self, repo: &'a ModRepositoryConfig) -> bool {
        Self::from_repository(repo).is_some_and(|remote| self.same_remote(&remote))
    }

    pub(super) fn same_remote(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.owner.eq_ignore_ascii_case(other.owner)
            && self.repository.eq_ignore_ascii_case(other.repository)
    }
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
            crate::config::mod_repository_is_available(config, repo, &config.region)
                && if let Some(binding) = binding {
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

/// Associate loose DLLs for display and package switches, without changing
/// their repository binding, installed versions, or manifest ownership.
pub(super) fn bind_declared_package_members(
    config: &crate::models::AppConfig,
    mods: &mut [InstalledMod],
) {
    for item in mods.iter_mut() {
        item.bundled_with_dll = None;
    }
    let guid_key = |guid: &str| (!guid.trim().is_empty()).then(|| guid.trim().to_ascii_lowercase());
    let local_guid = |item: &InstalledMod| item.guid.as_deref().and_then(guid_key);
    let mut counts = HashMap::new();
    for guid in mods.iter().filter_map(local_guid) {
        *counts.entry(guid).or_insert(0usize) += 1;
    }
    let repositories = config
        .repositories
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect::<HashMap<_, _>>();
    // A separately published plugin keeps its own identity, even if its source
    // is unavailable for this region/platform or its version cannot bind today.
    let primary_guids = config
        .repositories
        .iter()
        .filter_map(|repo| guid_key(&repo.plugin_guid))
        .collect::<HashSet<_>>();
    let mut claims = HashMap::<String, Option<String>>::new();
    let diylevel = (counts.get(super::plugin_target::DIYLEVEL_GUID) == Some(&1))
        .then(|| {
            let core = mods.iter().find(|item| is_diylevel_core(item))?;
            let repo = repositories.get(core.repository_id.as_deref()?)?;
            (core.repository_identity_match == Some(RepositoryIdentityMatch::Guid)
                && repo
                    .plugin_guid
                    .trim()
                    .eq_ignore_ascii_case(super::plugin_target::DIYLEVEL_GUID)
                && crate::config::mod_repository_is_available(config, repo, &config.region))
            .then(|| core.dll_file.clone())
        })
        .flatten();
    for core in mods
        .iter()
        .filter(|item| item.repository_identity_match == Some(RepositoryIdentityMatch::Guid))
    {
        let Some((guid, repo)) = local_guid(core).zip(
            core.repository_id
                .as_deref()
                .and_then(|id| repositories.get(id)),
        ) else {
            continue;
        };
        if !crate::config::mod_repository_is_available(config, repo, &config.region) {
            continue;
        }
        let anchor = (counts.get(&guid) == Some(&1)).then(|| core.dll_file.clone());
        for guid in repo
            .bundled_plugin_guids
            .iter()
            .filter_map(|guid| guid_key(guid))
        {
            // Distinct installed cores claiming the same GUID are ambiguous.
            claims
                .entry(guid)
                .and_modify(|claim| *claim = None)
                .or_insert_with(|| anchor.clone());
        }
    }
    for item in mods.iter_mut().filter(|item| {
        item.package_id.is_none()
            && item.repository_binding.is_none()
            && item.repository_id.is_none()
            && item.repository_ids.is_empty()
    }) {
        let Some(guid) = local_guid(item) else {
            if let Some(core) = &diylevel
                && legacy_diylevel_stub(core, &item.dll_file)
                && !config.repositories.iter().any(|repo| {
                    repository_matches_identity(
                        repo,
                        item.guid.as_deref(),
                        item.product_name.as_deref(),
                        item.original_filename.as_deref(),
                    )
                })
            {
                item.bundled_with_dll = Some(core.clone());
            }
            continue;
        };
        if counts.get(&guid) == Some(&1) && !primary_guids.contains(&guid) {
            item.bundled_with_dll = claims.get(&guid).cloned().flatten();
        }
    }
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
        let tag_current = binding.filter(|binding| {
            super::status::local_update_version(item) == Some(binding.tag.as_str())
        });
        item.prerelease = installed_prerelease(
            config,
            tag_current,
            (binding.is_none() || tag_current.is_some()) && item.prerelease,
            item.version.as_deref(),
        );
        item.repository_id = repository.map(|repo| repo.id.clone());
        item.repository_ids = ids;
        item.repository_identity_match = repository_identity_match(
            repository,
            item.guid.as_deref(),
            item.product_name.as_deref(),
            item.original_filename.as_deref(),
        );
        super::status::apply_mod_update_state(repository, item);
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
    bind_declared_package_members(config, mods);
    super::identity::mark_duplicate_guids(mods, config);
    super::status::refresh_status_labels(mods);
}
