use super::{LOCK_TIMEOUT, UNSAFE, directory_prompt, legacy, storage::ResetPaths};
use crate::mod_changes::transaction;
use crate::user_paths::{self, DataRoot, DataRootError, DirectoryProblem, windows_data_root};
use directory_prompt::{Choice, Issue};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) fn select(location: &Path) -> Result<Option<DataRoot>, String> {
    loop {
        let (source, problem) = match user_paths::locate_root(location) {
            Ok(root) => return Ok(Some(root)),
            Err(DataRootError::Location(error)) => return Err(error),
            Err(DataRootError::Directory(source, problem)) => (source, problem),
        };
        let mut issue = match problem {
            DirectoryProblem::Unavailable => Issue::Unavailable,
            DirectoryProblem::Unsafe | DirectoryProblem::NotDirectory => Issue::Unsafe,
        };
        let mut shown = source.clone();
        loop {
            match directory_prompt::choose(Some(&shown), issue) {
                Choice::Cancel => return Ok(None),
                Choice::Retry => break,
                Choice::Directory(selected) => {
                    match use_existing(location, &source, &selected, user_paths::default_root)? {
                        None => break,
                        Some(next) => {
                            issue = next;
                            shown = if matches!(next, Issue::RecoveryBlocked) {
                                source.clone()
                            } else {
                                selected
                            };
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn retry(path: &Path) -> bool {
    matches!(
        directory_prompt::choose(Some(path), Issue::Unavailable),
        Choice::Retry
    )
}

pub(super) fn is_unavailable(error: &str) -> bool {
    use winapi::shared::winerror::{
        ERROR_ACCESS_DENIED, ERROR_DEVICE_NOT_CONNECTED, ERROR_NOT_READY, ERROR_PATH_NOT_FOUND,
        ERROR_WRITE_PROTECT,
    };
    super::notice::diagnostic_os_code(error).is_some_and(|code| {
        matches!(
            code as u32,
            ERROR_ACCESS_DENIED
                | ERROR_DEVICE_NOT_CONNECTED
                | ERROR_NOT_READY
                | ERROR_PATH_NOT_FOUND
                | ERROR_WRITE_PROTECT
        )
    })
}

pub(super) fn probe_writable(path: &Path) -> io::Result<()> {
    let writable = if path.try_exists()? {
        path
    } else {
        path.parent().ok_or(io::ErrorKind::InvalidInput)?
    };
    tempfile::NamedTempFile::new_in(writable)?.close()
}

fn use_existing(
    location: &Path,
    source: &Path,
    selected: &Path,
    default: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<Option<Issue>, String> {
    if windows_data_root(selected, false, false).is_err() {
        return Ok(Some(Issue::InvalidSelection));
    }
    let control = location.parent().ok_or(UNSAFE)?;
    let paths = ResetPaths::with_control(selected, control)?;
    let lock = paths.open_lock()?;
    super::storage::wait_for_lock(&lock, false, LOCK_TIMEOUT)?;

    let problem = match user_paths::locate_root_with_default(location, default) {
        Err(DataRootError::Directory(path, problem)) if path == source => problem,
        Ok(_) | Err(DataRootError::Directory(_, _)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if matches!(problem, DirectoryProblem::Unavailable) {
        return Ok(None);
    }
    if !paths
        .record()
        .is_ok_and(|record| record.is_none_or(|record| !record.pending()))
    {
        return Ok(Some(Issue::RecoveryBlocked));
    }
    if !check_selection(&paths, selected) {
        return Ok(Some(Issue::InvalidSelection));
    }
    if !can_switch(problem, source, selected) {
        return Ok(Some(Issue::RecoveryBlocked));
    }

    let _legacy_lock = legacy::exclusive(&paths)?;
    if legacy::needs_import(&paths)? {
        return Ok(Some(Issue::RecoveryBlocked));
    }

    user_paths::write_location_at(location, selected)?;
    Ok(None)
}

fn can_switch(problem: DirectoryProblem, source: &Path, selected: &Path) -> bool {
    if problem != DirectoryProblem::NotDirectory {
        let Ok((previous, _)) = windows_data_root(source, true, false) else {
            return false;
        };
        if !transaction::has_retained_material(&previous.join(transaction::DIRECTORY_NAME))
            .is_ok_and(|retained| !retained)
        {
            return false;
        }
    }
    transaction::list_recoveries(&selected.join(transaction::DIRECTORY_NAME), None)
        .is_ok_and(|entries| entries.iter().all(|entry| entry.scope_known))
}

fn check_selection(paths: &ResetPaths, selected: &Path) -> bool {
    if windows_data_root(selected, false, false).is_err() {
        return false;
    }
    if paths.protect(&paths.parent).is_err()
        || !std::env::current_exe().is_ok_and(|exe| paths.protect(&exe).is_ok())
    {
        return false;
    }
    let config = crate::config::existing_config_path(selected)
        .ok()
        .flatten()
        .filter(|path| super::storage::reject_link_if_present(path).is_ok() && path.is_file())
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str::<crate::models::AppConfig>(&text).ok());
    let Some(config) = config else {
        return false;
    };
    if !config.game.path.trim().is_empty() && paths.protect(Path::new(&config.game.path)).is_err() {
        return false;
    }
    !crate::game::detect_game_root(&config.game).is_some_and(|game| paths.protect(&game).is_err())
        && probe_writable(selected).is_ok()
}

#[cfg(test)]
mod tests;
