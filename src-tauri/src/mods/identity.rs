use super::binding::{ModRepositoryBindingRef, unique_release_upstream};
use super::file_state::canonical_dll_name;
use crate::models::{AppConfig, Channel, InstalledMod, ModRepositoryBinding, ModRepositoryConfig};
use std::cmp::Ordering;

/// Borrowed plugin identity shared by local scanning and install previews. A
/// repository identifies a package, so changed GUIDs additionally require the
/// same DLL and agreeing plugin metadata; sibling plugins are not substitutes.
pub(crate) struct PluginIdentity<'a> {
    pub guid: Option<&'a str>,
    pub name: Option<&'a str>,
    pub product_name: Option<&'a str>,
    pub original_filename: Option<&'a str>,
    pub dll_file: &'a str,
    pub is_dependency: bool,
    repository: Option<PluginRepository<'a>>,
}

struct PluginRepository<'a> {
    remote: ModRepositoryBindingRef<'a>,
    channel: Channel,
    declared_core: bool,
    declared_bundled: bool,
}

impl<'a> PluginIdentity<'a> {
    pub(crate) fn from_installed(item: &'a InstalledMod, config: &'a AppConfig) -> Self {
        Self::from_metadata(
            item.guid.as_deref(),
            Some(&item.name),
            item.product_name.as_deref(),
            item.original_filename.as_deref(),
            &item.dll_file,
            item.is_dependency,
        )
        .with_repository(
            config,
            item.repository_binding.as_ref(),
            item.repository_id.as_deref(),
            item.version.as_deref(),
        )
    }

    pub(crate) fn from_metadata(
        guid: Option<&'a str>,
        name: Option<&'a str>,
        product_name: Option<&'a str>,
        original_filename: Option<&'a str>,
        dll_file: &'a str,
        is_dependency: bool,
    ) -> Self {
        Self {
            guid,
            name,
            product_name,
            original_filename,
            dll_file,
            is_dependency,
            repository: None,
        }
    }

    /// Preserve real install bindings. Manual DLL/ZIP previews reuse the scan
    /// binding matcher without inventing or persisting an installed Release tag.
    pub(crate) fn with_repository(
        mut self,
        config: &'a AppConfig,
        binding: Option<&'a ModRepositoryBinding>,
        source_id: Option<&str>,
        version: Option<&str>,
    ) -> Self {
        let Some(version) = nonempty(binding.map(|binding| binding.tag.as_str()).or(version))
        else {
            return self;
        };
        if crate::releases::compare_release_versions(version, version).is_none() {
            return self;
        }
        let channel = if super::binding::installed_prerelease(config, binding, false, Some(version))
        {
            Channel::Beta
        } else {
            Channel::Stable
        };
        let remote = if let Some(binding) = binding {
            if binding.channel != Channel::All && binding.channel != channel {
                return self;
            }
            let Some(remote) = ModRepositoryBindingRef::from_binding(binding) else {
                return self;
            };
            remote
        } else {
            let Some(upstream) = unique_release_upstream(
                config,
                self.guid,
                self.original_filename,
                version,
                source_id,
                channel,
            ) else {
                return self;
            };
            upstream
        };
        let mut declared_core = false;
        let mut declared_bundled = false;
        for repo in config.repositories.iter().filter(|repo| {
            crate::config::mod_repository_is_available(config, repo, &config.region)
                && remote.matches(repo)
                && (repo.channel == channel || repo.channel == Channel::All)
        }) {
            declared_core |= self.matches_release_core(repo);
            declared_bundled |= repo
                .bundled_plugin_guids
                .iter()
                .any(|guid| equal_nonempty(self.guid, Some(guid)));
        }
        self.repository = Some(PluginRepository {
            remote,
            channel,
            declared_core,
            declared_bundled,
        });
        self
    }

    /// A historical core must match both PE fields, not merely the display
    /// binding's product name or manifest repository. This never aliases GUIDs.
    pub(crate) fn matches_release_core(&self, repo: &ModRepositoryConfig) -> bool {
        equal_nonempty(self.guid, Some(&repo.plugin_guid))
            || (nonempty(self.guid).is_some()
                && !repo
                    .bundled_plugin_guids
                    .iter()
                    .any(|guid| equal_nonempty(self.guid, Some(guid)))
                && equal_nonempty(self.product_name, Some(&repo.local_product_name))
                && equal_nonempty(self.original_filename, Some(&repo.local_original_filename)))
    }
}

/// The sole plugin equivalence rule for duplicate labels, lower local versions
/// and installation conflicts. Different nonempty GUIDs require proven matching
/// repository coordinates/channel and compatible non-GUID DLL identity.
pub(crate) fn same_plugin_identity(left: &PluginIdentity<'_>, right: &PluginIdentity<'_>) -> bool {
    if let (Some(left_guid), Some(right_guid)) = (nonempty(left.guid), nonempty(right.guid)) {
        if left_guid.eq_ignore_ascii_case(right_guid) {
            return true;
        }
    } else {
        return equal_nonempty(left.name, right.name);
    }
    same_release_plugin_identity(left, right)
}

/// Match physical plugin evidence on the same upstream Release line even
/// when the GUID changed. A batch upgrade uses this in addition to its own
/// version, file ownership and candidate checks, including for same GUIDs.
pub(crate) fn same_release_plugin_identity(
    left: &PluginIdentity<'_>,
    right: &PluginIdentity<'_>,
) -> bool {
    // A package ID is a source entry ID, not a unique physical installation:
    // old/new copies in separate directories can legitimately have the same ID.
    // Distinct bundled plugins are separated by their DLL/metadata below.
    if left.is_dependency || right.is_dependency {
        return false;
    }
    let Some((left_repo, right_repo)) = left.repository.as_ref().zip(right.repository.as_ref())
    else {
        return false;
    };
    if left_repo.channel != right_repo.channel || !left_repo.remote.same_remote(&right_repo.remote)
    {
        return false;
    }
    for (left, right) in [
        (left.product_name, right.product_name),
        (left.original_filename, right.original_filename),
    ] {
        if matches!((nonempty(left), nonempty(right)), (Some(l), Some(r)) if !l.eq_ignore_ascii_case(r))
        {
            return false;
        }
    }
    let left_file = normalized_dll_filename(left.dll_file);
    let right_file = normalized_dll_filename(right.dll_file);
    let same_disk_name = !left_file.is_empty() && left_file == right_file;
    let same_original = nonempty(left.original_filename).is_some_and(|original| {
        normalized_dll_filename(original) == original.to_ascii_lowercase()
            && original.to_ascii_lowercase().ends_with(".dll")
            && equal_nonempty(Some(original), right.original_filename)
    });
    // A shared GUID on the same release line may keep its original filename
    // in PE metadata even after a user renames the physical file.
    if (same_disk_name || same_original) && equal_nonempty(left.guid, right.guid) {
        return true;
    }
    if left_repo.declared_bundled || right_repo.declared_bundled {
        return false;
    }
    let same_product = equal_nonempty(left.product_name, right.product_name);
    if left_repo.declared_core || right_repo.declared_core {
        // The defined core GUID and strong PE evidence distinguish a moved or
        // renamed legacy DLL from another plugin carrying the same package ID.
        if (same_original && (same_disk_name || same_product)) || (same_disk_name && same_product) {
            return true;
        }
    }
    // Keep the strict legacy path for old manifests without a configured core.
    same_disk_name && (same_original || same_product) && equal_nonempty(left.name, right.name)
}

fn normalized_dll_filename(path: &str) -> String {
    canonical_dll_name(
        path.replace('\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or_default(),
    )
    .trim()
    .to_ascii_lowercase()
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn equal_nonempty(left: Option<&str>, right: Option<&str>) -> bool {
    nonempty(left)
        .zip(nonempty(right))
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
}

/// Cleanup requires a distinct, enabled physical DLL with a real GUID and a
/// strictly newer equivalent version. Name-only matches are for labels only.
pub(crate) fn is_newer_duplicate(
    target: &InstalledMod,
    peer: &InstalledMod,
    config: &AppConfig,
) -> bool {
    is_higher_physical_copy(target, peer)
        && same_plugin_identity(
            &PluginIdentity::from_installed(target, config),
            &PluginIdentity::from_installed(peer, config),
        )
}

fn is_higher_physical_copy(target: &InstalledMod, peer: &InstalledMod) -> bool {
    peer.enabled
        && !canonical_dll_name(&peer.dll_file)
            .eq_ignore_ascii_case(&canonical_dll_name(&target.dll_file))
        && nonempty(target.guid.as_deref()).is_some()
        && nonempty(peer.guid.as_deref()).is_some()
        && peer
            .version
            .as_deref()
            .zip(target.version.as_deref())
            .is_some_and(|(newer, old)| compare_mod_versions(newer, old) == Some(Ordering::Greater))
}

pub(super) fn note_key_for_values(
    guid: Option<&str>,
    product_name: Option<&str>,
    dll_file: &str,
) -> String {
    if let Some(guid) = guid.filter(|value| !value.is_empty()) {
        return format!("guid:{}", guid.to_ascii_lowercase());
    }
    if let Some(name) = product_name.filter(|value| !value.is_empty()) {
        return format!("name:{}", name.to_ascii_lowercase());
    }
    format!("file:{}", dll_file.to_ascii_lowercase())
}

pub fn note_key(mod_info: &InstalledMod) -> String {
    note_key_for_values(
        mod_info.guid.as_deref(),
        mod_info.product_name.as_deref(),
        &mod_info.dll_file,
    )
}

pub(super) fn mark_duplicate_guids(mods: &mut [InstalledMod], config: &AppConfig) {
    // Rebinding reuses existing rows, so every pass must revoke its old verdicts
    // before checking enabled peers again (including after disable/removal).
    for item in mods.iter_mut() {
        item.duplicate_guid = false;
        item.duplicate_version = false;
        item.duplicate_cleanup_available = false;
        if item.load_warning.as_deref() == Some("mods.higherVersionPresent") {
            item.load_warning = None;
        }
        if item.status == "duplicate_lower_version" {
            // Runtime callers recompute source/update status before this pass.
            // A standalone repeat cannot recover that old source conclusion.
            item.status = "unknown".into();
        }
    }
    let identities: Vec<_> = mods
        .iter()
        .map(|item| PluginIdentity::from_installed(item, config))
        .collect();
    let mut matches = vec![(false, false, Vec::new()); mods.len()];
    for (index, item) in mods.iter().enumerate() {
        if !item.enabled {
            continue;
        }
        let peers = mods
            .iter()
            .enumerate()
            .filter(|(peer_index, item)| {
                *peer_index != index
                    && item.enabled
                    && same_plugin_identity(&identities[index], &identities[*peer_index])
            })
            .map(|(peer_index, _)| peer_index)
            .collect::<Vec<_>>();
        if peers.is_empty() {
            continue;
        }
        log::warn!(
            "发现重复 MOD 身份：名称={}，GUID={}，重复数量={}",
            mods[index].name,
            mods[index].guid.as_deref().unwrap_or("未知"),
            peers.len()
        );
        let duplicate_version = peers
            .iter()
            .any(|peer| mods_share_guid_and_version(item, &mods[*peer]));
        let higher: Vec<usize> = peers
            .into_iter()
            .filter(|peer| {
                mods[*peer].load_warning.is_none() && is_higher_physical_copy(item, &mods[*peer])
            })
            .collect();
        matches[index] = (true, duplicate_version, higher);
    }
    for (item, (duplicate, duplicate_version, higher)) in mods.iter_mut().zip(&matches) {
        item.duplicate_guid = *duplicate;
        item.duplicate_version = *duplicate_version;
        if let Some(current) = item.version.as_deref()
            && higher.iter().any(|&peer| matches[peer].2.is_empty())
        {
            log::warn!(
                "重复 MOD 中存在更高版本：名称={}，当前版本={}",
                item.name,
                current
            );
            let had_other_warning = item.load_warning.is_some();
            item.load_warning
                .get_or_insert_with(|| "mods.higherVersionPresent".into());
            item.duplicate_cleanup_available = true;
            // The older equivalent copy is a cleanup candidate, including a
            // predecessor GUID that BepInEx would otherwise load separately.
            item.update_available = false;
            if !had_other_warning
                && !matches!(
                    item.status.as_str(),
                    "not_release_asset" | "bound_repository_missing" | "repository_unavailable"
                )
            {
                item.status = "duplicate_lower_version".into();
            }
        }
    }
}

fn mods_share_guid_and_version(left: &InstalledMod, right: &InstalledMod) -> bool {
    let same_guid = left
        .guid
        .as_deref()
        .zip(right.guid.as_deref())
        .is_some_and(|(left, right)| {
            !left.trim().is_empty()
                && !right.trim().is_empty()
                && left.trim().eq_ignore_ascii_case(right.trim())
        });
    let same_version = left
        .version
        .as_deref()
        .zip(right.version.as_deref())
        .and_then(|(left, right)| compare_mod_versions(left, right))
        == Some(Ordering::Equal);
    same_guid && same_version
}

pub(crate) fn is_safe_replacement_mod(target: &InstalledMod, peer: &InstalledMod) -> bool {
    let target_path = canonical_dll_name(&target.dll_file.replace('\\', "/"));
    let peer_path = canonical_dll_name(&peer.dll_file.replace('\\', "/"));
    !target_path.eq_ignore_ascii_case(&peer_path)
        && peer.enabled
        && peer.load_warning.is_none()
        && target
            .guid
            .as_deref()
            .zip(peer.guid.as_deref())
            .is_some_and(|(target, peer)| {
                !target.trim().is_empty() && target.trim().eq_ignore_ascii_case(peer.trim())
            })
        && target
            .version
            .as_deref()
            .zip(peer.version.as_deref())
            .is_some_and(|(target, peer)| {
                compare_mod_versions(peer, target).is_some_and(|order| order != Ordering::Less)
            })
}

/// 比较两个**本地** DLL 的版本号，用于同 GUID 副本去重。
///
/// 这里刻意用「纯数字段逐段比较」，**不是** semver 语义 —— 与
/// `releases::compare_release_versions`（线上更新判定）是两套规则，
/// 不要试图统一。
///
/// 为什么必须不同：BepInEx 加载同 GUID 的多份 DLL 时，比的是程序集版本号
/// 的**数字大小**，`1.9.8.1`（四段）严格大于 `1.9.8`。如果这里改用 semver
/// （beta 低于同号正式版），管理器会把 `1.9.8` 判为更高并建议卸载
/// `1.9.8.1` —— 而实际被游戏加载的恰恰是 `1.9.8.1`，建议卸载正在生效的
/// 文件是最坏的结果。
///
/// 两者各司其职：
///   · 本地副本谁在生效 → 纯数字段比较（跟随 BepInEx）
///   · 线上是否有新版   → semver（`1.9.8` 确实是 `1.9.8-beta.1` 的升级）
///
/// `mod_preview::find_install_conflicts` 判断「本次要装的版本与本地已装是否
/// 相同」也用它：同一份 DLL 换不换版本，该跟 BepInEx 的判定一致。
pub(crate) fn compare_mod_versions(left: &str, right: &str) -> Option<Ordering> {
    Some(parse_mod_version_components(left)?.cmp(&parse_mod_version_components(right)?))
}

fn parse_mod_version_components(value: &str) -> Option<[u64; 4]> {
    let numeric = value
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()?;
    let parts = numeric.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut components = [0_u64; 4];
    for (index, part) in parts.into_iter().enumerate() {
        components[index] = part.parse().ok()?;
    }
    Some(components)
}
