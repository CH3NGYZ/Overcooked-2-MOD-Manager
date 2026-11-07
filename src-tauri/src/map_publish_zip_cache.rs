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
mod tests {
    use super::*;
    use std::fs::{self, File, FileTimes};
    use std::time::{Duration, UNIX_EPOCH};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("publish-cache-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn parsed(_: &Path) -> Result<PublishZipInspection, String> {
        Ok(PublishZipInspection {
            info_version: "1.2".into(),
            ..Default::default()
        })
    }

    #[test]
    fn unchanged_zip_is_parsed_once() {
        let root = Fixture::new();
        let path = root.file("a.zip", b"one");
        let cache = Mutex::new(VecDeque::new());
        inspect_with(&cache, &path, parsed).unwrap();
        let hit = inspect_with(&cache, &root.0.join(".").join("a.zip"), |_| {
            panic!("cache miss")
        })
        .unwrap();
        assert_eq!(hit.info_version, "1.2");
    }

    #[test]
    fn changed_size_mtime_and_path_invalidate() {
        let root = Fixture::new();
        let path = root.file("a.zip", b"one");
        let cache = Mutex::new(VecDeque::new());
        let mut calls = 0;
        let mut parse = |p: &Path| {
            calls += 1;
            parsed(p)
        };
        inspect_with(&cache, &path, &mut parse).unwrap();
        fs::write(&path, b"longer").unwrap();
        inspect_with(&cache, &path, &mut parse).unwrap();
        let file = File::options().write(true).open(&path).unwrap();
        file.set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1234567890)))
            .unwrap();
        inspect_with(&cache, &path, &mut parse).unwrap();
        inspect_with(&cache, &root.file("b.zip", b"longer"), &mut parse).unwrap();
        assert_eq!(calls, 4);
    }

    #[test]
    fn errors_and_files_changed_during_parse_are_not_cached() {
        let root = Fixture::new();
        let path = root.file("a.zip", b"one");
        let cache = Mutex::new(VecDeque::new());
        assert!(inspect_with(&cache, &path, |_| Err("bad ZIP".into())).is_err());
        assert!(cache.lock().unwrap().is_empty());
        assert!(
            inspect_with(&cache, &path, |p| {
                fs::write(p, b"changed").unwrap();
                parsed(p)
            })
            .is_err()
        );
        assert!(cache.lock().unwrap().is_empty());
        inspect_with(&cache, &path, parsed).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(inspect_with(&cache, &path, |_| panic!("deleted file parsed")).is_err());
    }

    #[test]
    fn cache_is_bounded() {
        let root = Fixture::new();
        let cache = Mutex::new(VecDeque::new());
        for i in 0..CAPACITY + 3 {
            inspect_with(&cache, &root.file(&format!("{i}.zip"), b"zip"), parsed).unwrap();
        }
        assert_eq!(cache.lock().unwrap().len(), CAPACITY);
        assert_eq!(
            cache
                .lock()
                .unwrap()
                .front()
                .unwrap()
                .0
                .path
                .file_name()
                .unwrap(),
            "3.zip"
        );
    }
}
