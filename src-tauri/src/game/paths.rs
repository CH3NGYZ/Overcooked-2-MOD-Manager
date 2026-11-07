use std::fs;
use std::path::{Path, PathBuf};

pub fn safe_game_path(game: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || relative.contains(':')
        || rel
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        log::warn!("拒绝不安全游戏相对路径：{}", relative);
        return Err("game.relativePathRequired".into());
    }
    let root = resolve_existing_ancestor(game)?;
    let candidate = resolve_existing_ancestor(&root.join(rel))?;
    if !candidate.starts_with(&root) {
        log::warn!("拒绝逃逸游戏目录的路径：{}", relative);
        return Err("archive.pathEscape".into());
    }
    log::trace!("解析游戏相对路径：{}", relative);
    Ok(candidate)
}

pub(crate) fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    // A new file cannot be canonicalized yet. Resolve its nearest existing
    // ancestor so a junction cannot hide an escape behind a missing suffix.
    for ancestor in absolute.ancestors() {
        match fs::canonicalize(ancestor) {
            Ok(resolved) => {
                let suffix = absolute.strip_prefix(ancestor).map_err(|e| e.to_string())?;
                return Ok(if suffix.as_os_str().is_empty() {
                    resolved
                } else {
                    resolved.join(suffix)
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("archive.pathEscape".into())
}

pub(super) fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests;
