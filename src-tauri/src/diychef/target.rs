use crate::{
    game,
    models::{ChefTarget, InstalledMod},
    mods,
};
use std::path::{Path, PathBuf};
pub(crate) const GUID: &str = mods::plugin_target::DIYCHEF_GUID;
fn supports_custom_default(version: Option<&str>) -> bool {
    // This is a DLL version, so use the same numeric ordering as target selection.
    version
        .and_then(|value| mods::compare_mod_versions(value, "2.2"))
        .is_some_and(|order| order != std::cmp::Ordering::Less)
}
pub(crate) fn candidates(mods: &[InstalledMod], generation: u64) -> Vec<ChefTarget> {
    mods::plugin_target::candidates(mods, GUID)
        .into_iter()
        .map(|m| ChefTarget {
            id: if m.enabled {
                m.dll_file.clone()
            } else {
                format!("{}.disabled", m.dll_file)
            },
            guid: GUID.into(),
            version: m.version.clone(),
            display_path: m.dll_file.clone(),
            enabled: m.enabled,
            generation,
            compatibility: "editable".into(),
            supports_custom_default: supports_custom_default(m.version.as_deref()),
        })
        .collect()
}
/// The picker and explicit selection share this eligibility rule. A known older
/// version is never offered while the highest version is installed more than once.
pub(crate) fn eligible(values: &[ChefTarget]) -> Vec<ChefTarget> {
    mods::plugin_target::highest_indices(
        &values
            .iter()
            .map(|item| item.version.as_deref())
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|index| values[index].clone())
    .collect()
}
pub(crate) fn select(
    values: &[ChefTarget],
    requested: Option<&str>,
) -> Result<Option<ChefTarget>, String> {
    if values.is_empty() {
        return Ok(None);
    }
    let options = eligible(values);
    if let Some(id) = requested {
        return options
            .into_iter()
            .find(|x| x.id == id)
            .map(Some)
            .ok_or_else(|| "diychef.targetChanged".into());
    }
    Ok(if options.len() == 1 {
        options.into_iter().next()
    } else {
        None
    })
}
pub(crate) fn mod_root(game_root: &Path, target: &ChefTarget) -> Result<PathBuf, String> {
    mods::plugin_target::directory(game_root, &target.display_path, target.enabled)
        .map_err(|_| "diychef.targetChanged".into())
}
pub(crate) fn child(game_root: &Path, parent: &Path, name: &str) -> Result<PathBuf, String> {
    let root = game::resolve_existing_ancestor(game_root)?;
    // Canonicalize both sides before strip_prefix: Windows adds the extended
    // path prefix to canonical paths, including ordinary temporary directories.
    let parent = game::resolve_existing_ancestor(parent)?;
    let relative = parent
        .join(name)
        .strip_prefix(&root)
        .map_err(|_| "archive.pathEscape")?
        .to_string_lossy()
        .replace('\\', "/");
    let candidate = game::safe_game_path(&root, &relative)?;
    if !candidate.starts_with(&parent) {
        return Err("archive.pathEscape".into());
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(id: &str, v: Option<&str>, enabled: bool) -> ChefTarget {
        ChefTarget {
            id: id.into(),
            guid: GUID.into(),
            version: v.map(str::to_string),
            display_path: id.into(),
            enabled,
            generation: 1,
            compatibility: "read_only".into(),
            supports_custom_default: supports_custom_default(v),
        }
    }
    #[test]
    fn custom_defaults_require_at_least_diychef_2_2() {
        for version in [
            Some("2.2"),
            Some("2.2.0"),
            Some("v2.2.0"),
            Some("2.2.0.1"),
            Some("2.3.0"),
            Some("3.0"),
            Some("2.2.0-beta.1"),
        ] {
            assert!(supports_custom_default(version), "{version:?}");
        }
        for version in [
            None,
            Some(""),
            Some("unknown"),
            Some("2.1.9"),
            Some("2.1.9-beta.1"),
        ] {
            assert!(!supports_custom_default(version), "{version:?}");
        }
    }
    #[test]
    fn older_unknown_and_non_fastinit_targets_are_editable() {
        for version in [None, Some("1.0"), Some("2.1"), Some("2.1.9"), Some("2.2")] {
            let installed: InstalledMod = serde_json::from_value(serde_json::json!({
                "name": "DIYChef", "guid": GUID, "version": version,
                "dll_file": "BepInEx/plugins/DIYChef/DIYChef.dll", "enabled": true,
                "version_source": "plugin", "update_available": false,
                "duplicate_guid": false, "status": "unknown"
            }))
            .unwrap();
            let values = candidates(&[installed], 7);
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].compatibility, "editable");
            assert_eq!(values[0].supports_custom_default, version == Some("2.2"));
        }
    }
    #[test]
    fn disabled_highest_is_selected() {
        let values = vec![
            t("disabled", Some("2.2.0"), false),
            t("enabled", Some("2.1.0"), true),
        ];
        assert_eq!(select(&values, None).unwrap().unwrap().id, "disabled");
        assert!(select(&values, Some("enabled")).is_err());
    }
    #[test]
    fn equal_or_unknown_versions_require_explicit_selection() {
        let values = vec![t("a", Some("2.2.0"), false), t("b", Some("2.2.0"), true)];
        assert!(select(&values, None).unwrap().is_none());
        assert_eq!(select(&values, Some("b")).unwrap().unwrap().id, "b");
        let unknown = vec![t("a", None, true), t("b", Some("2.2.0"), true)];
        assert!(select(&unknown, None).unwrap().is_none());
    }
    #[test]
    fn duplicate_latest_versions_exclude_older_targets_from_picker_and_selection() {
        let values = vec![
            t("latest-a", Some("2.2.0"), false),
            t("latest-b", Some("2.2.0"), true),
            t("older", Some("2.1.0"), true),
        ];
        let options = eligible(&values);
        assert_eq!(
            options
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["latest-a", "latest-b"]
        );
        assert!(select(&values, None).unwrap().is_none());
        assert!(select(&values, Some("older")).is_err());
        for candidate in options {
            assert_eq!(
                select(&values, Some(&candidate.id)).unwrap().unwrap().id,
                candidate.id
            );
        }
    }
    #[test]
    fn unknown_version_keeps_every_target_eligible_for_explicit_selection() {
        let values = vec![
            t("unknown", None, true),
            t("latest", Some("2.2.0"), true),
            t("older", Some("2.1.0"), true),
        ];
        assert_eq!(eligible(&values).len(), 3);
        assert!(select(&values, None).unwrap().is_none());
        assert_eq!(select(&values, Some("older")).unwrap().unwrap().id, "older");
    }
    #[test]
    fn child_accepts_plain_windows_paths_but_rejects_parent_escape() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        let nested = p.join("mod");
        std::fs::create_dir(&nested).unwrap();
        assert_eq!(
            child(&p, &nested, "prefer.txt").unwrap(),
            nested.canonicalize().unwrap().join("prefer.txt")
        );
        assert!(child(&p, &nested, "../prefer.txt").is_err());
        assert!(child(&p, &nested, "C:/outside").is_err());
        std::fs::remove_dir(nested).unwrap();
        std::fs::remove_dir(p).unwrap();
    }
}
