use super::errors::INVALID_RECORD;
use super::errors::UNSAFE_PATH;
use super::errors::io_error;
use crate::mods;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

pub(super) fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions and other reparse points, not only symbolic links.
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub(super) fn validate_absolute_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(UNSAFE_PATH.into());
    }
    let mut checked = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err(UNSAFE_PATH.into());
        }
        checked.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            // Windows verbatim drive prefixes (\\?\C:) only become a valid
            // filesystem path after the following RootDir separator.
            continue;
        }
        match fs::symlink_metadata(&checked) {
            Ok(metadata) if is_link(&metadata) => return Err(UNSAFE_PATH.into()),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

pub(super) fn validate_relative(relative: &str) -> Result<(), String> {
    if relative.is_empty()
        || relative.contains(['\\', ':', '\0'])
        || relative.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.chars().any(char::is_control)
        })
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(UNSAFE_PATH.into());
    }
    Ok(())
}

pub(super) fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative(relative)?;
    let path = root.join(relative);
    validate_absolute_path(&path)?;
    Ok(path)
}

pub(super) fn normalize_relative(relative: &str) -> Result<String, String> {
    let normalized = relative.replace('\\', "/");
    validate_relative(&normalized)?;
    Ok(normalized)
}

pub(super) fn valid_package_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id != "."
        && id != ".."
        && !id.ends_with(['.', ' '])
        && id
            .chars()
            .all(|value| value.is_alphanumeric() || "-_ .".contains(value))
}

pub(super) fn manifest_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_package_id(id) {
        return Err(INVALID_RECORD.into());
    }
    checked_path(directory, &format!("{id}.toml"))
}

pub(super) fn record_directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    if Uuid::parse_str(id)
        .ok()
        .is_none_or(|parsed| parsed.to_string() != id)
    {
        return Err(INVALID_RECORD.into());
    }
    checked_path(root, id)
}

pub(super) fn file_key(relative: &str) -> String {
    mods::canonical_dll_name(&relative.replace('\\', "/")).to_lowercase()
}

pub(super) fn existing_variants(game_root: &Path, relative: &str) -> Result<Vec<String>, String> {
    let mut candidates = vec![relative.to_owned()];
    if Path::new(relative)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
    {
        candidates.push(format!("{relative}.disabled"));
    }
    let mut existing = Vec::new();
    for candidate in candidates {
        let path = checked_path(game_root, &candidate)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_link(&metadata) => existing.push(candidate),
            Ok(_) => return Err(UNSAFE_PATH.into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(existing)
}
