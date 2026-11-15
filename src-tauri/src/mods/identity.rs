use super::file_state::canonical_dll_name;
use crate::models::{
    AppConfig, Channel, InstalledMod, ModDownloadMode, ModRepositoryBinding, Provider,
};
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
    provider: &'a Provider,
    owner: &'a str,
    repository: &'a str,
    channel: Channel,
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
        let (provider, owner, repository, channel, version) = if let Some(binding) = binding {
            if !binding.is_valid() {
                return self;
            }
            (
                &binding.provider,
                binding.owner.as_str(),
                binding.repository.as_str(),
                binding.channel,
                Some(binding.tag.as_str()),
            )
        } else {
            // Display grouping may inherit a sibling's repository ID. Treat
            // that ID only as a preference among actual metadata matches.
            let Some(repo) = super::binding::match_repository(
                config,
                self.guid,
                self.product_name,
                self.original_filename,
                version,
                None,
                source_id,
            )
            .0
            else {
                return self;
            };
            if repo.download_mode != ModDownloadMode::Release
                || crate::releases::repository_coordinates(&repo.username, &repo.repository)
                    .is_err()
            {
                return self;
            }
            (
                &repo.provider,
                repo.username.as_str(),
                repo.repository.as_str(),
                repo.channel,
                version,
            )
        };
        // `all` is a source filter, not a concrete installed release line.
        let channel = if channel == Channel::All {
            let Some(version) = nonempty(version) else {
                return self;
            };
            if crate::releases::is_prerelease_version(version) {
                Channel::Beta
            } else {
                Channel::Stable
            }
        } else {
            channel
        };
        self.repository = Some(PluginRepository {
            provider,
            owner: owner.trim(),
            repository: repository.trim(),
            channel,
        });
        self
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
    if left_repo.provider != right_repo.provider
        || left_repo.channel != right_repo.channel
        || !left_repo.owner.eq_ignore_ascii_case(right_repo.owner)
        || !left_repo
            .repository
            .eq_ignore_ascii_case(right_repo.repository)
        || normalized_dll_filename(left.dll_file) != normalized_dll_filename(right.dll_file)
    {
        return false;
    }
    let mut matched = false;
    for (left, right) in [
        (left.name, right.name),
        (left.product_name, right.product_name),
        (left.original_filename, right.original_filename),
    ] {
        if let (Some(left), Some(right)) = (nonempty(left), nonempty(right)) {
            if !left.eq_ignore_ascii_case(right) {
                return false;
            }
            matched = true;
        }
    }
    matched
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
        if item.load_warning.as_deref() == Some("mods.higherVersionPresent") {
            item.load_warning = None;
        }
        if item.status == "duplicate_lower_version" {
            // Runtime callers recompute source/update status before this pass.
            // A standalone repeat cannot recover that old source conclusion.
            item.status = "unknown".into();
        }
    }
    for index in 0..mods.len() {
        if !mods[index].enabled {
            continue;
        }
        let peers = mods
            .iter()
            .enumerate()
            .filter(|(peer_index, item)| {
                *peer_index != index
                    && item.enabled
                    && same_plugin_identity(
                        &PluginIdentity::from_installed(&mods[index], config),
                        &PluginIdentity::from_installed(item, config),
                    )
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
        mods[index].duplicate_guid = true;
        let duplicate_version = peers
            .iter()
            .any(|peer| mods_share_guid_and_version(&mods[index], &mods[*peer]));
        mods[index].duplicate_version = duplicate_version;
        let Some(current) = mods[index].version.as_deref() else {
            continue;
        };
        let has_higher = peers.iter().any(|peer| {
            mods[*peer]
                .version
                .as_deref()
                .and_then(|higher| compare_mod_versions(higher, current))
                == Some(Ordering::Greater)
        });
        if has_higher {
            log::warn!(
                "重复 MOD 中存在更高版本：名称={}，当前版本={}",
                mods[index].name,
                current
            );
            mods[index].load_warning = Some("mods.higherVersionPresent".into());
            // The older equivalent copy is a cleanup candidate, including a
            // predecessor GUID that BepInEx would otherwise load separately.
            mods[index].update_available = false;
            if mods[index].status != "not_release_asset" {
                mods[index].status = "duplicate_lower_version".into();
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
