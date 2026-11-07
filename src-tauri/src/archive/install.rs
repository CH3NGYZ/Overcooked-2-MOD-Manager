use super::paths::{clear_readonly, path_relative};
use super::zip_layout::{ModZipInstallPlan, local_mod_zip_plan, repository_mod_zip_plan};
use crate::game::safe_game_path;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::ZipArchive;

pub(crate) fn extract_framework_zip_pending(
    zip_path: &Path,
    game_root: &Path,
) -> Result<PendingModInstall, String> {
    log::info!("开始安全解压 ZIP：文件={}", zip_path.display());
    let result = extract_zip_pending(zip_path, game_root, "", None);
    if let Err(error) = &result {
        log::error!(
            "安全解压 ZIP 失败：文件={}，原因={}",
            zip_path.display(),
            error
        );
    }
    result
}

#[cfg(test)]
fn extract_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    log::info!(
        "开始解压 MOD ZIP：文件={}，目标={}",
        zip_path.display(),
        target_dir
    );
    let plan = prepare_repository_mod_zip(zip_path, game_root, target_dir, archive_name)?;
    extract_prepared_mod_zip(zip_path, game_root, &plan).map(PendingModInstall::commit)
}

#[cfg(test)]
fn extract_local_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    extract_local_mod_zip_pending(zip_path, game_root, archive_name).map(PendingModInstall::commit)
}

pub(crate) fn extract_local_mod_zip_pending(
    zip_path: &Path,
    game_root: &Path,
    archive_name: &str,
) -> Result<PendingModInstall, String> {
    log::info!(
        "开始解压本地 MOD ZIP：文件={}，归档名={}",
        zip_path.display(),
        archive_name
    );
    let plan = local_mod_zip_plan(zip_path, archive_name)?;
    extract_prepared_mod_zip(zip_path, game_root, &plan)
}

pub(crate) fn prepare_repository_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    install_target: &str,
    archive_name: &str,
) -> Result<ModZipInstallPlan, String> {
    let plan = repository_mod_zip_plan(zip_path, install_target, archive_name)?;
    zip_entry_destinations(
        zip_path,
        game_root,
        &plan.target_dir,
        plan.strip_root.as_deref(),
    )?;
    Ok(plan)
}

pub(crate) fn extract_prepared_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    plan: &ModZipInstallPlan,
) -> Result<PendingModInstall, String> {
    extract_zip_pending(
        zip_path,
        game_root,
        &plan.target_dir,
        plan.strip_root.as_deref(),
    )
}

pub(crate) fn prepared_mod_zip_files(
    zip_path: &Path,
    game_root: &Path,
    plan: &ModZipInstallPlan,
) -> Result<Vec<String>, String> {
    zip_entry_destinations(
        zip_path,
        game_root,
        &plan.target_dir,
        plan.strip_root.as_deref(),
    )
    .map(|destinations| {
        destinations
            .into_iter()
            .map(|(_, path)| path_relative(game_root, &path))
            .collect()
    })
}

fn snapshot_path(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        log::trace!("无需创建文件快照，目标不存在：{}", path.display());
        return Ok(None);
    }
    let snapshot = std::env::temp_dir().join(format!("oc2-mod-snapshot-{}", Uuid::new_v4()));
    log::debug!(
        "创建文件快照：源={}，快照={}",
        path.display(),
        snapshot.display()
    );
    copy_path(path, &snapshot)?;
    Ok(Some(snapshot))
}

fn restore_snapshot(snapshot: Option<PathBuf>, target: &Path) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    remove_existing_path(target)?;
    copy_path(&snapshot, target)?;
    log::warn!("已恢复文件快照：目标={}", target.display());
    remove_existing_path(&snapshot)
}

fn discard_snapshot(snapshot: Option<PathBuf>) {
    if let Some(snapshot) = snapshot {
        if let Err(error) = remove_existing_path(&snapshot) {
            log::warn!(
                "清理 MOD 临时快照失败：路径={}，原因={}",
                snapshot.display(),
                error
            );
        }
    }
}

fn remove_existing_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    clear_readonly(path)?;
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| {
            log::error!("删除目录失败：路径={}，原因={}", path.display(), e);
            e.to_string()
        })
    } else {
        fs::remove_file(path).map_err(|e| {
            log::error!("删除文件失败：路径={}，原因={}", path.display(), e);
            e.to_string()
        })
    }
}

fn copy_path(source: &Path, destination: &Path) -> Result<(), String> {
    if source.is_dir() {
        fs::create_dir_all(destination).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Keep overwritten files until the installation manifest has been committed.
/// Dropping an unfinished installation restores them and removes new files/directories.
pub(crate) struct PendingModInstall {
    game_root: PathBuf,
    files: Vec<String>,
    snapshots: Vec<(PathBuf, Option<PathBuf>)>,
    created_directories: Vec<PathBuf>,
}

impl PendingModInstall {
    fn new(game_root: &Path) -> Self {
        Self {
            game_root: fs::canonicalize(game_root).unwrap_or_else(|_| game_root.to_path_buf()),
            files: Vec::new(),
            snapshots: Vec::new(),
            created_directories: Vec::new(),
        }
    }

    fn track_destination(&mut self, destination: &Path) -> Result<(), String> {
        if destination.is_dir() {
            return Err(crate::locale_tag::with_params(
                "archive.targetExists",
                &[("path", &destination.display().to_string())],
            ));
        }
        let missing_parents = destination
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .take_while(|parent| !parent.exists())
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        if missing_parents
            .iter()
            .any(|parent| parent == &self.game_root || !parent.starts_with(&self.game_root))
        {
            return Err("archive.pathEscape".into());
        }
        let snapshot = snapshot_path(destination)?;
        self.snapshots.push((destination.to_path_buf(), snapshot));
        self.created_directories
            .extend(missing_parents.into_iter().rev());
        Ok(())
    }

    pub(crate) fn files(&self) -> &[String] {
        &self.files
    }

    pub(crate) fn commit(mut self) -> Vec<String> {
        for (_, snapshot) in self.snapshots.drain(..) {
            discard_snapshot(snapshot);
        }
        self.created_directories.clear();
        std::mem::take(&mut self.files)
    }

    pub(crate) fn rollback(&mut self) -> Result<(), String> {
        let mut failed = false;
        for (destination, snapshot) in self.snapshots.drain(..).rev() {
            let result = if snapshot.is_some() {
                restore_snapshot(snapshot, &destination)
            } else {
                remove_existing_path(&destination)
            };
            if let Err(error) = result {
                failed = true;
                log::error!(
                    "MOD 安装回滚失败：路径={}，原因={}",
                    destination.display(),
                    error
                );
            }
        }
        // Do not prune pre-existing containers or recursively remove newly added user files.
        for directory in self.created_directories.drain(..).rev() {
            let cleanup = (|| -> Result<(), String> {
                let relative = directory
                    .strip_prefix(&self.game_root)
                    .map_err(|e| e.to_string())?;
                if safe_game_path(&self.game_root, &relative.to_string_lossy())? != directory {
                    return Err("archive.pathEscape".into());
                }
                match fs::remove_dir(&directory) {
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                        ) =>
                    {
                        Ok(())
                    }
                    result => result.map_err(|e| e.to_string()),
                }
            })();
            if let Err(error) = cleanup {
                failed = true;
                log::error!(
                    "安装回滚目录清理失败：路径={}，原因={}",
                    directory.display(),
                    error
                );
            }
        }
        if failed {
            Err("recycle.partialFailure".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for PendingModInstall {
    fn drop(&mut self) {
        let _ = self.rollback();
    }
}

fn zip_entry_destinations(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    strip_root: Option<&str>,
) -> Result<Vec<(usize, PathBuf)>, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &e.to_string())])
    })?;
    let mut destinations = Vec::new();
    let mut unique = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &e.to_string())],
            )
        })?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            crate::locale_tag::with_params("archive.unsafePath", &[("entry", entry.name())])
        })?;
        if entry.is_dir() {
            continue;
        }
        let relative_entry = if let Some(root) = strip_root {
            strip_zip_root(&enclosed, root).ok_or_else(|| {
                crate::locale_tag::with_params(
                    "archive.unexpectedRootDirectory",
                    &[("root", root), ("entry", entry.name())],
                )
            })?
        } else {
            enclosed.to_path_buf()
        };
        let relative = Path::new(target_dir).join(relative_entry);
        let destination = safe_game_path(game_root, &relative.to_string_lossy())?;
        if !unique.insert(destination.to_string_lossy().to_lowercase()) {
            return Err(crate::locale_tag::with_params(
                "archive.targetExists",
                &[("path", &destination.display().to_string())],
            ));
        }
        destinations.push((index, destination));
    }
    if destinations.is_empty() {
        return Err("archive.noInstallableContent".into());
    }
    Ok(destinations)
}

fn extract_zip_pending(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    strip_root: Option<&str>,
) -> Result<PendingModInstall, String> {
    let destinations = zip_entry_destinations(zip_path, game_root, target_dir, strip_root)?;
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &e.to_string())])
    })?;
    let mut pending = PendingModInstall::new(game_root);
    let extraction = (|| -> Result<(), String> {
        for (index, destination) in destinations {
            let mut entry = archive.by_index(index).map_err(|e| {
                crate::locale_tag::with_params(
                    "archive.zipEntryReadFailed",
                    &[("reason", &e.to_string())],
                )
            })?;
            pending.track_destination(&destination)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            clear_readonly(&destination)?;
            let mut output = File::create(&destination).map_err(|e| e.to_string())?;
            io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
            pending.files.push(path_relative(game_root, &destination));
        }
        Ok(())
    })();
    if let Err(error) = extraction {
        pending.rollback()?;
        return Err(error);
    }
    Ok(pending)
}

fn strip_zip_root(path: &Path, expected_root: &str) -> Option<PathBuf> {
    let path_components = path.components().collect::<Vec<_>>();
    let expected_components = Path::new(expected_root).components().collect::<Vec<_>>();
    if expected_components.is_empty() || path_components.is_empty() {
        return None;
    }
    let full_match = path_components.len() >= expected_components.len()
        && path_components
            .iter()
            .zip(expected_components.iter())
            .all(|(actual, expected)| {
                actual
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
            });
    let strip_count = if full_match {
        expected_components.len()
    } else if path_components[0]
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected_components[0].as_os_str().to_string_lossy())
    {
        1
    } else {
        return None;
    };
    Some(path_components.into_iter().skip(strip_count).collect())
}

#[cfg(test)]
fn copy_asset(
    asset_path: &Path,
    game_root: &Path,
    install_target: &str,
    file_name: &str,
) -> Result<Vec<String>, String> {
    copy_asset_pending(asset_path, game_root, install_target, file_name)
        .map(PendingModInstall::commit)
}

pub(crate) fn copy_asset_pending(
    asset_path: &Path,
    game_root: &Path,
    install_target: &str,
    file_name: &str,
) -> Result<PendingModInstall, String> {
    log::info!(
        "复制 MOD 资产：文件={}，目标目录={}，文件名={}",
        asset_path.display(),
        install_target,
        file_name
    );
    let relative = Path::new(install_target).join(file_name);
    let destination = safe_game_path(game_root, &relative.to_string_lossy())?;
    // Path spelling alone cannot detect hard links; reject before snapshots or permission changes.
    if destination.try_exists().map_err(|e| e.to_string())?
        && same_file::is_same_file(asset_path, &destination).map_err(|e| e.to_string())?
    {
        return Err("archive.sourceEqualsTarget".into());
    }
    let mut pending = PendingModInstall::new(game_root);
    pending.track_destination(&destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    clear_readonly(&destination)?;
    if let Err(error) = fs::copy(asset_path, &destination) {
        let restore_error = pending.rollback().err();
        let result = match restore_error {
            Some(restore_error) => crate::locale_tag::with_params(
                "archive.copyAndRestoreFailed",
                &[
                    ("error", &error.to_string()),
                    ("restore", &restore_error.to_string()),
                ],
            ),
            None => error.to_string(),
        };
        log::error!(
            "复制 MOD 资产失败：文件={}，原因={}",
            asset_path.display(),
            result
        );
        return Err(result);
    }
    let relative = path_relative(game_root, &destination);
    log::info!("MOD 资产复制完成：目标={}", relative);
    pending.files.push(relative);
    Ok(pending)
}

#[cfg(test)]
mod rollback_tests;

#[cfg(test)]
mod safety_tests;

#[cfg(test)]
mod layout_tests;
