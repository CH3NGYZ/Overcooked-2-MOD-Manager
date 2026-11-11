use crate::{
    game,
    models::{ChefTarget, InstalledMod},
    mods,
};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};
pub(crate) const GUID: &str = "dev.gua.overcooked.diychef";
pub(crate) fn candidates(mods: &[InstalledMod], generation: u64) -> Vec<ChefTarget> {
    let mut values: Vec<_> = mods
        .iter()
        .filter(|m| {
            m.guid
                .as_deref()
                .is_some_and(|g| g.eq_ignore_ascii_case(GUID))
        })
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
            compatibility:
                if m.version.as_deref() == Some("2.2.0") && m.name.contains("FastInit") {
                    "fastinit_2_2"
                } else {
                    "read_only"
                }
                .into(),
        })
        .collect();
    values.sort_by(|a, b| {
        mods::compare_mod_versions(
            b.version.as_deref().unwrap_or(""),
            a.version.as_deref().unwrap_or(""),
        )
        .unwrap_or(Ordering::Equal)
        .then(a.id.cmp(&b.id))
    });
    values
}
/// The picker and explicit selection share this eligibility rule. A known older
/// version is never offered while the highest version is installed more than once.
pub(crate) fn eligible(values: &[ChefTarget]) -> Vec<ChefTarget> {
    let Some(top) = values.first() else {
        return vec![];
    };
    let unknown = values.iter().any(|x| {
        x.version
            .as_deref()
            .is_none_or(|v| mods::compare_mod_versions(v, v).is_none())
    });
    values
        .iter()
        .filter(|x| {
            unknown
                || mods::compare_mod_versions(
                    x.version.as_deref().unwrap_or(""),
                    top.version.as_deref().unwrap_or(""),
                ) == Some(Ordering::Equal)
        })
        .cloned()
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
    let enabled = game::safe_game_path(game_root, &target.id)?;
    let actual = if enabled.is_file() {
        enabled
    } else {
        game::safe_game_path(game_root, &format!("{}.disabled", target.id))?
    };
    if !actual.is_file() {
        return Err("diychef.targetChanged".into());
    }
    actual
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "diychef.targetChanged".into())
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
