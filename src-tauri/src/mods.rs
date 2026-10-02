use crate::game::read_file_metadata;
use crate::models::{AppConfig, InstalledMod};
use regex::Regex;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn scan_mods(game: &Path, config: &AppConfig) -> Vec<InstalledMod> {
    let plugins = game.join("BepInEx/plugins");
    if !plugins.is_dir() {
        return Vec::new();
    }
    let mut installed = WalkDir::new(&plugins)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.eq_ignore_ascii_case("dll"))
                    .unwrap_or(false)
        })
        .map(|entry| inspect_dll(entry.path(), game, config))
        .collect::<Vec<_>>();
    mark_duplicate_guids(&mut installed);
    installed
}

fn inspect_dll(path: &Path, game: &Path, config: &AppConfig) -> InstalledMod {
    let bytes = fs::read(path).unwrap_or_default();
    let plugin = parse_bepin_plugin_metadata(&bytes);
    let metadata = read_file_metadata(path);
    let guid = plugin.as_ref().and_then(|item| item.guid.clone());
    let plugin_version = plugin.as_ref().and_then(|item| item.version.clone());
    let candidates = config.repositories.iter().filter(|repo| {
        repo.enabled
            && !repo.plugin_guid.is_empty()
            && guid.as_deref().is_some_and(|value| value.eq_ignore_ascii_case(&repo.plugin_guid))
    });
    let local_component_count = installed_version_component_count(plugin_version.as_deref());
    let candidates = candidates.collect::<Vec<_>>();
    let repository = match local_component_count {
        Some(4) => candidates.iter()
            .find(|repo| repo.channel == crate::models::Channel::Beta)
            .or_else(|| candidates.iter().find(|repo| repo.channel == crate::models::Channel::All)),
        Some(_) => candidates.iter()
            .find(|repo| repo.channel == crate::models::Channel::Stable)
            .or_else(|| candidates.iter().find(|repo| repo.channel == crate::models::Channel::All)),
        None => candidates.first(),
    }.copied();
    let name = metadata
        .product_name
        .clone()
        .or_else(|| plugin.as_ref().and_then(|item| item.name.clone()))
        .or_else(|| repository.map(|repo| repo.name.clone()))
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("未知 MOD")
                .to_string()
        });
    let installed_version = plugin_version.clone();
    let version_source = if installed_version.is_some() { "BepInPlugin" } else { "unknown" };
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
    let product_name = metadata.product_name;
    let plugin_name = plugin.and_then(|item| item.name);
    let dll_file = path
        .strip_prefix(game)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
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
        name,
        product_name,
        guid,
        plugin_name,
        dll_file,
        version: installed_version,
        version_source: version_source.into(),
        update_available,
        duplicate_guid: false,
        load_warning: None,
        note,
        status: if update_available {
            "update_available".into()
        } else {
            "installed".into()
        },
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
    let guid_pattern = Regex::new(
        r"(?i)^(?:[a-z0-9_-]+\.)+[a-z0-9_-]+$|^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
    )
    .ok()?;
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
        if guid_pattern.is_match(guid) && !name.is_empty() && version_pattern.is_match(version) {
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
        let Some(guid) = mods[index].guid.as_deref() else {
            continue;
        };
        let peers = mods
            .iter()
            .enumerate()
            .filter(|(_, item)| item.guid.as_deref() == Some(guid))
            .map(|(peer_index, _)| peer_index)
            .collect::<Vec<_>>();
        if peers.len() < 2 {
            continue;
        }
        mods[index].duplicate_guid = true;
        let current = mods[index]
            .version
            .as_deref()
            .and_then(crate::releases::parse_version);
        let higher = peers
            .iter()
            .filter_map(|peer| {
                mods[*peer]
                    .version
                    .as_deref()
                    .and_then(crate::releases::parse_version)
            })
            .max();
        if let (Some(current), Some(higher)) = (current, higher) {
            if higher > current {
                mods[index].load_warning = Some("存在更高版本，框架默认不加载".into());
                mods[index].status = "duplicate_lower_version".into();
            }
        }
    }
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
                name: "Old".into(),
                product_name: Some("Old".into()),
                guid: Some("com.example.mod".into()),
                plugin_name: None,
                dll_file: "BepInEx/plugins/old.dll".into(),
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
                name: "New".into(),
                product_name: Some("New".into()),
                guid: Some("com.example.mod".into()),
                plugin_name: None,
                dll_file: "BepInEx/plugins/new.dll".into(),
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
