//! Directory callers share the same file journal and manifest transaction.
use super::{
    errors::io_error, manifests::all_active_manifests, manifests::read_active_manifest,
    paths::checked_path, paths::file_key, paths::is_link, records::RelatedManifest,
    staging::stage_with_related_manifests,
};
use crate::models::ModRecycleEntry;
use std::{fs, path::Path};

pub(crate) fn directory_files(game: &Path, relative: &str) -> Result<Vec<String>, String> {
    let directory = checked_path(game, relative)?;
    if !directory.is_dir() {
        return Err("mods.fileMissing".into());
    }
    let mut files = vec![];
    for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if is_link(&metadata) || (!metadata.is_file() && !metadata.is_dir()) {
            return Err("recycle.unsafePath".into());
        }
        if metadata.is_file() {
            let name = entry
                .path()
                .strip_prefix(game)
                .map_err(|_| "archive.pathEscape")?
                .to_string_lossy()
                .replace('\\', "/");
            checked_path(game, &name)?;
            files.push(name);
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn recycle_directories(
    root: &Path,
    manifests: &Path,
    game: &Path,
    directories: &[String],
    name: String,
    diychef_resources_imported: bool,
) -> Result<ModRecycleEntry, String> {
    let files = directories
        .iter()
        .map(|directory| directory_files(game, directory))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Err(super::errors::NO_FILES.into());
    }
    let actual = files
        .iter()
        .map(|file| file_key(file))
        .collect::<std::collections::HashSet<_>>();
    let mut related = vec![];
    for manifest in all_active_manifests(manifests)? {
        let removed = manifest
            .files
            .iter()
            .filter(|file| {
                actual.contains(&file_key(file))
                    && directories.iter().any(|directory| {
                        file.replace('\\', "/")
                            .to_lowercase()
                            .starts_with(&format!("{}/", directory.to_lowercase()))
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        if !removed.is_empty() {
            let (_, bytes) = read_active_manifest(manifests, &manifest.repository_id)?
                .ok_or("recycle.invalidRecord")?;
            related.push(RelatedManifest {
                package_id: manifest.repository_id,
                original: String::from_utf8(bytes).map_err(|_| "recycle.invalidRecord")?,
                removed_files: removed,
            });
        }
    }
    let result = stage_with_related_manifests(
        root,
        manifests,
        game,
        name,
        None,
        None,
        vec![],
        files,
        None,
        related,
        diychef_resources_imported,
        &mut || Ok(()),
    )?;
    for directory in directories {
        let Ok(directory) = checked_path(game, directory) else {
            continue;
        };
        for entry in walkdir::WalkDir::new(directory)
            .contents_first(true)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_dir() {
                let _ = fs::remove_dir(entry.path());
            }
        }
    }
    Ok(result)
}
