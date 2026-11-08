//! ZIP 解析结果的短小进程内缓存；路径、长度或修改时间变化就重新解析。
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::models::PublishZipInspection;

const CAPACITY: usize = 8;
static CACHE: Mutex<VecDeque<(Fingerprint, PublishZipInspection)>> = Mutex::new(VecDeque::new());

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}

impl Fingerprint {
    fn read(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|_| "mapPublish.zipUnreadable".to_string())?;
        let metadata = path
            .metadata()
            .map_err(|_| "mapPublish.zipUnreadable".to_string())?;
        if !metadata.is_file() {
            return Err("mapPublish.zipUnreadable".to_string());
        }
        if metadata.len() > super::MAX_ASSET_BYTES {
            return Err(crate::locale_tag::with_params(
                "mapPublish.assetTooLarge",
                &[("limit", &(super::MAX_ASSET_BYTES / 1024 / 1024).to_string())],
            ));
        }
        Ok(Self {
            path,
            len: metadata.len(),
            modified: metadata
                .modified()
                .map_err(|_| "mapPublish.zipUnreadable".to_string())?,
        })
    }
}

pub(in crate::map_publish) fn inspect(path: &Path) -> Result<PublishZipInspection, String> {
    inspect_with(&CACHE, path, super::inspect_publish_zip_blocking)
}

fn inspect_with(
    cache: &Mutex<VecDeque<(Fingerprint, PublishZipInspection)>>,
    path: &Path,
    parse: impl FnOnce(&Path) -> Result<PublishZipInspection, String>,
) -> Result<PublishZipInspection, String> {
    let before = Fingerprint::read(path)?;
    // 解析和磁盘 I/O 不持锁；缓存不是身份校验，上传仍要对比远端地图身份。
    if let Some(result) = cache
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?
        .iter()
        .find(|(key, _)| key == &before)
        .map(|(_, result)| result.clone())
    {
        return Ok(result);
    }
    let result = parse(path)?;
    if Fingerprint::read(path)? != before {
        return Err(super::invalid_asset(
            "ZIP changed during inspection; select it again",
        ));
    }
    let mut entries = cache.lock().map_err(|_| "lock.poisoned".to_string())?;
    entries.retain(|(key, _)| key.path != before.path);
    if entries.len() >= CAPACITY {
        entries.pop_front();
    }
    entries.push_back((before, result.clone()));
    Ok(result)
}

#[cfg(test)]
#[path = "map_publish_zip_cache/tests.rs"]
mod tests;
