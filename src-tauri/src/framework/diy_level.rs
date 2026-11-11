//! DIYLevel backups preserve the selected plugin directory instead of guessing
//! the latest installed version's destination when restoring an older snapshot.
use super::{DIYLEVEL_BACKUP_PREFIX, DIYLEVEL_DIR, FrameworkBackupSpec};
use crate::{archive, game};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fs, path::Path};

const LAYOUT_FILE: &str = "framework-layout.toml";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiyLevelLayout {
    version: u32,
    directory: String,
}

/// Whole-directory restore cannot own the shared plugins root or anything above it.
fn checked_directory(game_root: &Path, package_root: &Path) -> Result<String, String> {
    let game_root = game::resolve_existing_ancestor(game_root)?;
    let package_root = game::resolve_existing_ancestor(package_root)?;
    let plugins = game::safe_game_path(&game_root, "BepInEx/plugins")?;
    if package_root == plugins || !package_root.starts_with(&plugins) {
        return Err("archive.pathEscape".into());
    }
    let relative = package_root
        .strip_prefix(&game_root)
        .map_err(|_| "archive.pathEscape")?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    if game::safe_game_path(&game_root, &relative)? != package_root {
        return Err("archive.pathEscape".into());
    }
    Ok(relative)
}

pub(crate) fn diy_level_backup_spec(
    game_root: &Path,
    package_root: &Path,
) -> Result<FrameworkBackupSpec<'static>, String> {
    Ok(FrameworkBackupSpec {
        prefix: DIYLEVEL_BACKUP_PREFIX,
        directories: Cow::Owned(vec![Cow::Owned(checked_directory(
            game_root,
            package_root,
        )?)]),
        files: &[],
    })
}

pub(super) fn write_layout(backup: &Path, spec: &FrameworkBackupSpec<'_>) -> Result<(), String> {
    if spec.prefix != DIYLEVEL_BACKUP_PREFIX {
        return Ok(());
    }
    let directory = spec
        .directories
        .first()
        .ok_or("backup.wrongKind")?
        .to_string();
    let bytes = toml::to_string(&DiyLevelLayout {
        version: 1,
        directory,
    })
    .map_err(|error| error.to_string())?;
    fs::write(backup.join(LAYOUT_FILE), bytes).map_err(|error| error.to_string())
}

pub(crate) fn diy_level_restore_spec(
    game_root: &Path,
    backup: &Path,
    current_package_root: Option<&Path>,
) -> Result<FrameworkBackupSpec<'static>, String> {
    let layout_path = backup.join(LAYOUT_FILE);
    let directory = match fs::read_to_string(&layout_path) {
        Ok(text) => {
            let layout: DiyLevelLayout = toml::from_str(&text).map_err(|_| "backup.wrongKind")?;
            if layout.version != 1 {
                return Err("backup.wrongKind".into());
            }
            layout.directory
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DIYLEVEL_DIR.to_string(),
        Err(error) => return Err(error.to_string()),
    };
    let destination = game::safe_game_path(game_root, &directory)?;
    let source = game::safe_game_path(backup, &directory)?;
    if !source.is_dir() {
        return Err("backup.notFound".into());
    }
    let spec = diy_level_backup_spec(game_root, &destination)?;
    if let Some(current) = current_package_root {
        if game::resolve_existing_ancestor(current)? != destination {
            return Err("backup.diyLevelTargetMismatch".into());
        }
    } else {
        // A former plugin directory may now belong to another MOD. Without a
        // selected DIYLevel target, only an absent or proven empty path is ours.
        match fs::read_dir(&destination) {
            Ok(mut entries) => {
                if entries
                    .next()
                    .transpose()
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    return Err("backup.diyLevelTargetMismatch".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(spec)
}

/// A package manifest may contain sibling plugins. A directory backup must not
/// later restore a manifest that gives uninstall ownership over those siblings.
pub(super) fn scope_manifest(
    spec: &FrameworkBackupSpec<'_>,
    manifest: &mut archive::InstallManifest,
) -> Result<(), String> {
    if spec.prefix != DIYLEVEL_BACKUP_PREFIX {
        return Ok(());
    }
    let id = Path::new(&manifest.repository_id);
    if manifest.repository_id.contains(['\\', ':', '\0'])
        || id.components().count() != 1
        || !matches!(
            id.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err("backup.wrongKind".into());
    }
    let directory = spec.directories.first().ok_or("backup.wrongKind")?;
    manifest.files.retain(|file| {
        let normalized = file.replace('\\', "/");
        let path = Path::new(&normalized);
        !normalized.contains(':')
            && !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
            && owned_path(&normalized, directory)
    });
    Ok(())
}

fn owned_path(relative: &str, directory: &str) -> bool {
    let relative = relative.replace('\\', "/");
    let directory = directory.replace('\\', "/");
    #[cfg(windows)]
    let (relative, directory) = (relative.to_lowercase(), directory.to_lowercase());
    Path::new(&relative).starts_with(&directory)
}

pub(super) fn verify_restore_ownership(
    spec: &FrameworkBackupSpec<'_>,
    manifests_dir: &Path,
    manifest_id: &str,
) -> Result<(), String> {
    if spec.prefix != DIYLEVEL_BACKUP_PREFIX {
        return Ok(());
    }
    if let Some(manifest) = archive::read_manifest(manifests_dir, manifest_id) {
        let mut scoped = manifest.clone();
        scope_manifest(spec, &mut scoped)?;
        if scoped.files != manifest.files {
            return Err("backup.diyLevelTargetMismatch".into());
        }
    }
    let directory = spec.directories.first().ok_or("backup.wrongKind")?;
    // Replacing the whole directory must not overwrite files claimed by another
    // package, even when its manifest uses different path separators or casing.
    for manifest in archive::read_manifests(manifests_dir) {
        if manifest.repository_id != manifest_id
            && manifest
                .files
                .iter()
                .any(|file| owned_path(file, directory))
        {
            return Err("backup.diyLevelTargetMismatch".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
