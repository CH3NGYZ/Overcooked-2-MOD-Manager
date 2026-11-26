use crate::{
    app_state::AppState,
    level_sets,
    models::{AppConfig, ModDownloadMode, ModRepositoryConfig, ReleaseInfo},
    releases::repository_cache_key,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::Path,
    sync::OnceLock,
};

pub(in crate::mod_changes) fn replan_scope(
    state: &AppState,
    root: &Path,
    config: &AppConfig,
) -> Result<String, String> {
    static KEY: OnceLock<uuid::Uuid> = OnceLock::new();
    let selection = state.config.data_selection();
    if selection.0 != config.data_provider {
        return Err("modChanges.targetChanged".into());
    }
    let mut hash = Sha256::new();
    hash.update(KEY.get_or_init(uuid::Uuid::new_v4).as_bytes());
    hash.update(selection.1.to_le_bytes());
    for path in [root, state.config.config_path().as_path()] {
        let bytes = path.as_os_str().as_encoded_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    let mut settings = config.clone();
    for repository in &mut settings.repositories {
        repository.etag = None;
        repository.cached_releases.clear();
        repository.cached_releases_complete = false;
        repository.release_prerelease_metadata.clear();
        repository.releases_cached_at = None;
        repository.last_error = None;
    }
    for repository in &mut settings.map_repositories {
        repository.cached_releases.clear();
        repository.releases_cached_at = None;
        repository.last_error = None;
    }
    settings.staged_release_cache.clear();
    hash.update(serde_json::to_vec(&settings).map_err(|_| "modChanges.targetChanged")?);
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn revision(
    state: &AppState,
    root: &Path,
    config: &AppConfig,
) -> Result<String, String> {
    let scope = replan_scope(state, root, config)?;
    revision_for_disk(config, disk_revision(state, root, &scope)?)
}

fn release_key(repository: &ModRepositoryConfig) -> String {
    repository_cache_key(
        repository.provider.clone(),
        &repository.username,
        &repository.repository,
    )
}

fn strip_download_counts(releases: &mut [ReleaseInfo]) {
    for asset in releases.iter_mut().flat_map(|release| &mut release.assets) {
        asset.download_count = None;
    }
}

/// Keep cache availability and the ordering used to select exact prerelease
/// evidence across snapshots of one repository, not the wall-clock values.
fn normalize_cache_metadata(config: &mut AppConfig) {
    let mut clocks = HashMap::<String, BTreeSet<i64>>::new();
    for repository in &config.repositories {
        if repository.download_mode == ModDownloadMode::Release
            && let Some(time) = repository.releases_cached_at
        {
            clocks
                .entry(release_key(repository))
                .or_default()
                .insert(time);
        }
    }
    for snapshot in config.staged_release_cache.values() {
        if let (Some(key), Some(time)) = (&snapshot.repository_key, snapshot.cached_at) {
            clocks.entry(key.clone()).or_default().insert(time);
        }
    }
    let rank = |key: &str, time| {
        clocks
            .get(key)
            .map_or(0, |times| times.range(..time).count() as i64)
    };
    for repository in &mut config.repositories {
        let key = release_key(repository);
        repository.releases_cached_at = repository.releases_cached_at.map(|time| rank(&key, time));
        repository.etag = None;
        repository.last_error.iter_mut().for_each(String::clear);
        strip_download_counts(&mut repository.cached_releases);
    }
    for repository in &mut config.map_repositories {
        repository.releases_cached_at = repository.releases_cached_at.map(|_| 0);
        repository.last_error.iter_mut().for_each(String::clear);
        strip_download_counts(&mut repository.cached_releases);
    }
    for snapshot in config.staged_release_cache.values_mut() {
        snapshot.cached_at = snapshot.cached_at.map(|time| {
            snapshot
                .repository_key
                .as_deref()
                .map_or(0, |key| rank(key, time))
        });
        snapshot.last_error.iter_mut().for_each(String::clear);
        strip_download_counts(&mut snapshot.releases);
    }
}

pub(in crate::mod_changes) fn revision_for_disk(
    config: &AppConfig,
    disk: u64,
) -> Result<String, String> {
    let mut config = config.clone();
    normalize_cache_metadata(&mut config);
    let mut hash = DefaultHasher::new();
    disk.hash(&mut hash);
    serde_json::to_vec(&config)
        .map_err(|_| "modChanges.targetChanged")?
        .hash(&mut hash);
    Ok(format!("{:016x}", hash.finish()))
}

pub(in crate::mod_changes) fn disk_revision(
    state: &AppState,
    root: &Path,
    scope: &str,
) -> Result<u64, String> {
    let mut hash = DefaultHasher::new();
    scope.hash(&mut hash);
    for directory in [
        root.join("BepInEx/plugins"),
        level_sets::disabled_root(root),
        state.config.manifests_dir(),
    ] {
        if !directory.exists() {
            continue;
        }
        let mut entries = Vec::new();
        for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
            let entry = entry.map_err(|error| error.to_string())?;
            // Windows directory enumeration may retain the parent's timestamp
            // from before its newest child was closed. Read current metadata by
            // path rather than hashing WalkDir's cached enumeration metadata.
            let meta =
                std::fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("archive.pathEscape".into());
            }
            entries.push((entry.path().to_path_buf(), meta.len(), meta.modified().ok()));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.hash(&mut hash);
    }
    Ok(hash.finish())
}
