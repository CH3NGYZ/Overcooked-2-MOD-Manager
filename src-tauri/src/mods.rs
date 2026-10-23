use crate::archive::{self, InstallManifest};
use crate::game::read_file_metadata;
use crate::models::{
    AppConfig, Channel, InstalledMod, InstalledModStatusLabel, RepositoryBinding, RepositoryConfig,
    RepositoryDownloadMode,
};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn scan_mods_with_manifests(
    game: &Path,
    config: &AppConfig,
    manifest_dir: Option<&Path>,
) -> Vec<InstalledMod> {
    let plugins = game.join("BepInEx/plugins");
    log::debug!("开始扫描本地 MOD：插件目录={}", plugins.display());
    if !plugins.is_dir() {
        log::warn!("插件目录不存在，跳过 MOD 扫描：{}", plugins.display());
        return Vec::new();
    }
    let manifests = manifest_dir
        .map(archive::read_manifests)
        .unwrap_or_default();
    let mut paths = Vec::new();
    let mut traversal_errors = 0usize;
    for entry in WalkDir::new(&plugins) {
        match entry {
            Ok(entry) if entry.file_type().is_file() && is_mod_file(entry.path()) => {
                paths.push(entry.into_path());
            }
            Ok(_) => {}
            Err(error) => {
                traversal_errors += 1;
                let relative_path = error
                    .path()
                    .and_then(|path| path.strip_prefix(&plugins).ok())
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<unknown>".into());
                log::warn!(
                    "遍历 MOD 插件目录失败：相对路径={}，错误类别={:?}",
                    relative_path,
                    error.io_error().map(std::io::Error::kind)
                );
            }
        }
    }
    log::debug!(
        "发现候选 MOD 文件：数量={}，清单数量={}",
        paths.len(),
        manifests.len()
    );
    // If both names exist, the enabled DLL is the file BepInEx will load.
    // Keep one logical MOD row and let the enabled copy win.
    paths.sort_by_key(|path| is_disabled_mod_file(path));
    let mut seen = HashSet::new();
    let mut installed = paths
        .into_iter()
        .filter(|path| seen.insert(canonical_dll_file(path, game).to_ascii_lowercase()))
        .map(|path| inspect_dll(&path, game, config, &manifests))
        .collect::<Vec<_>>();
    annotate_package_counts(&mut installed);
    mark_duplicate_guids(&mut installed);
    refresh_status_labels(&mut installed);
    let packages = installed
        .iter()
        .filter_map(|item| item.package_id.as_ref())
        .collect::<HashSet<_>>()
        .len();
    let updates = installed
        .iter()
        .filter(|item| item.update_available)
        .count();
    let duplicates = installed.iter().filter(|item| item.duplicate_guid).count();
    log::info!(
        "本地 MOD 扫描完成：MOD数量={}，MOD包数量={}，可更新={}，重复身份={}，目录遍历错误={}",
        installed.len(),
        packages,
        updates,
        duplicates,
        traversal_errors
    );
    installed
}

fn is_mod_file(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.to_ascii_lowercase().ends_with(".dll")
        || name.to_ascii_lowercase().ends_with(".dll.disabled")
}

fn is_disabled_mod_file(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".dll.disabled")
}

pub fn canonical_dll_file(path: &Path, game: &Path) -> String {
    let relative = path
        .strip_prefix(game)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    canonical_dll_name(&relative)
}

pub fn canonical_dll_name(dll_file: &str) -> String {
    if dll_file.to_ascii_lowercase().ends_with(".disabled") {
        dll_file[..dll_file.len() - ".disabled".len()].to_string()
    } else {
        dll_file.to_string()
    }
}

pub fn disabled_dll_file(dll_file: &str) -> String {
    format!("{dll_file}.disabled")
}

fn inspect_dll(
    path: &Path,
    game: &Path,
    config: &AppConfig,
    manifests: &[InstallManifest],
) -> InstalledMod {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!(
                "读取 MOD 文件失败，按空元数据处理：文件={}，原因={}",
                path.display(),
                error
            );
            Vec::new()
        }
    };
    let plugin = parse_bepin_plugin_metadata(&bytes);
    let metadata = read_file_metadata(path);
    let guid = plugin.as_ref().and_then(|item| item.guid.clone());
    let plugin_version = plugin.as_ref().and_then(|item| item.version.clone());
    let (installed_version, version_source) = choose_local_version(plugin_version.as_deref());
    let dll_file = canonical_dll_file(path, game);
    let manifest = manifests
        .iter()
        .filter(|manifest| {
            manifest
                .files
                .iter()
                .any(|file| canonical_dll_name(file).eq_ignore_ascii_case(&dll_file))
        })
        .max_by(|left, right| left.installed_at.cmp(&right.installed_at));
    let source_repository = manifest.and_then(|manifest| {
        config
            .repositories
            .iter()
            .find(|repo| repo.id == manifest.repository_id)
    });
    let repository_binding = manifest
        .and_then(|manifest| manifest.repository_binding.as_ref())
        .filter(|binding| binding.is_valid());
    let candidates = config.repositories.iter().filter(|repo| {
        repo.enabled
            && crate::config::repository_matches_channel(
                config,
                repo,
                crate::config::current_user_region(),
            )
            && if let Some(binding) = repository_binding {
                binding.matches_repository(repo)
            } else {
                repository_matches_mod(
                    repo,
                    guid.as_deref(),
                    metadata.product_name.as_deref(),
                    metadata.original_filename.as_deref(),
                    installed_version.as_deref(),
                )
            }
    });
    let candidates = candidates.collect::<Vec<_>>();
    let repository = if let Some(binding) = repository_binding {
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
        select_repository(&candidates, installed_version.as_deref(), source_repository)
    };
    let repository_ids = repository
        .map(|bound| {
            candidates
                .iter()
                .filter(|repo| repo.download_mode == bound.download_mode)
                .map(|repo| repo.id.clone())
                .collect()
        })
        .unwrap_or_default();
    log::trace!(
        "分析 MOD：文件={}，GUID={}，版本={}，候选仓库={}，绑定={}",
        path.display(),
        guid.as_deref().unwrap_or("未知"),
        installed_version.as_deref().unwrap_or("未知"),
        candidates.len(),
        repository.is_some()
    );
    let name = plugin
        .as_ref()
        .and_then(|item| item.name.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            metadata
                .product_name
                .clone()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| {
            Path::new(&canonical_dll_file(path, game))
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("未知 MOD")
                .to_string()
        });
    let (update_available, status) = if let Some(binding) = repository_binding {
        binding_update_state(repository, binding)
    } else {
        repository_update_state(repository, installed_version.as_deref())
    };
    if update_available {
        let remote = repository.and_then(|repo| {
            if let Some(binding) = repository_binding {
                bound_release(repo, binding).map(|release| release.tag_name.clone())
            } else {
                repository_remote_version(repo)
            }
        });
        log::debug!(
            "检测到 MOD 可更新：文件={}，本地版本={}，远程版本={}",
            path.display(),
            repository_binding
                .map(|binding| binding.tag.as_str())
                .or(installed_version.as_deref())
                .unwrap_or("未知"),
            remote.as_deref().unwrap_or("未知")
        );
    }
    let product_name = metadata.product_name;
    let original_filename = metadata.original_filename;
    let plugin_name = plugin.and_then(|item| item.name);
    let package_id = manifest.map(|item| item.repository_id.clone());
    let package_name = manifest.and_then(|item| {
        item.package_name
            .clone()
            .or_else(|| {
                config
                    .repositories
                    .iter()
                    .find(|repo| repo.id == item.repository_id)
                    .map(|repo| repo.name.clone())
            })
            .or_else(|| (!item.release_tag.trim().is_empty()).then(|| item.release_tag.clone()))
    });
    let enabled = !is_disabled_mod_file(path);
    let note = config
        .mod_notes
        .iter()
        .find(|item| {
            item.key == note_key_for_values(guid.as_deref(), product_name.as_deref(), &dll_file)
        })
        .map(|item| item.note.clone())
        .or_else(|| manifest.and_then(|item| item.note.clone()));
    InstalledMod {
        repository_id: repository.map(|repo| repo.id.clone()),
        repository_ids,
        repository_binding: repository_binding.cloned(),
        package_id,
        package_name,
        package_mod_count: 0,
        name,
        title: metadata.title,
        description: metadata.description,
        product_name,
        original_filename,
        has_config: guid
            .as_deref()
            .is_some_and(|guid| crate::mod_config::exists(game, guid)),
        guid,
        plugin_name,
        company: metadata.company,
        copyright: metadata.copyright,
        dll_file,
        enabled,
        version: installed_version,
        version_source: version_source.into(),
        update_available,
        labels: Vec::new(),
        duplicate_guid: false,
        duplicate_version: false,
        load_warning: None,
        note,
        status: status.into(),
    }
}

fn select_repository<'a>(
    candidates: &[&'a RepositoryConfig],
    installed_version: Option<&str>,
    source: Option<&RepositoryConfig>,
) -> Option<&'a RepositoryConfig> {
    // A real Raw install cannot silently become a Release install because the
    // same plugin GUID also occurs in a repository with a newer cached tag.
    if let Some(source) =
        source.filter(|repo| repo.download_mode == RepositoryDownloadMode::RepositoryFile)
    {
        return candidates.iter().copied().find(|repo| repo.id == source.id);
    }
    let count = installed_version_component_count(installed_version);
    let preferred_channel = match count {
        Some(4) => Some(Channel::Beta),
        Some(_) => Some(Channel::Stable),
        None => None,
    };
    let compatible_release = |repo: &&RepositoryConfig| {
        repo.download_mode == RepositoryDownloadMode::Release
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
                repo.download_mode == RepositoryDownloadMode::Release && repo.channel == *channel
            })
        })
        .or_else(|| candidates.iter().copied().find(compatible_release));
    // Raw has no Release channel, regardless of the number of version parts.
    release.or_else(|| {
        candidates
            .iter()
            .copied()
            .find(|repo| repo.download_mode == RepositoryDownloadMode::RepositoryFile)
    })
}

fn repository_remote_version(repo: &RepositoryConfig) -> Option<String> {
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

fn repository_update_state(
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

fn bound_release<'a>(
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

fn binding_update_state(
    repo: Option<&RepositoryConfig>,
    binding: &RepositoryBinding,
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
    // Exact tags also work for non-SemVer releases; comparable versions must
    // never turn an installed newer tag into a downgrade notification.
    let update = match (
        crate::releases::parse_version(&release.tag_name),
        crate::releases::parse_version(&binding.tag),
    ) {
        (Some(remote), Some(local)) => remote > local,
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
        Some(binding) => binding_update_state(Some(repo), binding),
        None => repository_update_state(Some(repo), installed.version.as_deref()),
    }
}

fn annotate_package_counts(mods: &mut [InstalledMod]) {
    let mut counts = HashMap::<String, usize>::new();
    for item in mods.iter().filter_map(|item| item.package_id.as_ref()) {
        *counts.entry(item.clone()).or_default() += 1;
    }
    for item in mods {
        item.package_mod_count = item
            .package_id
            .as_ref()
            .and_then(|id| counts.get(id).copied())
            .unwrap_or(0);
    }
}

fn repository_matches_mod(
    repo: &crate::models::RepositoryConfig,
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

fn choose_local_version(plugin_version: Option<&str>) -> (Option<String>, &'static str) {
    if let Some(version) = plugin_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        (Some(version.to_string()), "BepInPlugin")
    } else {
        (None, "unknown")
    }
}

#[derive(Debug, Clone)]
struct PluginMetadata {
    guid: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

fn parse_bepin_plugin_metadata(bytes: &[u8]) -> Option<PluginMetadata> {
    if !String::from_utf8_lossy(bytes).contains("BepInPlugin") {
        return None;
    }
    let version_pattern = Regex::new(r"(?i)^(?:\d+\.){1,3}\d+(?:[-+][0-9a-z.-]+)?$").ok()?;
    for (position, window) in bytes.windows(2).enumerate() {
        if window != [1, 0] {
            continue;
        }
        let mut offset = position + 2;
        let Some(guid) = read_serialized_string(bytes, &mut offset) else {
            continue;
        };
        let Some(name) = read_serialized_string(bytes, &mut offset) else {
            continue;
        };
        let Some(version) = read_serialized_string(bytes, &mut offset) else {
            continue;
        };
        let guid = guid.trim_matches(['{', '}']);
        if !guid.is_empty()
            && guid.len() <= 256
            && !guid.chars().any(|character| character.is_control())
            && !name.trim().is_empty()
            && version_pattern.is_match(version)
        {
            return Some(PluginMetadata {
                guid: Some(guid.to_string()),
                name: Some(name.to_string()),
                version: Some(version.to_string()),
            });
        }
    }
    None
}

fn read_serialized_string<'a>(bytes: &'a [u8], offset: &mut usize) -> Option<&'a str> {
    let first = *bytes.get(*offset)?;
    *offset += 1;
    let length = if first & 0x80 == 0 {
        first as usize
    } else if first & 0xc0 == 0x80 {
        let second = *bytes.get(*offset)?;
        *offset += 1;
        (((first & 0x3f) as usize) << 8) | second as usize
    } else if first & 0xe0 == 0xc0 {
        let remaining = bytes.get(*offset..*offset + 3)?;
        *offset += 3;
        (((first & 0x1f) as usize) << 24)
            | ((remaining[0] as usize) << 16)
            | ((remaining[1] as usize) << 8)
            | remaining[2] as usize
    } else {
        return None;
    };
    let end = offset.checked_add(length)?;
    let value = std::str::from_utf8(bytes.get(*offset..end)?).ok()?;
    *offset = end;
    Some(value)
}

fn refresh_status_labels(mods: &mut [InstalledMod]) {
    for item in mods {
        let mut labels = Vec::new();
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
        if item.duplicate_guid && item.status != "duplicate_lower_version" {
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

fn note_key_for_values(guid: Option<&str>, product_name: Option<&str>, dll_file: &str) -> String {
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

fn mark_duplicate_guids(mods: &mut [InstalledMod]) {
    for index in 0..mods.len() {
        if !mods[index].enabled {
            continue;
        }
        let peers = mods
            .iter()
            .enumerate()
            .filter(|(peer_index, item)| {
                *peer_index != index && item.enabled && mods_share_identity(&mods[index], item)
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
            mods[index].load_warning = Some("存在更高版本，建议卸载".into());
            // BepInEx loads the higher local copy, so the shadowed DLL is a
            // cleanup candidate rather than another pending online update.
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

fn mods_share_identity(left: &InstalledMod, right: &InstalledMod) -> bool {
    let same_guid = left
        .guid
        .as_deref()
        .zip(right.guid.as_deref())
        .is_some_and(|(left, right)| left.trim().eq_ignore_ascii_case(right.trim()));
    let same_name = {
        let left = left.name.trim();
        let right = right.name.trim();
        !left.is_empty() && !right.is_empty() && left.eq_ignore_ascii_case(right)
    };
    same_guid || same_name
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

fn compare_mod_versions(left: &str, right: &str) -> Option<Ordering> {
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

fn installed_version_component_count(value: Option<&str>) -> Option<usize> {
    release_version_component_count(value)
}

fn release_version_component_count(value: Option<&str>) -> Option<usize> {
    let value = value?.trim().trim_start_matches(['v', 'V']);
    let numeric = value.split(['-', '+']).next()?;
    let count = numeric.split('.').count();
    (count >= 2 && count <= 4).then_some(count)
}

#[cfg(test)]
mod tests {
    fn plugin_blob(guid: &str, name: &str, version: &str) -> Vec<u8> {
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in [guid, name, version] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes
    }

    fn duplicate_configuration_manager(
        version: &str,
        online_version: &str,
    ) -> crate::models::InstalledMod {
        serde_json::from_value(serde_json::json!({
            "repository_id": "configuration-manager", "repository_ids": ["configuration-manager"],
            "name": "Configuration Manager", "guid": "com.bepis.bepinex.configurationmanager",
            "dll_file": format!("BepInEx/plugins/{version}/ConfigurationManager.dll"),
            "enabled": true, "version": version, "version_source": "BepInPlugin",
            "update_available": super::compare_mod_versions(online_version, version) == Some(std::cmp::Ordering::Greater),
            "duplicate_guid": false, "status": "latest"
        })).unwrap()
    }

    #[test]
    fn disabled_mod_keeps_update_label_and_legacy_json_defaults_labels() {
        let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
            "repository_id": "configuration-manager", "repository_ids": ["configuration-manager"],
            "name": "Configuration Manager", "dll_file": "BepInEx/plugins/ConfigurationManager.dll",
            "enabled": false, "version": "18.3", "version_source": "BepInPlugin",
            "update_available": true, "duplicate_guid": false, "status": "update_available"
        }))
        .unwrap();
        assert!(item.labels.is_empty());
        super::refresh_status_labels(std::slice::from_mut(&mut item));
        assert_eq!(
            item.labels,
            [
                crate::models::InstalledModStatusLabel::UpdateAvailable,
                crate::models::InstalledModStatusLabel::Disabled
            ]
        );
        assert_eq!(
            serde_json::to_value(&item).unwrap()["labels"],
            serde_json::json!(["update_available", "disabled"])
        );
    }

    #[test]
    fn shadowed_old_configuration_manager_is_not_an_available_update() {
        let mut installed = vec![
            duplicate_configuration_manager("18.3", "19.0"),
            duplicate_configuration_manager("19.0", "19.0"),
        ];
        assert!(installed[0].update_available);
        super::mark_duplicate_guids(&mut installed);
        assert_eq!(installed[0].status, "duplicate_lower_version");
        assert_eq!(
            installed[0].load_warning.as_deref(),
            Some("存在更高版本，建议卸载")
        );
        assert_eq!(
            installed
                .iter()
                .filter(|item| item.update_available)
                .count(),
            0
        );
    }

    #[test]
    fn newer_online_configuration_manager_keeps_the_effective_local_copy_update() {
        let mut installed = vec![
            duplicate_configuration_manager("18.3", "20.0"),
            duplicate_configuration_manager("19.0", "20.0"),
        ];
        super::mark_duplicate_guids(&mut installed);
        assert!(!installed[0].update_available);
        assert!(installed[1].update_available);
        assert_eq!(
            installed
                .iter()
                .filter(|item| item.update_available)
                .count(),
            1
        );
    }

    #[test]
    fn disabled_higher_copy_does_not_hide_the_loaded_mod_update() {
        let mut installed = vec![
            duplicate_configuration_manager("18.3", "19.0"),
            duplicate_configuration_manager("19.0", "19.0"),
        ];
        installed[1].enabled = false;
        super::mark_duplicate_guids(&mut installed);
        assert!(installed[0].update_available);
        assert!(installed[0].load_warning.is_none());
        assert_ne!(installed[0].status, "duplicate_lower_version");
    }

    #[test]
    fn same_version_duplicates_keep_updates_and_raw_keeps_its_source_status() {
        let mut same_version = vec![
            duplicate_configuration_manager("19.0", "20.0"),
            duplicate_configuration_manager("19.0", "20.0"),
        ];
        super::mark_duplicate_guids(&mut same_version);
        assert!(
            same_version
                .iter()
                .all(|item| item.duplicate_version && item.update_available)
        );
        let mut raw = duplicate_configuration_manager("18.3", "19.0");
        raw.status = "not_release_asset".into();
        raw.update_available = false;
        let mut raw_and_release = vec![raw, duplicate_configuration_manager("19.0", "19.0")];
        super::mark_duplicate_guids(&mut raw_and_release);
        assert_eq!(raw_and_release[0].status, "not_release_asset");
        assert!(!raw_and_release[0].update_available);
    }

    #[test]
    fn disabled_dll_names_keep_the_same_logical_path() {
        assert_eq!(
            super::canonical_dll_name("BepInEx/plugins/example.dll.disabled"),
            "BepInEx/plugins/example.dll"
        );
        assert_eq!(
            super::disabled_dll_file("BepInEx/plugins/example.dll"),
            "BepInEx/plugins/example.dll.disabled"
        );
    }

    #[test]
    fn unknown_version_is_not_update() {
        assert!(!matches!(
            crate::releases::parse_version("unknown"),
            Some(_)
        ));
    }

    #[test]
    fn local_release_channel_uses_component_count() {
        assert_eq!(
            super::installed_version_component_count(Some("1.8.5")),
            Some(3)
        );
        assert_eq!(
            super::installed_version_component_count(Some("1.8.5.2")),
            Some(4)
        );
        assert_eq!(
            super::installed_version_component_count(Some("v1.8.5.2-beta")),
            Some(4)
        );
    }

    #[test]
    fn mod_version_comparison_fills_missing_components_with_zero() {
        assert_eq!(
            super::compare_mod_versions("1.8.5.03", "1.8.5"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            super::compare_mod_versions("1.8.5", "1.8.5.0"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(
            super::compare_mod_versions("1.10.0", "1.9.99.99"),
            Some(std::cmp::Ordering::Greater)
        );
    }

    #[test]
    fn local_version_only_uses_bepinplugin_and_preserves_zero_versions() {
        for version in ["0.10.0", "1.2.3.4", "0.0.0", "0.0.0.0"] {
            assert_eq!(
                super::choose_local_version(Some(version)),
                (Some(version.into()), "BepInPlugin")
            );
        }
        assert_eq!(
            super::choose_local_version(Some(" 0.10.0 ")),
            (Some("0.10.0".into()), "BepInPlugin")
        );
        for missing in [None, Some(""), Some(" \t\r\n")] {
            assert_eq!(super::choose_local_version(missing), (None, "unknown"));
        }
    }

    fn binding_repository() -> crate::models::RepositoryConfig {
        crate::models::RepositoryConfig {
            id: "test".into(),
            name: "Test".into(),
            name_i18n: std::collections::BTreeMap::new(),
            group: String::new(),
            group_i18n: std::collections::BTreeMap::new(),
            provider: crate::models::Provider::Gitee,
            username: "example".into(),
            repository: "mod".into(),
            download_mode: crate::models::RepositoryDownloadMode::Release,
            branch: String::new(),
            file_path: String::new(),
            region: None,
            channel: crate::models::Channel::All,
            asset_rules: Vec::new(),
            install_target: String::new(),
            note: String::new(),
            note_i18n: std::collections::BTreeMap::new(),
            plugin_guid: "com.example.guid".into(),
            local_product_name: "Example Product".into(),
            local_original_filename: "Example.dll".into(),
            version_regex: String::new(),
            max_release_count: 10,
            download_threads: 1,
            github_token: String::new(),
            gitee_token: String::new(),
            official_api_only: true,
            enabled: true,
            builtin: false,
            pinned_version: None,
            ignored_version: None,
            etag: None,
            cached_releases: Vec::new(),
            last_error: None,
            source_id: None,
        }
    }

    #[test]
    fn explicit_repository_tags_override_dll_versions_for_every_source_type() {
        for source in [Some("official"), Some(crate::community::SOURCE_ID), None] {
            let mut repo = binding_repository();
            repo.source_id = source.map(str::to_string);
            repo.builtin = source == Some("official");
            repo.cached_releases = vec![high_cached_release()];
            let binding = crate::models::RepositoryBinding::from_release(&repo, "v98.0.0").unwrap();
            assert_eq!(
                super::binding_update_state(Some(&repo), &binding),
                (true, "update_available")
            );
            let mut current = binding.clone();
            current.tag = "v99.0.0".into();
            assert_eq!(
                super::binding_update_state(Some(&repo), &current),
                (false, "latest")
            );
            current.tag = "v100.0.0".into();
            assert_eq!(
                super::binding_update_state(Some(&repo), &current),
                (false, "latest")
            );
            repo.last_error = Some("request failed".into());
            assert_eq!(
                super::binding_update_state(Some(&repo), &binding),
                (false, "repository_unavailable")
            );
        }
    }

    #[test]
    fn opaque_tags_and_channel_provider_boundaries_are_preserved() {
        let mut repo = binding_repository();
        repo.channel = crate::models::Channel::Stable;
        let binding = crate::models::RepositoryBinding::from_release(&repo, "old-nightly").unwrap();
        let mut beta = high_cached_release();
        beta.tag_name = "beta-nightly".into();
        beta.prerelease = true;
        let mut stable = high_cached_release();
        stable.tag_name = "new-nightly".into();
        repo.channel = crate::models::Channel::All;
        repo.cached_releases = vec![beta, stable];
        assert_eq!(
            super::bound_release(&repo, &binding).unwrap().tag_name,
            "new-nightly"
        );
        assert_eq!(
            super::binding_update_state(Some(&repo), &binding),
            (true, "update_available")
        );
        let mut current = binding.clone();
        current.tag = "new-nightly".into();
        assert_eq!(
            super::binding_update_state(Some(&repo), &current),
            (false, "latest")
        );
        repo.provider = crate::models::Provider::Github;
        assert!(!binding.matches_repository(&repo));
        repo.provider = binding.provider.clone();
        repo.channel = crate::models::Channel::Beta;
        assert!(!binding.matches_repository(&repo));
        repo.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        assert!(crate::models::RepositoryBinding::from_release(&repo, "tag").is_none());
        assert_eq!(
            super::binding_update_state(None, &binding),
            (false, "bound_repository_missing")
        );
    }

    #[test]
    fn real_manifest_scan_keeps_dll_metadata_but_selects_owner_repository_tag_first() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/community-20261013")
            .join(format!("binding-scan-{}", uuid::Uuid::new_v4()));
        let relative = "BepInEx/plugins/Example.dll";
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in ["com.example.guid", "Example", "100.0.0"] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        std::fs::write(&path, &bytes).unwrap();
        let mut bound = binding_repository();
        bound.id = "refreshed-source-id".into();
        bound.plugin_guid.clear();
        bound.local_product_name.clear();
        bound.local_original_filename.clear();
        bound.cached_releases = vec![high_cached_release()];
        let mut misleading = binding_repository();
        misleading.id = "dll-matching-but-wrong-repository".into();
        misleading.repository = "unrelated".into();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1, "ui_zoom":1.5, "onboarding_seen_steps":[],
            "game":{"path":"", "auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[misleading, bound]
        })).unwrap();
        let manifests = root.join("manifests");
        let mut manifest = crate::archive::new_manifest(
            "old-source-id",
            "v98.0.0",
            Some("Package"),
            None,
            vec![relative.into()],
        );
        manifest.repository_binding =
            crate::models::RepositoryBinding::from_release(&bound, "v98.0.0");
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(scan.len(), 1);
        let installed = &scan[0];
        assert_eq!(installed.guid.as_deref(), Some("com.example.guid"));
        assert_eq!(installed.version.as_deref(), Some("100.0.0"));
        assert_eq!(
            installed.repository_id.as_deref(),
            Some("refreshed-source-id")
        );
        assert_eq!(installed.repository_ids, ["refreshed-source-id"]);
        assert_eq!(installed.package_id.as_deref(), Some("old-source-id"));
        assert_eq!(installed.repository_binding, manifest.repository_binding);
        assert!(installed.update_available);
        assert_eq!(
            super::local_update_version(installed).as_deref(),
            Some("v98.0.0")
        );

        std::fs::rename(&path, root.join(format!("{relative}.disabled"))).unwrap();
        let disabled = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert!(!disabled[0].enabled);
        assert_eq!(disabled[0].repository_binding, installed.repository_binding);
        assert_eq!(disabled[0].package_id, installed.package_id);

        manifest.repository_binding = None;
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let legacy = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(
            legacy[0].repository_id.as_deref(),
            Some("dll-matching-but-wrong-repository")
        );
        assert!(legacy[0].repository_binding.is_none());
        let verified = std::fs::canonicalize(&root).unwrap();
        assert!(verified.starts_with(
            std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
        ));
        std::fs::remove_dir_all(verified).unwrap();
    }

    #[test]
    fn channel_bound_install_only_checks_its_own_release_track() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/channel-binding")
            .join(format!("scan-{}", uuid::Uuid::new_v4()));
        let relative = "BepInEx/plugins/Example.dll";
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, plugin_blob("com.example.guid", "Example", "100.0.0")).unwrap();
        let mut stable = binding_repository();
        stable.id = "stable-track".into();
        stable.channel = crate::models::Channel::Stable;
        stable.cached_releases = vec![high_cached_release()];
        let mut beta = binding_repository();
        beta.id = "beta-track".into();
        beta.channel = crate::models::Channel::Beta;
        let mut beta_release = high_cached_release();
        beta_release.tag_name = "v99.0.0-beta.1".into();
        beta_release.version = Some("99.0.0-beta.1".into());
        beta_release.prerelease = true;
        beta.cached_releases = vec![beta_release];
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1, "ui_zoom":1.5, "onboarding_seen_steps":[],
            "game":{"path":"", "auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[stable, beta]
        })).unwrap();
        let manifests = root.join("manifests");
        let mut manifest = crate::archive::new_manifest(
            "package",
            "v98.0.0-beta.1",
            Some("Package"),
            None,
            vec![relative.into()],
        );
        manifest.repository_binding =
            crate::models::RepositoryBinding::from_release(&beta, "v98.0.0-beta.1");
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(scan.len(), 1);
        assert_eq!(scan[0].repository_id.as_deref(), Some("beta-track"));
        assert_eq!(scan[0].repository_ids, ["beta-track"]);
        assert!(scan[0].update_available);

        manifest.repository_binding =
            crate::models::RepositoryBinding::from_release(&stable, "v98.0.0");
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let stable_scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(
            stable_scan[0].repository_id.as_deref(),
            Some("stable-track")
        );
        assert_eq!(stable_scan[0].repository_ids, ["stable-track"]);
        assert!(stable_scan[0].update_available);

        let verified = std::fs::canonicalize(&root).unwrap();
        assert!(verified.starts_with(
            std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
        ));
        std::fs::remove_dir_all(verified).unwrap();
    }

    #[test]
    fn repository_binding_matches_any_configured_identity() {
        let repo = binding_repository();
        assert!(super::repository_matches_mod(
            &repo,
            Some("different"),
            Some("example product"),
            None,
            None
        ));
        assert!(super::repository_matches_mod(
            &repo,
            None,
            None,
            Some("EXAMPLE.DLL"),
            None
        ));
        assert!(super::repository_matches_mod(
            &repo,
            Some("COM.EXAMPLE.GUID"),
            None,
            None,
            None
        ));
        let mut versioned = repo.clone();
        versioned.version_regex = r"^1\.2\.\d+$".into();
        assert!(super::repository_matches_mod(
            &versioned,
            Some("com.example.guid"),
            None,
            None,
            Some("1.2.3")
        ));
        assert!(!super::repository_matches_mod(
            &versioned,
            Some("com.example.guid"),
            None,
            None,
            Some("1.3.0")
        ));
    }

    fn high_cached_release() -> crate::models::ReleaseInfo {
        crate::models::ReleaseInfo {
            release_id: None,
            tag_name: "v99.0.0".into(),
            version: Some("99.0.0".into()),
            name: "newer Release".into(),
            body: String::new(),
            published_at: None,
            prerelease: false,
            draft: false,
            assets: Vec::new(),
        }
    }

    #[test]
    fn failed_release_refresh_does_not_confirm_a_cached_version() {
        for cached_version in ["1.2.3", "99.0.0"] {
            let mut repo = binding_repository();
            let mut release = high_cached_release();
            release.version = Some(cached_version.into());
            release.tag_name = format!("v{cached_version}");
            repo.cached_releases = vec![release];
            repo.last_error = Some("Release request failed".into());
            assert_eq!(super::repository_remote_version(&repo), None);
            assert_eq!(
                super::repository_update_state(Some(&repo), Some("1.2.3")),
                (false, "repository_unavailable")
            );
        }
    }

    #[test]
    fn release_state_requires_comparable_local_and_remote_versions() {
        let mut repo = binding_repository();
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "unknown")
        );
        let mut release = high_cached_release();
        release.version = Some("not-a-version".into());
        repo.cached_releases = vec![release];
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "unknown")
        );
        repo.cached_releases = vec![high_cached_release()];
        for local_version in [None, Some("not-a-version")] {
            assert_eq!(
                super::repository_update_state(Some(&repo), local_version),
                (false, "unknown")
            );
        }
    }

    #[test]
    fn successful_release_retry_restores_version_comparison() {
        let mut repo = binding_repository();
        repo.last_error = Some("Release request failed".into());
        repo.cached_releases = vec![high_cached_release()];
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "repository_unavailable")
        );
        repo.last_error = None;
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (true, "update_available")
        );
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("99.0.0")),
            (false, "latest")
        );
        repo.cached_releases.clear();
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "unknown")
        );
    }

    #[test]
    fn failed_release_source_does_not_rebind_to_another_cached_repository() {
        let mut source = binding_repository();
        source.id = "source".into();
        source.channel = crate::models::Channel::Stable;
        source.cached_releases = vec![high_cached_release()];
        source.last_error = Some("Release request failed".into());
        let mut mirror = source.clone();
        mirror.id = "mirror".into();
        mirror.last_error = None;
        let selected =
            super::select_repository(&[&mirror, &source], Some("1.2.3"), Some(&source)).unwrap();
        assert_eq!(selected.id, "source");
        assert_eq!(
            super::repository_update_state(Some(selected), Some("1.2.3")),
            (false, "repository_unavailable")
        );
    }

    #[test]
    fn raw_binding_uses_identity_and_version_regex_without_release_comparison() {
        let mut raw = binding_repository();
        raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        raw.cached_releases = vec![high_cached_release()];
        for (guid, product, filename) in [
            (Some("COM.EXAMPLE.GUID"), None, None),
            (None, Some("example product"), None),
            (None, None, Some("EXAMPLE.DLL")),
        ] {
            assert!(super::repository_matches_mod(
                &raw,
                guid,
                product,
                filename,
                Some("1.2.3.4")
            ));
        }
        assert_eq!(super::repository_remote_version(&raw), None);
        assert_eq!(
            super::repository_update_state(Some(&raw), Some("1.2.3.4")),
            (false, "not_release_asset")
        );
        raw.version_regex = r"^1\.2\.\d+\.\d+$".into();
        assert!(super::repository_matches_mod(
            &raw,
            Some("com.example.guid"),
            None,
            None,
            Some("1.2.3.4")
        ));
        assert!(!super::repository_matches_mod(
            &raw,
            Some("com.example.guid"),
            None,
            None,
            Some("1.3.0.0")
        ));
        assert!(!super::repository_matches_mod(
            &raw,
            Some("com.example.guid"),
            None,
            None,
            None
        ));
        assert!(!super::repository_matches_mod(
            &raw,
            Some("unknown.guid"),
            None,
            None,
            Some("1.2.3.4")
        ));
    }

    #[test]
    fn real_raw_source_wins_while_release_sources_keep_their_channel_rules() {
        let mut release = binding_repository();
        release.id = "release".into();
        release.channel = crate::models::Channel::Stable;
        let mut raw = binding_repository();
        raw.id = "raw".into();
        raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        // Raw selection ignores its unused channel field, including four-part versions.
        raw.channel = crate::models::Channel::Stable;
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3"), Some(&raw))
                .unwrap()
                .id,
            "raw"
        );
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3"), Some(&release))
                .unwrap()
                .id,
            "release"
        );
        assert!(super::select_repository(&[&release], Some("1.2.3"), Some(&raw)).is_none());
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3.4"), None)
                .unwrap()
                .id,
            "raw"
        );
        release.channel = crate::models::Channel::Beta;
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3.4"), None)
                .unwrap()
                .id,
            "release"
        );
        assert!(super::select_repository(&[], Some("1.2.3.4"), None).is_none());
        assert_eq!(
            super::repository_update_state(None, Some("1.2.3")),
            (false, "unbound")
        );
        raw.channel = crate::models::Channel::All;
        assert_eq!(
            super::select_repository(&[&raw], Some("1.2.3.4"), None)
                .unwrap()
                .id,
            "raw"
        );
    }

    #[test]
    fn scanning_real_raw_manifest_prevents_same_guid_release_update() {
        for installed_version in ["1.2.3", "1.2.3.4"] {
            let root =
                std::env::temp_dir().join(format!("oc2-raw-binding-{}", uuid::Uuid::new_v4()));
            let relative = "BepInEx/plugins/Example.dll";
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut bytes = b"BepInPlugin".to_vec();
            bytes.extend_from_slice(&[1, 0]);
            for value in ["com.example.guid", "Example", installed_version] {
                bytes.push(value.len() as u8);
                bytes.extend_from_slice(value.as_bytes());
            }
            std::fs::write(&path, bytes).unwrap();
            let mut raw = binding_repository();
            raw.id = "raw".into();
            raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
            raw.cached_releases = vec![high_cached_release()];
            let mut release = raw.clone();
            release.id = "release".into();
            release.download_mode = crate::models::RepositoryDownloadMode::Release;
            let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[release,raw]
        })).unwrap();
            let manifest = crate::archive::InstallManifest {
                repository_id: "raw".into(),
                release_tag: String::new(),
                repository_binding: None,
                package_name: Some("Raw MOD".into()),
                note: None,
                installed_at: "2026-10-07T00:00:00Z".into(),
                files: vec![relative.into()],
            };
            let installed =
                super::inspect_dll(&path, &root, &config, std::slice::from_ref(&manifest));
            assert_eq!(installed.version.as_deref(), Some(installed_version));
            assert_eq!(installed.repository_id.as_deref(), Some("raw"));
            assert_eq!(installed.repository_ids, vec!["raw"]);
            assert_eq!(installed.status, "not_release_asset");
            assert!(!installed.update_available);
            assert!(!installed.has_config);
            std::fs::create_dir_all(root.join("BepInEx/config")).unwrap();
            std::fs::write(
                root.join("BepInEx/config/com.example.guid.cfg"),
                "[General]\nEnabled = true\n",
            )
            .unwrap();
            assert!(super::inspect_dll(&path, &root, &config, &[]).has_config);
            let release_install = super::inspect_dll(&path, &root, &config, &[]);
            assert_eq!(release_install.repository_id.as_deref(), Some("release"));
            assert!(release_install.update_available);
            let mut restricted = config;
            restricted.repositories[1].version_regex = r"^2\.".into();
            let invalid_raw = super::inspect_dll(&path, &root, &restricted, &[manifest]);
            assert_eq!(invalid_raw.status, "unbound");
            assert!(!invalid_raw.update_available);
            assert!(invalid_raw.repository_ids.is_empty());
            let target = std::fs::canonicalize(&root).unwrap();
            let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
            assert!(target.starts_with(&temp) && target != temp);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn bepin_plugin_metadata_provides_local_version() {
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "dev.gua.overcooked.diylevel",
                "Overcooked2 DIYLevel Plugin",
                "0.10.0"
            ))
            .and_then(|item| item.version),
            Some("0.10.0".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "com.example.mod",
                "HostUtilities",
                "1.2.3"
            ))
            .and_then(|item| item.version),
            Some("1.2.3".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "{12345678-1234-1234-1234-123456789abc}",
                "HostUtilities",
                "1.2.3.4"
            ))
            .and_then(|item| item.guid),
            Some("12345678-1234-1234-1234-123456789abc".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "HostUtilities",
                "HostUtilities",
                "1.2.3"
            ))
            .and_then(|item| item.guid),
            Some("HostUtilities".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "com.example.mod",
                "HostUtilities",
                "0.0.0"
            ))
            .and_then(|item| item.version),
            Some("0.0.0".into())
        );
    }

    #[test]
    fn local_mod_scan_returns_unknown_without_readable_bepinplugin_version() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/bepinplugin-version")
            .join(uuid::Uuid::new_v4().to_string());
        let plugins = root.join("BepInEx/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "ui_zoom": 1.5, "onboarding_seen_steps": [],
            "game": {"path": "", "auto_detect": false},
            "network": {"timeout_seconds": 5, "download_threads": 1, "max_retry_count": 3, "check_updates_on_start": false},
            "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
            "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
            "repositories": []
        })).unwrap();
        let mut truncated = plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "0.10.0");
        truncated.pop();
        let cases = [
            (
                "no-attribute",
                b"AssemblyVersion\0FileVersion\0ProductVersion\09.9.9.9".to_vec(),
            ),
            (
                "empty",
                plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", ""),
            ),
            (
                "blank",
                plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "   "),
            ),
            (
                "invalid",
                plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "not-a-version"),
            ),
            ("truncated", truncated),
        ];
        for (name, bytes) in cases {
            for suffix in [".dll", ".dll.disabled"] {
                let path = plugins.join(format!("{name}{suffix}"));
                std::fs::write(&path, &bytes).unwrap();
                let installed = super::inspect_dll(&path, &root, &config, &[]);
                assert_eq!(installed.version, None, "{name}{suffix}");
                assert_eq!(installed.version_source, "unknown", "{name}{suffix}");
                assert!(!installed.update_available);
                assert_eq!(installed.enabled, suffix == ".dll");
            }
        }
        let scan = super::scan_mods_with_manifests(&root, &config, None);
        assert_eq!(scan.len(), 5);
        assert!(
            scan.iter()
                .all(|item| item.version.is_none() && item.version_source == "unknown")
        );
        let verified = std::fs::canonicalize(&root).unwrap();
        let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(verified.starts_with(workspace.join("target/verification/bepinplugin-version")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lower_duplicate_guid_is_marked_as_not_loaded() {
        let mut mods = vec![
            crate::models::InstalledMod {
                repository_id: None,
                repository_ids: Vec::new(),
                repository_binding: None,
                package_id: None,
                package_name: None,
                package_mod_count: 0,
                name: "Old".into(),
                has_config: false,
                title: None,
                description: None,
                product_name: Some("Old".into()),
                original_filename: Some("old.dll".into()),
                guid: Some("com.example.mod".into()),
                plugin_name: None,
                company: None,
                copyright: None,
                dll_file: "BepInEx/plugins/old.dll".into(),
                enabled: true,
                version: Some("1.0.0".into()),
                version_source: "BepInPlugin".into(),
                update_available: false,
                labels: Vec::new(),
                duplicate_guid: false,
                duplicate_version: false,
                load_warning: None,
                note: None,
                status: "installed".into(),
            },
            crate::models::InstalledMod {
                repository_id: None,
                repository_ids: Vec::new(),
                repository_binding: None,
                package_id: None,
                package_name: None,
                package_mod_count: 0,
                name: "New".into(),
                has_config: false,
                title: None,
                description: None,
                product_name: Some("New".into()),
                original_filename: Some("new.dll".into()),
                guid: Some("com.example.mod".into()),
                plugin_name: None,
                company: None,
                copyright: None,
                dll_file: "BepInEx/plugins/new.dll".into(),
                enabled: true,
                version: Some("2.0.0".into()),
                version_source: "BepInPlugin".into(),
                update_available: false,
                labels: Vec::new(),
                duplicate_guid: false,
                duplicate_version: false,
                load_warning: None,
                note: None,
                status: "installed".into(),
            },
        ];
        super::mark_duplicate_guids(&mut mods);
        assert_eq!(
            mods[0].load_warning.as_deref(),
            Some("存在更高版本，建议卸载")
        );
        assert!(mods[1].load_warning.is_none());
        assert!(mods[0].duplicate_guid && mods[1].duplicate_guid);
        assert!(!mods[0].duplicate_version && !mods[1].duplicate_version);

        mods[1].version = Some("1.0.0".into());
        mods[0].load_warning = None;
        mods[0].status = "installed".into();
        mods[0].duplicate_guid = false;
        mods[1].duplicate_guid = false;
        super::mark_duplicate_guids(&mut mods);
        assert!(mods[0].duplicate_version && mods[1].duplicate_version);
    }
}
