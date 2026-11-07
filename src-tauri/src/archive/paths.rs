use std::fs;
use std::path::Path;

pub fn prune_empty_parent_dirs(game_root: &Path, path: &Path) -> Result<(), String> {
    log::trace!("清理空 MOD 目录：起点={}", path.display());
    let root = fs::canonicalize(game_root).unwrap_or_else(|_| game_root.to_path_buf());
    let mut current = Some(path.to_path_buf());
    while let Some(directory) = current {
        let canonical = fs::canonicalize(&directory).unwrap_or(directory.clone());
        if canonical == root || !canonical.starts_with(&root) {
            break;
        }
        let name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if name.eq_ignore_ascii_case("BepInEx") || name.eq_ignore_ascii_case("plugins") {
            break;
        }
        let mut entries = fs::read_dir(&canonical).map_err(|e| e.to_string())?;
        if entries.next().is_some() {
            break;
        }
        fs::remove_dir(&canonical).map_err(|e| e.to_string())?;
        current = canonical.parent().map(Path::to_path_buf);
    }
    Ok(())
}

pub fn clear_readonly(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let mut changed = 0;
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
    {
        let metadata = fs::metadata(entry.path()).map_err(|e| e.to_string())?;
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            fs::set_permissions(entry.path(), permissions).map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    if changed > 0 {
        log::debug!("清除只读属性：路径={}，条目={}", path.display(), changed);
    }
    Ok(())
}

pub(super) fn path_relative(root: &Path, path: &Path) -> String {
    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&canonical_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
