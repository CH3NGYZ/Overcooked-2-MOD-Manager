//! 只读预览的目录校验、缓存与重活入口。
use super::{info_cache::content_sha256, locate_level_set_directory, scan::locate_info_file};
use crate::level_sets::preview_reader::{Context, Level, Picture, Recipe};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
struct Entry {
    key: String,
    context: Context,
}
fn cache() -> &'static Mutex<Option<Entry>> {
    static C: OnceLock<Mutex<Option<Entry>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}
#[derive(serde::Serialize)]
pub(crate) struct Detail {
    pub picture: Option<Picture>,
    pub recipes: Option<Vec<Recipe>>,
}
/// External common/game resources may change without changing info. Fingerprint their
/// directory entries as well, so reopening never revives a stale external image.
fn fingerprint(root: &Path, local: &Path, info: &[u8]) -> String {
    let mut keys = vec![
        content_sha256(info),
        root.display().to_string(),
        local.display().to_string(),
        "preview-v11".into(),
    ];
    for dir in [
        local.to_path_buf(),
        root.join("BepInEx/plugins/OC2DIYLevel"),
        root.join("Overcooked2_Data/StreamingAssets/Windows"),
        root.join("Overcooked2.app/Contents/Resources/Data/StreamingAssets/OSX"),
    ] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(m) = entry.metadata() {
                    keys.push(format!(
                        "{}:{}:{:?}",
                        entry.path().display(),
                        m.len(),
                        m.modified().ok()
                    ));
                }
            }
        }
    }
    for path in super::preview_reader::sources::common_sources(root, local)
        .into_iter()
        .chain(super::preview_reader::sources::game_sources(root))
        .chain(super::preview_reader::sources::sibling_info_sources(root))
    {
        if let Ok(meta) = path.metadata() {
            keys.push(format!(
                "{}:{}:{:?}",
                path.display(),
                meta.len(),
                meta.modified().ok()
            ));
        }
    }
    keys.sort();
    content_sha256(keys.join("\n").as_bytes())
}
fn with_context<T>(
    root: &Path,
    dir: &str,
    enabled: bool,
    f: impl FnOnce(&mut Context, &str) -> Result<T, String>,
) -> Result<T, String> {
    let local = locate_level_set_directory(root, dir, enabled)?;
    let info = locate_info_file(&local).ok_or("assetBundle.missingLevelSetInfo")?;
    let canonical = info
        .canonicalize()
        .map_err(|_| "assetBundle.previewUnavailable")?;
    if !canonical.starts_with(
        local
            .canonicalize()
            .map_err(|_| "assetBundle.previewUnavailable")?,
    ) {
        return Err("archive.pathEscape".into());
    }
    let bytes = crate::level_sets::preview_reader::read_limited(&info)?;
    let key = fingerprint(root, &local, &bytes);
    let mut cache = cache().lock().map_err(|_| "lock.poisoned")?;
    if cache.as_ref().is_none_or(|e| e.key != key) {
        *cache = Some(Entry {
            key: key.clone(),
            context: Context::load(&bytes, &local, root)?,
        });
    }
    f(&mut cache.as_mut().unwrap().context, &key)
}
pub(crate) fn catalog(root: &Path, dir: &str, enabled: bool) -> Result<Vec<Level>, String> {
    with_context(root, dir, enabled, |c, _| {
        c.refresh_files();
        Ok(c.levels.clone())
    })
}
fn disk_dir() -> Option<PathBuf> {
    crate::user_paths::root()
        .ok()
        .map(|p| p.join("cache/level-preview-v4"))
}
fn read_disk_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Option<T> {
    let meta = path.metadata().ok()?;
    if meta.len() > limit {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
fn write_disk_json<T: serde::Serialize>(path: &Path, value: &T, limit: usize) {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return;
    };
    if bytes.len() > limit || std::fs::create_dir_all(path.parent().unwrap()).is_err() {
        return;
    }
    let temp = path.with_extension("tmp");
    if std::fs::write(&temp, bytes).is_ok() {
        let _ = std::fs::rename(&temp, path);
    }
    let _ = std::fs::remove_file(temp);
}
pub(crate) fn detail(
    root: &Path,
    dir: &str,
    enabled: bool,
    index: usize,
    part: &str,
) -> Result<Detail, String> {
    if !matches!(part, "picture" | "recipes") {
        return Err("assetBundle.previewUnavailable".into());
    }
    with_context(root, dir, enabled, |c, key| {
        if index >= c.levels.len() {
            return Err("assetBundle.previewUnavailable".into());
        }
        if part == "recipes" {
            let path = disk_dir().map(|p| p.join(format!("{key}-{index}-recipes.json")));
            if let Some(path) = &path {
                if let Some(recipes) = read_disk_json(path, 32 * 1024 * 1024) {
                    return Ok(Detail {
                        picture: None,
                        recipes: Some(recipes),
                    });
                }
            }
            let recipes = c.recipes(index)?;
            if let Some(path) = path {
                write_disk_json(&path, &recipes, 32 * 1024 * 1024);
            }
            return Ok(Detail {
                picture: None,
                recipes: Some(recipes),
            });
        }
        let path = disk_dir().map(|p| p.join(format!("{key}-{index}.json")));
        if let Some(path) = &path {
            if let Some(picture) = read_disk_json(path, 12 * 1024 * 1024) {
                return Ok(Detail {
                    picture: Some(picture),
                    recipes: None,
                });
            }
        }
        let picture = c.picture(index)?;
        if picture.state != "error" {
            if let Some(path) = path {
                write_disk_json(&path, &picture, 12 * 1024 * 1024);
            }
        }
        Ok(Detail {
            picture: Some(picture),
            recipes: None,
        })
    })
}
pub(crate) fn clear() -> Result<usize, String> {
    *cache().lock().unwrap_or_else(|p| p.into_inner()) = None;
    super::preview_reader::clear_shared_textures();
    let mut count = 0;
    if let Some(dir) = disk_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if matches!(
                    path.extension().and_then(|x| x.to_str()),
                    Some("json" | "tmp")
                ) && path.is_file()
                {
                    std::fs::remove_file(path).map_err(|_| "assetBundle.previewUnavailable")?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}
