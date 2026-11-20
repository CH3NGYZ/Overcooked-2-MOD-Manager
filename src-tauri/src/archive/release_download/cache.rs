//! A disposable store for verified Release assets, separate from installation
//! staging. Only provider SHA-256 values can authorize reusing cached bytes.

use crate::archive::integrity::{
    normalize_sha256, validate_download_digest, validate_download_size,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, Metadata};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::{Builder, TempPath};
use uuid::Uuid;

const RETAINED_VERSIONS: usize = 3;
const INDEX_LIMIT: u64 = 4096;
const INDEX: &str = "versions.json";
const ARCHIVE_SUFFIX: &str = ".asset";

// All handles in a process share the same lock, including cache handles made
// by separate callers. Only metadata and publication need it, never ZIP I/O.
static ACCESS: Mutex<()> = Mutex::new(());
static PROCESS_TEMP_PREFIX: OnceLock<String> = OnceLock::new();

#[derive(Clone)]
pub(super) struct ReleaseDownloadCache {
    root: PathBuf,
}

pub(super) struct ReleaseCacheKey {
    scope: String,
    version: String,
    asset: String,
    digest: String,
}

impl ReleaseCacheKey {
    pub(super) fn new(
        scope: &str,
        version: &str,
        asset: &str,
        api_digest: Option<&str>,
    ) -> Option<Self> {
        if scope.trim().is_empty() || version.trim().is_empty() || asset.trim().is_empty() {
            return None;
        }
        Some(Self {
            scope: key_hash(scope),
            version: key_hash(version),
            asset: key_hash(asset),
            digest: normalize_sha256(api_digest?)?.to_ascii_lowercase(),
        })
    }

    fn asset_path(&self, version: &Path) -> PathBuf {
        version.join(format!("{}-{}{ARCHIVE_SUFFIX}", self.asset, self.digest))
    }
}

impl ReleaseDownloadCache {
    /// `root` is a manager-owned directory under the existing data root's cache directory.
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The copy remains owned by TempPath if the awaiting task is dropped.
    /// The caller can keep it when handing ownership to the installation flow.
    pub(super) fn restore(
        &self,
        key: &ReleaseCacheKey,
        temp_dir: &Path,
        expected_size: Option<u64>,
        cancelled: impl Fn() -> bool,
    ) -> Option<TempPath> {
        let mut input = {
            let _access = ACCESS.lock().ok()?;
            let scope = self.scope(key, false).ok()?;
            if !read_index(&scope).ok()?.contains(&key.version) {
                return None;
            }
            let version = scope.join(&key.version);
            directory(&version, false).ok()?;
            open_regular(&key.asset_path(&version)).ok()?
        };
        let result = (|| {
            fs::create_dir_all(temp_dir)?;
            let mut output = Builder::new()
                .prefix(&Uuid::new_v4().to_string())
                .rand_bytes(0)
                .suffix(".download")
                .tempfile_in(temp_dir)?;
            copy_verified(
                &mut input,
                output.as_file_mut(),
                key,
                expected_size,
                &cancelled,
            )?;
            Ok::<_, io::Error>(output.into_temp_path())
        })();
        match result {
            Ok(output) => Some(output),
            Err(error) => {
                log::debug!("无法复用 Release 资产缓存：{error}");
                None
            }
        }
    }

    /// Promote only a complete, independently verified download, not a volume of it.
    pub(super) fn touch(&self, key: &ReleaseCacheKey) {
        // The version may have been evicted while we copied it. Do not revive it.
        if let Ok(_access) = ACCESS.lock()
            && let Ok(scope) = self.scope(key, false)
            && let Ok(mut versions) = read_index(&scope)
            && versions.contains(&key.version)
        {
            if versions.first() != Some(&key.version) {
                promote(&mut versions, &key.version);
                let _ = write_index(&scope, &versions);
            }
            // A prior process can stop between publishing the index and
            // pruning. Only inspect this scope and the direct files of stale
            // versions; retained versions are left untouched.
            prune_scope(&scope, &versions);
        }
    }

    /// Advance the version index only after the whole validated asset set is on
    /// disk. A failed part or cancelled copy must not evict a usable version.
    pub(super) fn remember_set(
        &self,
        assets: &[(&ReleaseCacheKey, &Path, Option<u64>)],
        cancelled: impl Fn() -> bool,
    ) {
        let Some((key, _, _)) = assets.first() else {
            return;
        };
        let result = (|| {
            check_cancelled(&cancelled)?;
            let mut names = HashSet::new();
            if !assets.iter().all(|(part, _, _)| {
                part.scope == key.scope
                    && part.version == key.version
                    && names.insert(part.asset.as_str())
            }) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "invalid Release asset set",
                ));
            }
            let scope = self.scope(key, true)?;
            let version = scope.join(&key.version);
            let mut staged = Vec::with_capacity(assets.len());
            for (part, downloaded_path, expected_size) in assets {
                let mut input = open_regular(downloaded_path)?;
                let mut output = Builder::new()
                    .prefix(process_temp_prefix())
                    .suffix(".tmp")
                    .tempfile_in(&scope)?;
                copy_verified(
                    &mut input,
                    output.as_file_mut(),
                    part,
                    *expected_size,
                    &cancelled,
                )?;
                output.as_file().sync_all()?;
                staged.push((output.into_temp_path(), part.asset_path(&version)));
            }
            let _access = ACCESS
                .lock()
                .map_err(|_| io::Error::other("Release cache access lock poisoned"))?;
            check_cancelled(&cancelled)?;
            if self.scope(key, false)? != scope {
                return Err(unsafe_path());
            }
            let mut versions = read_index(&scope)?;
            directory(&version, true)?;
            for (_, asset) in &staged {
                // Replace stale regular files atomically; don't overwrite
                // symlinks or traverse Windows junctions in the cache layout.
                ignore_missing(regular_file(asset))?;
            }
            let mut current = HashSet::new();
            for (output, asset) in staged {
                output.persist(&asset).map_err(|error| error.error)?;
                current.insert(asset);
            }
            promote(&mut versions, &key.version);
            write_index(&scope, &versions)?;
            prune_replaced_assets(&version, &names, &current);
            prune_scope(&scope, &versions);
            Ok::<_, io::Error>(())
        })();
        if let Err(error) = result {
            log::debug!("未保存 Release 资产缓存：{error}");
        }
    }

    fn scope(&self, key: &ReleaseCacheKey, create: bool) -> io::Result<PathBuf> {
        // The user data root may itself be redirected. Keep its managed cache
        // directory, this store and each hashed child free of links/reparse points.
        let parent = self.root.parent().ok_or_else(unsafe_path)?;
        directory(parent, create)?;
        directory(&self.root, create)?;
        let scope = self.root.join(&key.scope);
        directory(&scope, create)?;
        // TempPath::persist uses MoveFileExW directly on Windows. Resolving
        // this already checked directory supplies the verbatim path it needs
        // for long names while keeping the cache in the intended data root.
        let resolved = scope.canonicalize()?;
        let data_root = parent.parent().ok_or_else(unsafe_path)?.canonicalize()?;
        if !resolved.starts_with(&data_root) || resolved == data_root {
            return Err(unsafe_path());
        }
        Ok(resolved)
    }
}

fn copy_verified(
    input: &mut File,
    output: &mut File,
    key: &ReleaseCacheKey,
    expected_size: Option<u64>,
    cancelled: &impl Fn() -> bool,
) -> io::Result<()> {
    validate_download_size(input.metadata()?.len(), expected_size).map_err(io::Error::other)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let bytes = &buffer[..read];
        output.write_all(bytes)?;
        hash.update(bytes);
        count += read as u64;
    }
    validate_download_size(count, expected_size).map_err(io::Error::other)?;
    validate_download_digest(&format!("{:x}", hash.finalize()), &key.digest)
        .map_err(io::Error::other)
}

fn check_cancelled(cancelled: &impl Fn() -> bool) -> io::Result<()> {
    if cancelled() {
        return Err(io::Error::new(
            ErrorKind::Interrupted,
            "Release cache copy cancelled",
        ));
    }
    Ok(())
}

fn key_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn process_temp_prefix() -> &'static str {
    PROCESS_TEMP_PREFIX.get_or_init(|| format!("incoming-{}-", Uuid::new_v4()))
}

fn is_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn promote(versions: &mut Vec<String>, version: &str) {
    versions.retain(|value| value != version);
    versions.insert(0, version.to_owned());
    versions.truncate(RETAINED_VERSIONS);
}

fn read_index(scope: &Path) -> io::Result<Vec<String>> {
    let file = match open_regular(&scope.join(INDEX)) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(INDEX_LIMIT + 1).read_to_end(&mut bytes)?;
    // A bad index is a miss, never a source of filenames or retention policy.
    let versions: Vec<String> = if bytes.len() as u64 <= INDEX_LIMIT {
        serde_json::from_slice(&bytes).unwrap_or_default()
    } else {
        Vec::new()
    };
    if versions.len() > RETAINED_VERSIONS
        || !versions
            .iter()
            .enumerate()
            .all(|(index, version)| is_key(version) && !versions[..index].contains(version))
    {
        return Ok(Vec::new());
    }
    Ok(versions)
}

fn write_index(scope: &Path, versions: &[String]) -> io::Result<()> {
    let mut output = Builder::new()
        .prefix("versions-")
        .suffix(".tmp")
        .tempfile_in(scope)?;
    serde_json::to_writer(&mut output, versions)?;
    output.as_file().sync_all()?;
    output
        .into_temp_path()
        .persist(scope.join(INDEX))
        .map_err(|error| error.error)
}

fn prune_replaced_assets(version: &Path, names: &HashSet<&str>, current: &HashSet<PathBuf>) {
    if let Ok(entries) = fs::read_dir(version) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let path = entry.path();
            if indexed_asset(name.to_str().unwrap_or_default())
                .is_some_and(|(name, _)| names.contains(name))
                && !current.contains(&path)
                && regular_file(&path).is_ok()
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn prune_scope(scope: &Path, versions: &[String]) {
    if let Ok(entries) = fs::read_dir(scope) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_str().unwrap_or_default();
            let expired = entry.path();
            if is_stale_temporary(name) && regular_file(&expired).is_ok() {
                let _ = fs::remove_file(expired);
            } else if is_key(name)
                && !versions.iter().any(|version| version == name)
                && directory(&expired, false).is_ok()
                && let Ok(assets) = fs::read_dir(&expired)
            {
                for entry in assets.flatten() {
                    let path = entry.path();
                    if indexed_asset(entry.file_name().to_str().unwrap_or_default()).is_some()
                        && regular_file(&path).is_ok()
                    {
                        let _ = fs::remove_file(path);
                    }
                }
                // Never recurse into unknown entries, even within the cache.
                let _ = fs::remove_dir(expired);
            }
        }
    }
}

fn is_stale_temporary(name: &str) -> bool {
    // The manager's data session excludes other running instances. Ignore
    // this process's concurrent incoming assets, and only collect our own
    // recognized temporary names after a prior process ended mid-write.
    if let Some(stem) = name.strip_suffix(".tmp") {
        if let Some(stem) = stem.strip_prefix("versions-") {
            return stem.len() == 6 && stem.bytes().all(|byte| byte.is_ascii_alphanumeric());
        }
        if let Some(stem) = stem.strip_prefix("incoming-")
            && let Some((id, random)) = stem.rsplit_once('-')
        {
            return Uuid::parse_str(id).is_ok()
                && random.len() == 6
                && random.bytes().all(|byte| byte.is_ascii_alphanumeric())
                && !name.starts_with(process_temp_prefix());
        }
    }
    false
}

fn indexed_asset(name: &str) -> Option<(&str, &str)> {
    let (name, digest) = name.strip_suffix(ARCHIVE_SUFFIX)?.split_once('-')?;
    (is_key(name) && is_key(digest)).then_some((name, digest))
}

fn ignore_missing<T>(result: io::Result<T>) -> io::Result<()> {
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn open_regular(path: &Path) -> io::Result<File> {
    regular_file(path)?;
    File::open(path)
}

fn regular_file(path: &Path) -> io::Result<Metadata> {
    let metadata = checked_metadata(path)?;
    if !metadata.is_file() {
        return Err(unsafe_path());
    }
    Ok(metadata)
}

fn directory(path: &Path, create: bool) -> io::Result<()> {
    let metadata = match checked_metadata(path) {
        Err(error) if create && error.kind() == ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Err(error) if error.kind() != ErrorKind::AlreadyExists => return Err(error),
                _ => {}
            }
            checked_metadata(path)?
        }
        result => result?,
    };
    if !metadata.is_dir() {
        return Err(unsafe_path());
    }
    Ok(())
}

fn checked_metadata(path: &Path) -> io::Result<Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(unsafe_path());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(unsafe_path());
        }
    }
    Ok(metadata)
}

fn unsafe_path() -> io::Error {
    io::Error::new(ErrorKind::InvalidData, "unsafe Release cache path")
}

#[cfg(test)]
mod tests;
