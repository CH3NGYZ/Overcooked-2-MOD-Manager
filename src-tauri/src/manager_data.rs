//! Mutate the selected data root only after every manager process releases it.
//! A fixed bootstrap lock/journal coordinates reset and migration across roots.
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod handoff;
mod legacy;
mod migration;
mod notice;
mod storage;
#[cfg(test)]
mod tests;
mod webview_reset;

pub(crate) use notice::install_language;
pub(crate) use webview_reset::webview_reset_script;

const HELPER_FLAG: &str = "--oc2-clear-manager-data";
const BUSY: &str = "tools.clearAllConfigBusy";
const UNSAFE: &str = "tools.clearAllConfigUnsafe";
const LOCK_TIMEOUT: Duration = Duration::from_secs(120);
static RESETTING: AtomicBool = AtomicBool::new(false);

fn failed(reason: impl std::fmt::Display) -> String {
    crate::locale_tag::with_params(
        "tools.clearAllConfigFailed",
        &[("reason", &reason.to_string())],
    )
}

pub(crate) struct DataSession {
    // The OS closes this on process exit, after configuration/log writers stop.
    _lock: File,
    _legacy_lock: Option<File>,
    pub(crate) skip_legacy_migration: bool,
    pub(crate) completed_reset_token: Option<uuid::Uuid>,
    pub(crate) failure_diagnostics: Option<String>,
}

fn completed_reset_token(record: Option<&storage::ResetRecord>) -> Option<uuid::Uuid> {
    record
        .filter(|record| record.phase == storage::ResetPhase::Completed)
        .filter(|record| matches!(record.action, storage::DataAction::Clear))
        .map(|record| record.token)
}

/// Shared startup ownership preserves the existing single-instance plugin:
/// ordinary secondary launches still reach its callback instead of silently exiting.
pub(crate) fn initialize() -> Result<DataSession, String> {
    loop {
        // A pending migration may publish a different root before this retry.
        let paths = storage::ResetPaths::current()?;
        let lock = paths.open_lock()?;
        storage::wait_for_lock(&lock, true, LOCK_TIMEOUT)?;
        let legacy_lock = legacy::session(&paths)?;
        if storage::normalized(&storage::ResetPaths::current()?.root)
            != storage::normalized(&paths.root)
        {
            drop(lock);
            continue;
        }
        let record = match paths.record() {
            Ok(record) => record,
            Err(error) => {
                // A damaged coordination record must not repeatedly delete data
                // or strand the application before its diagnostic UI can start.
                if paths.parent.join("data-location.json").is_file() && !paths.root.is_dir() {
                    return Err("config.directoryUnavailable".into());
                }
                notice::show_unrecorded(&error, None);
                crate::user_paths::freeze_root(&crate::user_paths::root()?)?;
                return Ok(DataSession {
                    _lock: lock,
                    _legacy_lock: legacy_lock,
                    skip_legacy_migration: true,
                    completed_reset_token: None,
                    failure_diagnostics: Some(format!("无法读取管理器数据操作记录：{error}")),
                });
            }
        };
        if record.as_ref().is_some_and(storage::ResetRecord::pending)
            || legacy::needs_import(&paths)?
        {
            // A helper or this startup can win exclusive ownership; both re-read
            // the journal, so a second launch never deletes newly initialized data.
            drop(legacy_lock);
            drop(lock);
            let exclusive = paths.open_lock()?;
            storage::wait_for_lock(&exclusive, false, LOCK_TIMEOUT)?;
            legacy::import(&paths)?;
            if let Some(record) = paths.record()?.filter(storage::ResetRecord::pending) {
                let token = record.token;
                let result = match legacy::exclusive_for_request(&paths, &record) {
                    Ok(_legacy) => match handoff::wait_for_owner(&record) {
                        Ok(()) => paths.finish(record),
                        Err(error) => paths.mark_failed(record, &error),
                    },
                    Err(error) => Err(error),
                };
                if let Err(error) = result
                    && !paths
                        .record()?
                        .is_some_and(|record| record.token == token && !record.pending())
                {
                    return Err(error);
                }
            }
            drop(exclusive);
            continue;
        }
        let failure_diagnostics = record
            .as_ref()
            .filter(|record| record.phase == storage::ResetPhase::Failed)
            .map(|record| {
                if !record.reported {
                    notice::show_once(&paths, record);
                }
                notice::diagnostics(record)
            });
        // Only a completed clear deliberately leaves the selected root absent.
        if paths.parent.join("data-location.json").is_file()
            && !paths.root.is_dir()
            && !record.as_ref().is_some_and(|record| {
                record.phase == storage::ResetPhase::Completed
                    && matches!(record.action, storage::DataAction::Clear)
            })
        {
            return Err("config.directoryUnavailable".into());
        }
        // Preserve the pre-bootstrap path spelling in existing transaction roots.
        // Canonical Windows verbatim paths are only used for safety comparisons.
        crate::user_paths::freeze_root(&crate::user_paths::root()?)?;
        return Ok(DataSession {
            _lock: lock,
            _legacy_lock: legacy_lock,
            skip_legacy_migration: record.is_some(),
            completed_reset_token: completed_reset_token(record.as_ref()),
            failure_diagnostics,
        });
    }
}

/// This entry runs before Tauri, migration, logging and ConfigService initialize.
/// Its only argument is an opaque request UUID; paths always come from user_paths.
pub(crate) fn run_if_requested() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(HELPER_FLAG)) {
        return;
    }
    let result = (|| -> Result<bool, String> {
        let token = args
            .next()
            .and_then(|arg| arg.into_string().ok())
            .and_then(|arg| uuid::Uuid::parse_str(&arg).ok())
            .ok_or_else(|| failed("Invalid reset-helper arguments"))?;
        if args.next().is_some() {
            return Err(failed("Invalid reset-helper arguments"));
        }
        let paths = storage::ResetPaths::current()?;
        if let Err(error) = handoff::run_helper(&paths, token) {
            if let Ok(Some(record)) = paths.record()
                && record.token == token
                && record.phase == storage::ResetPhase::Failed
            {
                notice::show_once(&paths, &record);
            } else {
                eprintln!("Data operation helper stopped ({token}): {error}");
            }
            return Ok(false);
        }
        Ok(true)
    })();
    if let Err(error) = &result {
        notice::show_unrecorded(error, None);
    }
    std::process::exit(if matches!(result, Ok(true)) { 0 } else { 1 });
}

pub(crate) fn show_startup_error(error: &str) {
    notice::show_unrecorded(error, None);
}

struct ResetGuard;
impl Drop for ResetGuard {
    fn drop(&mut self) {
        RESETTING.store(false, Ordering::Release);
    }
}

fn validate_protected_paths(
    paths: &storage::ResetPaths,
    state: &crate::app_state::AppState,
    executable: &Path,
) -> Result<(), String> {
    paths.validate_root()?;
    paths.protect(executable)?;
    let config = state.config.load_or_default();
    if !config.game.path.trim().is_empty() {
        paths.protect(Path::new(&config.game.path))?;
    }
    if let Some(game) = state
        .game_state
        .lock()
        .map_err(|_| "lock.poisoned")?
        .as_ref()
        .and_then(|game| game.path.as_ref())
    {
        paths.protect(Path::new(game))?;
    }
    Ok(())
}

/// All locks, native-process preparation and shutdown stay on the blocking worker.
/// No caller-provided path is accepted, and failures return without exiting.
pub(crate) fn clear_and_exit(
    state: &crate::app_state::AppState,
    locale: Option<String>,
) -> Result<(), String> {
    change_and_exit(state, None, locale)
}

pub(crate) fn migrate_and_exit(
    state: &crate::app_state::AppState,
    parent: &Path,
    locale: Option<String>,
) -> Result<(), String> {
    change_and_exit(state, Some(parent), locale)
}

fn change_and_exit(
    state: &crate::app_state::AppState,
    parent: Option<&Path>,
    locale: Option<String>,
) -> Result<(), String> {
    RESETTING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| BUSY.to_string())?;
    let _reset = ResetGuard;
    // Share the updater's atomic exclusion so an update cannot begin mid-reset.
    let _updater = crate::archive::begin_app_update()?;
    let paths = storage::ResetPaths::current()?;
    let executable = std::env::current_exe().map_err(failed)?;
    validate_protected_paths(&paths, state, &executable)?;
    let action = match parent {
        Some(parent) => storage::DataAction::Migrate {
            source: paths.root.clone(),
            target: migration::prepare(&paths, parent, state, &executable)?,
        },
        None => storage::DataAction::Clear,
    };
    // Keep the established repository_cache -> operation order. Do not wait for
    // an installation/download to finish while a destructive confirmation is open.
    let _repositories = state.repository_cache.try_lock().map_err(|_| BUSY)?;
    {
        let _operation = state.operation.try_lock().map_err(|_| BUSY)?;
        crate::mod_changes::guard::ensure_recovered(state)?;
    }
    let mut helper = handoff::PreparedHelper::start(&paths, &executable)?;
    crate::level_sets::recompress::shutdown().map_err(|error| {
        log::error!("地图资源恢复未完成，保留管理器数据：{error}");
        "assetBundle.recoveryPending".to_string()
    })?;
    let _operation = state.operation.try_lock().map_err(|_| BUSY)?;
    crate::mod_changes::guard::ensure_recovered(state)?;
    if crate::mod_changes::transaction::has_pending(&paths.root.join("mod-change-transactions"))? {
        return Err("modChanges.recoveryRequired".into());
    }
    // Helper preparation/shutdown may take time. Re-read both game-path sources
    // under operation so a settings change cannot bypass the final handoff gate.
    validate_protected_paths(&paths, state, &executable)?;
    if let Some(parent) = parent {
        migration::prepare(&paths, parent, state, &executable)?;
    }
    let locale = locale
        .as_deref()
        .and_then(notice::supported_language)
        .map(str::to_owned);
    helper.commit(&paths, action, locale)?;
    log::info!("管理器数据操作已交接；退出后由原生辅助进程完成");
    log::logger().flush();
    std::process::exit(0)
}
