use super::errors::INVALID_RECORD;
use super::errors::io_error;
use super::file_io::read_bounded;
use super::file_io::write_atomic;
use super::paths::checked_path;
use super::paths::file_key;
use super::paths::manifest_path;
use super::paths::normalize_relative;
use super::paths::validate_absolute_path;
use super::records::RecycleRecord;
use crate::archive::InstallManifest;
use std::fs;
use std::path::Path;

pub(super) fn read_active_manifest(
    directory: &Path,
    id: &str,
) -> Result<Option<(InstallManifest, Vec<u8>)>, String> {
    let path = manifest_path(directory, id)?;
    if !path.try_exists().map_err(io_error)? {
        return Ok(None);
    }
    let bytes = read_bounded(&path)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| INVALID_RECORD.to_string())?;
    let manifest: InstallManifest = toml::from_str(text).map_err(|_| INVALID_RECORD.to_string())?;
    if manifest.repository_id != id {
        return Err(INVALID_RECORD.into());
    }
    for relative in &manifest.files {
        normalize_relative(relative)?;
    }
    Ok(Some((manifest, bytes)))
}

pub(super) fn all_active_manifests(directory: &Path) -> Result<Vec<InstallManifest>, String> {
    validate_absolute_path(directory)?;
    if !directory.try_exists().map_err(io_error)? {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            let id = entry
                .path()
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or(INVALID_RECORD)?
                .to_owned();
            if let Some((manifest, _)) = read_active_manifest(directory, &id)? {
                manifests.push(manifest);
            }
        }
    }
    Ok(manifests)
}

pub(super) fn set_active_manifest(
    directory: &Path,
    id: &str,
    manifest: Option<&InstallManifest>,
) -> Result<(), String> {
    let path = manifest_path(directory, id)?;
    if let Some(manifest) = manifest {
        let text = toml::to_string_pretty(manifest).map_err(io_error)?;
        fs::create_dir_all(directory).map_err(io_error)?;
        write_atomic(&path, text.as_bytes())
    } else if path.try_exists().map_err(io_error)? {
        fs::remove_file(path).map_err(io_error)
    } else {
        Ok(())
    }
}

pub(super) fn original_manifest(
    directory: &Path,
    record: &RecycleRecord,
) -> Result<Option<InstallManifest>, String> {
    let Some(id) = record.package_id.as_deref() else {
        if !record.removed_manifest_files.is_empty() || record.release_tag.is_some() {
            return Err(INVALID_RECORD.into());
        }
        /*
         * 非包记录没有要合并的安装清单：散装 DLL 与关卡集整目录都走这条路径
         * （`level_sets::remove_level_set` 把目录展开成文件列表再 `stage`），
         * 所以不能再要求「必须是 BepInEx/plugins 下的 .dll」—— 关卡集的 `info`
         * 与场景文件会被这条判定当成坏记录，恢复永远报 invalidRecord。
         * 清单快照只可能和 `package_id` 一起出现，多出 `manifest.toml` 才是坏记录。
         */
        if checked_path(directory, "manifest.toml")?
            .try_exists()
            .map_err(io_error)?
        {
            return Err(INVALID_RECORD.into());
        }
        return Ok(None);
    };
    let bytes = read_bounded(&checked_path(directory, "manifest.toml")?)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| INVALID_RECORD.to_string())?;
    let manifest: InstallManifest = toml::from_str(text).map_err(|_| INVALID_RECORD.to_string())?;
    if manifest.repository_id != id
        || record.release_tag.as_deref() != Some(&manifest.release_tag)
        || record.removed_manifest_files.iter().any(|removed| {
            !manifest
                .files
                .iter()
                .any(|file| file_key(file) == file_key(removed))
        })
    {
        return Err(INVALID_RECORD.into());
    }
    for relative in &manifest.files {
        normalize_relative(relative)?;
    }
    if record.files.iter().any(|file| {
        !record
            .removed_manifest_files
            .iter()
            .any(|removed| file_key(removed) == file_key(&file.relative))
    }) {
        return Err(INVALID_RECORD.into());
    }
    Ok(Some(manifest))
}

pub(super) fn merge_restored_manifest(
    original: &InstallManifest,
    active: Option<&InstallManifest>,
    removed: &[String],
) -> Result<InstallManifest, String> {
    let mut merged = if let Some(active) = active {
        if active.release_tag != original.release_tag
            || active.repository_binding != original.repository_binding
            || active.installed_at != original.installed_at
        {
            return Err("recycle.packageUpdated".into());
        }
        active.clone()
    } else {
        let mut merged = original.clone();
        merged.files.clear();
        merged
    };
    for relative in removed {
        if !merged
            .files
            .iter()
            .any(|file| file_key(file) == file_key(relative))
        {
            let original_position = original
                .files
                .iter()
                .position(|file| file_key(file) == file_key(relative))
                .ok_or(INVALID_RECORD)?;
            let insertion = original.files[original_position + 1..]
                .iter()
                .find_map(|next| {
                    merged
                        .files
                        .iter()
                        .position(|file| file_key(file) == file_key(next))
                })
                .unwrap_or(merged.files.len());
            merged.files.insert(insertion, relative.clone());
        }
    }
    Ok(merged)
}
