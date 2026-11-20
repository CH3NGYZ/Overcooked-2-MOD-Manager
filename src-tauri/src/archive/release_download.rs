//! Release assets share discovery, route execution, integrity and temporary
//! ownership across MOD and map staging. No installation writes happen here.
use super::progress::VolumeProgress;
use super::{DownloadPhase, DownloadProgress};
use crate::download::planning::RepositoryDownloadPlan;
use crate::download::{DownloadContext, try_download_candidates_with_progress};
use crate::models::{NetworkConfig, ReleaseAsset};
use futures_util::{StreamExt, TryStreamExt, stream};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

mod cache;
mod reuse;
mod split_proof;

pub(crate) use reuse::ReleaseCacheScope;
use reuse::ReusableRelease;
pub(crate) use split_proof::SplitZipProof;

/// Exact file stem and same Release only; callers must supply the complete
/// asset list, including companions that do not match the install asset rule.
pub(crate) fn asset_set(
    main: &ReleaseAsset,
    assets: &[ReleaseAsset],
) -> Result<Vec<ReleaseAsset>, String> {
    let Some(stem) = main.name.strip_suffix(".zip") else {
        return Ok(vec![main.clone()]);
    };
    let prefix = format!("{stem}.z");
    let mut parts = BTreeMap::new();
    for asset in assets {
        let Some(suffix) = asset.name.strip_prefix(&prefix) else {
            continue;
        };
        if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let number = suffix
            .parse::<usize>()
            .map_err(|_| "download.invalidVolumeSet")?;
        if suffix.len() != 2
            || number == 0
            || number > 99
            || parts.insert(number, asset.clone()).is_some()
        {
            return Err("download.invalidVolumeSet".into());
        }
    }
    if parts.is_empty() {
        return Ok(vec![main.clone()]);
    }
    if assets
        .iter()
        .filter(|asset| asset.name == main.name)
        .count()
        != 1
        || parts.keys().copied().ne(1..=parts.len())
    {
        return Err("download.invalidVolumeSet".into());
    }
    Ok(parts
        .into_values()
        .chain(std::iter::once(main.clone()))
        .collect())
}

/// Always own completed paths, even when another volume fails or the parent
/// future is dropped. User-provided local ZIPs never enter this guard.
#[derive(Debug)]
struct TemporaryFile(Option<PathBuf>);
impl TemporaryFile {
    fn path(&self) -> &Path {
        self.0.as_deref().unwrap()
    }
    fn keep(mut self) -> PathBuf {
        self.0.take().unwrap()
    }
}
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn expected_size(asset: &ReleaseAsset) -> Option<u64> {
    (asset.size > 0).then_some(asset.size)
}

fn is_zip(asset: &ReleaseAsset) -> bool {
    asset.name.to_ascii_lowercase().ends_with(".zip")
}

fn merge_volumes(files: &[TemporaryFile], cache: &Path) -> Result<TemporaryFile, String> {
    let output = TemporaryFile(Some(cache.join(format!("{}.zip", uuid::Uuid::new_v4()))));
    let paths = files.iter().map(TemporaryFile::path).collect::<Vec<_>>();
    super::split_zip::reassemble(&paths, output.path())?;
    Ok(output)
}

#[derive(Debug)]
pub(crate) struct DownloadedRelease {
    file: TemporaryFile,
    split_assets: Option<Vec<ReleaseAsset>>,
}

impl DownloadedRelease {
    pub(crate) fn path(&self) -> &Path {
        self.file.path()
    }

    pub(crate) fn into_path(self) -> PathBuf {
        self.file.keep()
    }

    pub(crate) fn split_proof(&self) -> Result<Option<SplitZipProof>, String> {
        self.split_assets
            .as_ref()
            .map(|assets| SplitZipProof::capture(self.path(), assets.clone()))
            .transpose()
    }
}

fn aggregate(label: &str, volumes: &[VolumeProgress], finished: bool) -> DownloadProgress {
    let mut result = DownloadProgress::connecting(label);
    // `asset_set` puts the main ZIP last, even when another volume is active.
    result.set_file_name(volumes.last().map(|v| v.name.as_str()));
    result.downloaded = volumes.iter().map(|v| v.progress.downloaded).sum();
    result.total = volumes
        .iter()
        .try_fold(0u64, |sum, v| sum.checked_add(v.progress.total?));
    result.speed_bytes = volumes
        .iter()
        .filter(|v| v.state == "downloading")
        .map(|v| v.progress.speed_bytes)
        .sum();
    result.phase = if finished {
        DownloadPhase::Finished
    } else {
        DownloadPhase::Downloading
    };
    result.finished = finished;
    result.volumes = volumes.to_vec();
    result
}

pub(crate) async fn download<F>(
    client: &reqwest::Client,
    assets: Vec<ReleaseAsset>,
    cache: &Path,
    cache_scope: Option<&ReleaseCacheScope>,
    label: &str,
    app: Option<&AppHandle>,
    network: &NetworkConfig,
    plan: F,
) -> Result<DownloadedRelease, String>
where
    F: Fn(&str) -> Result<RepositoryDownloadPlan, String>,
{
    let split = assets.len() > 1;
    if assets.is_empty() {
        return Err("source.noMatchingAsset".into());
    }
    let reuse = cache_scope.and_then(|scope| ReusableRelease::new(cache, scope, &assets));
    if let Some(reuse) = reuse.clone() {
        let cache = cache.to_owned();
        let cached = tauri::async_runtime::spawn_blocking(move || reuse.restore(&cache)).await;
        if super::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        if let Some((file, sizes)) = cached.unwrap_or(Ok(None))? {
            log::debug!("已复用 Release 资产缓存：{label}");
            let volumes = assets
                .iter()
                .zip(sizes)
                .map(|(asset, size)| {
                    let mut progress = DownloadProgress::connecting(label);
                    progress.downloaded = size;
                    progress.total = Some(size);
                    progress.phase = DownloadPhase::Finished;
                    progress.finished = true;
                    VolumeProgress {
                        name: asset.name.clone(),
                        state: "completed".into(),
                        retry: 0,
                        progress: Box::new(progress),
                    }
                })
                .collect::<Vec<_>>();
            let mut finished = aggregate(label, &volumes, true);
            if !split {
                finished.volumes.clear();
            }
            super::emit_progress_event(app, finished);
            return Ok(DownloadedRelease {
                file,
                split_assets: split.then_some(assets),
            });
        }
        log::debug!("Release 资产缓存未命中：{label}");
    }
    // Resolve all routes before opening a connection, using the caller's task
    // snapshot. Each volume preserves the selected plan's race/thread limits.
    let plans = assets
        .iter()
        .map(|a| plan(&a.download_url))
        .collect::<Result<Vec<_>, _>>()?;
    let volumes = Arc::new(Mutex::new(
        assets
            .iter()
            .map(|a| {
                let mut progress = DownloadProgress::connecting(label);
                progress.total = expected_size(a);
                VolumeProgress {
                    name: a.name.clone(),
                    state: "pending".into(),
                    retry: 0,
                    progress: Box::new(progress),
                }
            })
            .collect::<Vec<_>>(),
    ));
    let split_assets = split.then(|| assets.clone());
    let files = stream::iter(assets.into_iter().zip(plans).enumerate().map(
        |(index, (asset, planned))| {
            let volumes = volumes.clone();
            async move {
                let expected_size = expected_size(&asset);
                for retry in 0..=if split { 3 } else { 0 } {
                    if super::is_download_cancelled() {
                        return Err("download.cancelled".into());
                    }
                    let result = try_download_candidates_with_progress(
                        planned.candidates.clone(),
                        super::is_download_cancelled,
                        |candidate, reporter| {
                            let digest = asset.digest.as_deref();
                            let zip = !split && is_zip(&asset);
                            let threads = planned.download_threads;
                            async move {
                                let path =
                                    super::download_to_temp_with_threads_and_first_byte_timeout(
                                        client,
                                        &candidate.url,
                                        cache,
                                        label,
                                        &reporter,
                                        threads,
                                        Some(network.first_byte_timeout()),
                                    )
                                    .await?;
                                let file = TemporaryFile(Some(path));
                                super::validate_download_file(file.path(), expected_size, digest)?;
                                if zip {
                                    super::integrity::validate_zip_with_cancel(
                                        file.path(),
                                        super::is_download_cancelled,
                                    )?;
                                }
                                Ok(file.keep())
                            }
                        },
                        network,
                        Some(DownloadContext {
                            execution: Some(planned.execution),
                            app: None,
                            label,
                            file_name: (!split).then_some(asset.name.as_str()),
                            expected_size,
                        }),
                        |progress| {
                            if split {
                                let mut all = volumes.lock().unwrap_or_else(|e| e.into_inner());
                                all[index].state = if progress.finished {
                                    "completed"
                                } else {
                                    "downloading"
                                }
                                .into();
                                all[index].retry = retry;
                                all[index].progress = Box::new(progress);
                                super::emit_progress_event(app, aggregate(label, &all, false));
                            } else {
                                super::emit_progress_event(app, progress);
                            }
                        },
                    )
                    .await;
                    match result {
                        Ok(path) => return Ok((index, TemporaryFile(Some(path)))),
                        Err(error) => {
                            if split {
                                let mut all = volumes.lock().unwrap_or_else(|e| e.into_inner());
                                all[index].state = "failed".into();
                                all[index].retry = retry;
                                all[index].progress.downloaded = 0;
                                all[index].progress.speed_bytes = 0;
                                super::emit_progress_event(app, aggregate(label, &all, false));
                            }
                            if error == "download.cancelled" || retry == if split { 3 } else { 0 } {
                                return Err(error);
                            }
                        }
                    }
                }
                unreachable!()
            }
        },
    ))
    .buffer_unordered(if split { 2 } else { 1 })
    .try_collect::<Vec<_>>()
    .await?;
    let mut files = files;
    files.sort_by_key(|(index, _)| *index);
    if !split && reuse.is_none() {
        return Ok(DownloadedRelease {
            file: files.pop().unwrap().1,
            split_assets: None,
        });
    }
    if split {
        let mut merging = aggregate(
            label,
            &volumes.lock().unwrap_or_else(|e| e.into_inner()),
            false,
        );
        merging.phase = DownloadPhase::Merging;
        super::emit_progress_event(app, merging);
    }
    let cache = cache.to_owned();
    // Reassembly (including CRC) and copying into the persistent cache are
    // blocking. Move the guards in so dropping the await cannot orphan files.
    let output = tauri::async_runtime::spawn_blocking(move || {
        let mut files = files.into_iter().map(|(_, file)| file).collect::<Vec<_>>();
        let merged = split.then(|| merge_volumes(&files, &cache)).transpose()?;
        if let Some(reuse) = reuse {
            reuse.remember(&files);
        }
        Ok::<_, String>(DownloadedRelease {
            file: merged.unwrap_or_else(|| files.pop().unwrap()),
            split_assets,
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    if super::is_download_cancelled() {
        return Err("download.cancelled".into());
    }
    if split {
        super::emit_progress_event(
            app,
            aggregate(
                label,
                &volumes.lock().unwrap_or_else(|e| e.into_inner()),
                true,
            ),
        );
    }
    Ok(output)
}

#[cfg(test)]
pub(crate) mod tests;
