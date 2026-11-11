//! GUID/version selection shared by plugin-owned configuration and map directories.
use crate::{game, models::InstalledMod};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

pub(crate) const DIYLEVEL_GUID: &str = "dev.gua.overcooked.diylevel";
pub(crate) const DIYCHEF_GUID: &str = "dev.gua.overcooked.diychef";

pub(crate) fn candidates<'a>(mods: &'a [InstalledMod], guid: &str) -> Vec<&'a InstalledMod> {
    let mut values = mods
        .iter()
        .filter(|item| {
            item.guid
                .as_deref()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case(guid))
        })
        .collect::<Vec<_>>();
    values.sort_by(|a, b| {
        super::compare_mod_versions(
            b.version.as_deref().unwrap_or(""),
            a.version.as_deref().unwrap_or(""),
        )
        .unwrap_or(Ordering::Equal)
        .then(a.dll_file.cmp(&b.dll_file))
    });
    values
}

/// An unknown version cannot be ordered safely. Equal highest versions remain
/// explicit choices, including disabled DLLs; known lower versions are excluded.
pub(crate) fn highest_indices(versions: &[Option<&str>]) -> Vec<usize> {
    if versions.iter().any(|version| {
        version.is_none_or(|value| super::compare_mod_versions(value, value).is_none())
    }) {
        return (0..versions.len()).collect();
    }
    let top = versions
        .iter()
        .flatten()
        .max_by(|a, b| super::compare_mod_versions(a, b).unwrap_or(Ordering::Equal));
    versions
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            (super::compare_mod_versions(value.unwrap_or(""), top.copied().unwrap_or(""))
                == Some(Ordering::Equal))
            .then_some(index)
        })
        .collect()
}

pub(crate) fn eligible<'a>(mods: &'a [InstalledMod], guid: &str) -> Vec<&'a InstalledMod> {
    let values = candidates(mods, guid);
    highest_indices(
        &values
            .iter()
            .map(|item| item.version.as_deref())
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|index| values[index])
    .collect()
}

pub(crate) fn unique<'a>(mods: &'a [InstalledMod], guid: &str) -> Option<&'a InstalledMod> {
    let values = eligible(mods, guid);
    (values.len() == 1).then(|| values[0])
}

pub(crate) fn directory(
    game_root: &Path,
    dll_file: &str,
    enabled: bool,
) -> Result<PathBuf, String> {
    let relative = if enabled {
        dll_file.to_string()
    } else {
        super::disabled_dll_file(dll_file)
    };
    let actual = game::safe_game_path(game_root, &relative)?;
    let plugins = game::safe_game_path(game_root, super::file_state::PLUGINS_DIR)?;
    if !actual.is_file() || !actual.starts_with(plugins) {
        return Err("mods.fileMissing".into());
    }
    actual
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "mods.fileMissing".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dll_versions_use_numeric_fourth_segments_and_preserve_ambiguity() {
        assert_eq!(
            highest_indices(&[Some("1.2.3"), Some("1.2.3.4"), Some("1.2.3.3")]),
            [1]
        );
        assert_eq!(
            highest_indices(&[Some("2.1.0"), Some("2.2.0"), Some("2.2.0")]),
            [1, 2]
        );
        assert_eq!(
            highest_indices(&[Some("2.2.0"), None, Some("2.1.0")]),
            [0, 1, 2]
        );
        assert_eq!(highest_indices(&[Some("broken"), Some("2.2.0")]), [0, 1]);
        assert!(highest_indices(&[]).is_empty());
    }

    #[test]
    fn guid_selection_matches_case_and_spacing_and_only_excludes_known_lower_versions() {
        let installed = [
            crate::mods::test_support::plain_mod(
                "BepInEx/plugins/low/A.dll",
                serde_json::json!({
                    "guid": DIYLEVEL_GUID, "version": "2.2.0", "enabled": true
                }),
            ),
            crate::mods::test_support::plain_mod(
                "BepInEx/plugins/latest/A.dll",
                serde_json::json!({
                    "guid": format!(" {} ", DIYLEVEL_GUID.to_uppercase()), "version": "2.2.0.1", "enabled": false
                }),
            ),
            crate::mods::test_support::plain_mod(
                "BepInEx/plugins/chef/A.dll",
                serde_json::json!({
                    "guid": DIYCHEF_GUID, "version": "99.0", "enabled": true
                }),
            ),
        ];
        let latest = unique(&installed, DIYLEVEL_GUID).unwrap();
        assert_eq!(latest.dll_file, "BepInEx/plugins/latest/A.dll");
        assert!(!latest.enabled);
        let mut unknown = installed.to_vec();
        unknown[0].version = None;
        assert!(unique(&unknown, DIYLEVEL_GUID).is_none());
        assert_eq!(eligible(&unknown, DIYLEVEL_GUID).len(), 2);
    }

    #[test]
    fn directory_checks_the_exact_enabled_state_and_rejects_outside_plugins() {
        let root = std::env::temp_dir().join(format!("oc2-plugin-target-{}", uuid::Uuid::new_v4()));
        let package = root.join("BepInEx/plugins/Renamed");
        std::fs::create_dir_all(&package).unwrap();
        let relative = "BepInEx/plugins/Renamed/Plugin.dll";
        std::fs::write(package.join("Plugin.dll.disabled"), b"fixture").unwrap();
        assert_eq!(
            directory(&root, relative, false).unwrap(),
            package.canonicalize().unwrap()
        );
        assert_eq!(
            directory(&root, relative, true).unwrap_err(),
            "mods.fileMissing"
        );
        std::fs::create_dir_all(root.join("BepInEx/cache")).unwrap();
        std::fs::write(root.join("BepInEx/cache/Plugin.dll"), b"fixture").unwrap();
        assert_eq!(
            directory(&root, "BepInEx/cache/Plugin.dll", true).unwrap_err(),
            "mods.fileMissing"
        );
        assert!(directory(&root, "../outside.dll", true).is_err());
        let canonical = root.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(canonical.starts_with(&temp) && canonical != temp);
        std::fs::remove_dir_all(canonical).unwrap();
    }
}
