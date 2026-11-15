//! Keep pre-2.0 DIYChef models in the existing removal journal, including user files.
use super::{
    directories::directory_files,
    paths::{checked_path, file_key},
    records::RecycleRecord,
};
use crate::{archive::InstallManifest, models::DllRole, mods};
use std::{cmp::Ordering, collections::BTreeSet, path::Path};

fn is_legacy_chef(details: &mods::DllDetails) -> bool {
    details.guid.as_deref().is_some_and(|guid| {
        guid.trim()
            .eq_ignore_ascii_case(mods::plugin_target::DIYCHEF_GUID)
    }) && details
        .version
        .as_deref()
        .and_then(|version| mods::compare_mod_versions(version, "2.0"))
        == Some(Ordering::Less)
}

fn legacy_roots(base: &Path, files: &[String]) -> Result<BTreeSet<String>, String> {
    let mut roots = BTreeSet::new();
    for file in files {
        if !mods::canonical_dll_name(file)
            .to_ascii_lowercase()
            .ends_with(".dll")
        {
            continue;
        }
        let path = checked_path(base, file)?;
        let details = mods::read_dll_details(&path, file);
        if !is_legacy_chef(&details) {
            continue;
        }
        if let Some((parent, _)) = file.rsplit_once('/') {
            roots.insert(parent.to_owned());
        }
    }
    Ok(roots)
}

fn in_resources(relative: &str, plugin_root: &str) -> bool {
    relative
        .to_lowercase()
        .starts_with(&format!("{}/resources/", plugin_root.to_lowercase()))
}

/// Verify that every active manifest which references a Resources file in this
/// plugin root also owns one of the legacy DIYChef DLLs being migrated.
///
/// Migration recycles a whole plugin directory after copying its models.  The
/// normal staging path performs this check while it discovers legacy files,
/// but migration deliberately marks the resulting record as imported and thus
/// skips that discovery.  Keep the ownership check as an explicit preflight so
/// an orphan/shared Resources tree can never be removed as a side effect of a
/// migration.
pub(crate) fn ensure_resource_root_unshared(
    manifests: &Path,
    plugin_root: &str,
    allowed_dlls: &BTreeSet<String>,
) -> Result<(), String> {
    let prefix = format!(
        "{}/resources/",
        plugin_root.trim_end_matches('/').to_lowercase()
    );
    for manifest in super::manifests::all_active_manifests(manifests)? {
        let references_resources = manifest
            .files
            .iter()
            .any(|file| file_key(file).starts_with(&prefix));
        if !references_resources {
            continue;
        }
        let owns_legacy_dll = manifest
            .files
            .iter()
            .any(|file| allowed_dlls.contains(&file_key(file)));
        if !owns_legacy_dll {
            return Err("recycle.fileSharedByOtherPackage".into());
        }
    }
    Ok(())
}

/// Called before Preparing or any move. Nothing is inferred from a DLL filename,
/// a release tag or a directory name when the actual GUID/version is unknown.
pub(super) fn legacy_resource_files(
    game: &Path,
    manifests: &Path,
    files: &[String],
    manifest: Option<&InstallManifest>,
) -> Result<Vec<String>, String> {
    let roots = legacy_roots(game, files)?;
    if roots.is_empty() {
        return Ok(vec![]);
    }
    let selected = files
        .iter()
        .map(|file| file_key(file))
        .collect::<BTreeSet<_>>();
    let active = super::manifests::all_active_manifests(manifests)?;
    let registered = crate::level_sets::registered_plugin_paths(manifests)
        .into_iter()
        .map(|file| file_key(&file))
        .collect::<BTreeSet<_>>();
    let mut models = vec![];
    for root in roots {
        let children = directory_files(game, &root)?;
        let resources = children
            .iter()
            .filter(|file| in_resources(file, &root))
            .cloned()
            .collect::<Vec<_>>();
        if resources.is_empty() {
            continue;
        }
        let parts = root.split('/').collect::<Vec<_>>();
        if parts.len() < 3
            || !parts[0].eq_ignore_ascii_case("BepInEx")
            || !parts[1].eq_ignore_ascii_case("plugins")
            || children
                .iter()
                .any(|file| registered.contains(&file_key(file)))
            || active
                .iter()
                .filter(|peer| {
                    manifest.is_none_or(|primary| peer.repository_id != primary.repository_id)
                })
                .any(|peer| peer.files.iter().any(|file| in_resources(file, &root)))
        {
            return Err("recycle.fileSharedByOtherPackage".into());
        }
        // Other plugins (including another chef that remains installed) may be
        // using this same Resources tree. Pure dependency DLLs are not plugins.
        for file in children.iter().filter(|file| {
            mods::canonical_dll_name(file)
                .to_ascii_lowercase()
                .ends_with(".dll")
        }) {
            let details = mods::read_dll_details(&checked_path(game, file)?, file);
            if matches!(details.role, DllRole::Mod)
                && (!selected.contains(&file_key(file)) || !is_legacy_chef(&details))
            {
                return Err("recycle.fileSharedByOtherPackage".into());
            }
        }
        let mut seen = BTreeSet::new();
        if resources
            .iter()
            .any(|file| !seen.insert(file.to_lowercase()))
        {
            return Err("recycle.unsafePath".into());
        }
        models.extend(resources);
    }
    Ok(models)
}

/// Automatic cleanup can retain model payloads; explicit delete/empty stays final.
pub(super) fn has_pending_legacy_resources(
    directory: &Path,
    record: &RecycleRecord,
) -> Result<bool, String> {
    if record.diychef_resources_imported {
        return Ok(false);
    }
    let payload = checked_path(directory, "files")?;
    let files = record
        .files
        .iter()
        .map(|file| file.relative.clone())
        .collect::<Vec<_>>();
    let roots = legacy_roots(&payload, &files)?;
    Ok(roots
        .iter()
        .any(|root| files.iter().any(|file| in_resources(file, root))))
}

/// Package journals may contain user-added model files which were never part
/// of the active manifest.  They are valid only when the payload also contains
/// a verified pre-2.0 DIYChef DLL whose Resources tree owns that file.
pub(super) fn is_recorded_legacy_resource(
    directory: &Path,
    record: &RecycleRecord,
    relative: &str,
) -> Result<bool, String> {
    if record.diychef_resources_imported {
        return Ok(false);
    }
    let payload = checked_path(directory, "files")?;
    let files = record
        .files
        .iter()
        .map(|file| file.relative.clone())
        .collect::<Vec<_>>();
    let roots = legacy_roots(&payload, &files)?;
    Ok(roots.iter().any(|root| in_resources(relative, root)))
}
