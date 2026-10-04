use crate::game::read_file_metadata;
use crate::models::{AppConfig, InstalledMod};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn scan_mods(game: &Path, config: &AppConfig) -> Vec<InstalledMod> {
    let plugins = game.join("BepInEx/plugins");
    if !plugins.is_dir() {
        return Vec::new();
    }
    let mut paths = WalkDir::new(&plugins)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file() && is_mod_file(entry.path())
        })
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    // If both names exist, the enabled DLL is the file BepInEx will load.
    // Keep one logical MOD row and let the enabled copy win.
    paths.sort_by_key(|path| is_disabled_mod_file(path));
    let mut seen = HashSet::new();
    let mut installed = paths
        .into_iter()
        .filter(|path| seen.insert(canonical_dll_file(path, game).to_ascii_lowercase()))
        .map(|path| inspect_dll(&path, game, config))
        .collect::<Vec<_>>();
    mark_duplicate_guids(&mut installed);
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

fn inspect_dll(path: &Path, game: &Path, config: &AppConfig) -> InstalledMod {
    let bytes = fs::read(path).unwrap_or_default();
    let plugin = parse_bepin_plugin_metadata(&bytes);
    let metadata = read_file_metadata(path);
    let guid = plugin.as_ref().and_then(|item| item.guid.clone());
    let plugin_version = plugin.as_ref().and_then(|item| item.version.clone());
    let candidates = config.repositories.iter().filter(|repo| {
        repo.enabled && repository_matches_mod(
            repo,
            guid.as_deref(),
            metadata.product_name.as_deref(),
            metadata.original_filename.as_deref(),
        )
    });
    let (installed_version, version_source) =
        choose_local_version(metadata.product_version.as_deref(), plugin_version.as_deref());
    let local_component_count = installed_version_component_count(installed_version.as_deref());
    let candidates = candidates.collect::<Vec<_>>();
    let repository_ids = candidates
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<Vec<_>>();
    let repository = match local_component_count {
        Some(4) => candidates.iter()
            .find(|repo| repo.channel == crate::models::Channel::Beta)
            .or_else(|| candidates.iter().find(|repo| repo.channel == crate::models::Channel::All)),
        Some(_) => candidates.iter()
            .find(|repo| repo.channel == crate::models::Channel::Stable)
            .or_else(|| candidates.iter().find(|repo| repo.channel == crate::models::Channel::All)),
        None => candidates.first(),
    }.copied();
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
        .or_else(|| metadata.title.clone().filter(|value| !value.trim().is_empty()))
        .or_else(|| {
            metadata
                .original_filename
                .clone()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| repository.map(|repo| repo.name.clone()))
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("未知 MOD")
                .to_string()
        });
    let remote = repository.and_then(|repo| {
        repo.cached_releases
            .iter()
            .filter(|release| local_component_count
                .map(|count| release_version_component_count(Some(release.tag_name.as_str())) == Some(count))
                .unwrap_or(true))
            .next()
            .and_then(|release| release.version.clone())
    });
    let update_available = match (
        installed_version
            .as_deref()
            .and_then(crate::releases::parse_version),
        remote.as_deref().and_then(crate::releases::parse_version),
    ) {
        (Some(local), Some(remote)) => remote > local,
        _ => false,
    };
    let bound = repository.is_some();
    let product_name = metadata.product_name;
    let original_filename = metadata.original_filename;
    let plugin_name = plugin.and_then(|item| item.name);
    let dll_file = canonical_dll_file(path, game);
    let enabled = !is_disabled_mod_file(path);
    let note = config
        .mod_notes
        .iter()
        .find(|item| {
            item.key
                == note_key_for_values(
                    guid.as_deref(),
                    product_name.as_deref(),
                    &dll_file,
                )
        })
        .map(|item| item.note.clone());
    InstalledMod {
        repository_id: repository.map(|repo| repo.id.clone()),
        repository_ids,
        name,
        title: metadata.title,
        description: metadata.description,
        product_name,
        original_filename,
        guid,
        plugin_name,
        company: metadata.company,
        copyright: metadata.copyright,
        dll_file,
        enabled,
        version: installed_version,
        version_source: version_source.into(),
        update_available,
        duplicate_guid: false,
        load_warning: None,
        note,
        status: if !bound {
            "unbound".into()
        } else if update_available {
            "update_available".into()
        } else {
            "latest".into()
        },
    }
}

fn repository_matches_mod(
    repo: &crate::models::RepositoryConfig,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
) -> bool {
    [
        (guid, repo.plugin_guid.as_str()),
        (product_name, repo.local_product_name.as_str()),
        (original_filename, repo.local_original_filename.as_str()),
    ]
    .into_iter()
    .any(|(actual, configured)| {
        let configured = configured.trim();
        !configured.is_empty()
            && actual.is_some_and(|value| value.trim().eq_ignore_ascii_case(configured))
    })
}

fn is_zero_version(value: &str) -> bool {
    matches!(value.trim(), "0.0.0" | "0.0.0.0")
}

fn choose_local_version<'a>(
    product_version: Option<&'a str>,
    plugin_version: Option<&'a str>,
) -> (Option<String>, &'static str) {
    if let Some(version) = product_version.filter(|value| !is_zero_version(value)) {
        (Some(version.trim().to_string()), "ProductVersion")
    } else if let Some(version) = plugin_version {
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

fn note_key_for_values(
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

fn mark_duplicate_guids(mods: &mut [InstalledMod]) {
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
                    && mods_share_identity(&mods[index], item)
            })
            .map(|(peer_index, _)| peer_index)
            .collect::<Vec<_>>();
        if peers.is_empty() {
            continue;
        }
        mods[index].duplicate_guid = true;
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
            mods[index].load_warning = Some("存在更高版本，框架默认不加载".into());
            mods[index].status = "duplicate_lower_version".into();
        }
    }
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
    (count == 3 || count == 4).then_some(count)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(super::installed_version_component_count(Some("1.8.5")), Some(3));
        assert_eq!(super::installed_version_component_count(Some("1.8.5.2")), Some(4));
        assert_eq!(super::installed_version_component_count(Some("v1.8.5.2-beta")), Some(4));
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
    fn product_version_is_preferred_with_zero_fallback() {
        assert_eq!(
            super::choose_local_version(Some("1.2.3.4"), Some("1.2.3")),
            (Some("1.2.3.4".into()), "ProductVersion")
        );
        assert_eq!(
            super::choose_local_version(Some("0.0.0"), Some("1.2.3")),
            (Some("1.2.3".into()), "BepInPlugin")
        );
        assert_eq!(
            super::choose_local_version(Some("0.0.0.0"), Some("1.2.3.4")),
            (Some("1.2.3.4".into()), "BepInPlugin")
        );
        assert_eq!(super::choose_local_version(None, None), (None, "unknown"));
    }

    #[test]
    fn repository_binding_matches_any_configured_identity() {
        let repo = crate::models::RepositoryConfig {
            id: "test".into(),
            name: "Test".into(),
            name_i18n: std::collections::BTreeMap::new(),
            group: String::new(),
            group_i18n: std::collections::BTreeMap::new(),
            provider: crate::models::Provider::Gitee,
            url: String::new(),
            channel: crate::models::Channel::All,
            asset_rules: Vec::new(),
            install_target: String::new(),
            note: String::new(),
            note_i18n: std::collections::BTreeMap::new(),
            plugin_guid: "com.example.guid".into(),
            local_product_name: "Example Product".into(),
            local_original_filename: "Example.dll".into(),
            download_threads: 1,
            api_proxy_mode: crate::models::ProxyMode::Direct,
            api_proxy_url: String::new(),
            release_proxy_mode: crate::models::ProxyMode::Direct,
            release_proxy_url: String::new(),
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
        };
        assert!(super::repository_matches_mod(
            &repo,
            Some("different"),
            Some("example product"),
            None
        ));
        assert!(super::repository_matches_mod(&repo, None, None, Some("EXAMPLE.DLL")));
        assert!(super::repository_matches_mod(
            &repo,
            Some("COM.EXAMPLE.GUID"),
            None,
            None
        ));
    }

    #[test]
    fn bepin_plugin_metadata_provides_local_version() {
        fn plugin_blob(guid: &str, name: &str, version: &str) -> Vec<u8> {
            let mut bytes = b"BepInPlugin".to_vec();
            bytes.extend_from_slice(&[1, 0]);
            for value in [guid, name, version] {
                bytes.push(value.len() as u8);
                bytes.extend_from_slice(value.as_bytes());
            }
            bytes
        }
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob("com.example.mod", "HostUtilities", "1.2.3"))
                .and_then(|item| item.version),
            Some("1.2.3".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob("{12345678-1234-1234-1234-123456789abc}", "HostUtilities", "1.2.3.4"))
                .and_then(|item| item.guid),
            Some("12345678-1234-1234-1234-123456789abc".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob("HostUtilities", "HostUtilities", "1.2.3"))
                .and_then(|item| item.guid),
            Some("HostUtilities".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob("com.example.mod", "HostUtilities", "0.0.0"))
                .and_then(|item| item.version),
            Some("0.0.0".into())
        );
    }

    #[test]
    fn lower_duplicate_guid_is_marked_as_not_loaded() {
        let mut mods = vec![
            crate::models::InstalledMod {
                repository_id: None,
                repository_ids: Vec::new(),
                name: "Old".into(),
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
                duplicate_guid: false,
                load_warning: None,
                note: None,
                status: "installed".into(),
            },
            crate::models::InstalledMod {
                repository_id: None,
                repository_ids: Vec::new(),
                name: "New".into(),
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
                duplicate_guid: false,
                load_warning: None,
                note: None,
                status: "installed".into(),
            },
        ];
        super::mark_duplicate_guids(&mut mods);
        assert_eq!(
            mods[0].load_warning.as_deref(),
            Some("存在更高版本，框架默认不加载")
        );
        assert!(mods[1].load_warning.is_none());
        assert!(mods[0].duplicate_guid && mods[1].duplicate_guid);
    }
}
