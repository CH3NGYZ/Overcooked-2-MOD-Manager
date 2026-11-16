//! Reset the fixed user-data root only after every manager process releases it.
//! The lock and reset journal live beside the root so deleting logs/configuration
//! cannot race a surviving writer or resurrect an installation-local migration.
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod handoff;
mod storage;
#[cfg(test)]
mod tests;
mod webview_reset;

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
    pub(crate) skip_legacy_migration: bool,
    pub(crate) completed_reset_token: Option<uuid::Uuid>,
}

fn completed_reset_token(record: Option<&storage::ResetRecord>) -> Option<uuid::Uuid> {
    record
        .filter(|record| record.phase == storage::ResetPhase::Completed)
        .map(|record| record.token)
}

/// Shared startup ownership preserves the existing single-instance plugin:
/// ordinary secondary launches still reach its callback instead of silently exiting.
pub(crate) fn initialize() -> Result<DataSession, String> {
    let paths = storage::ResetPaths::new(&crate::user_paths::root()?)?;
    loop {
        let lock = paths.open_lock()?;
        storage::wait_for_lock(&lock, true, LOCK_TIMEOUT)?;
        let record = match paths.record() {
            Ok(record) => record,
            Err(error) => {
                // A damaged coordination record must not repeatedly delete data
                // or strand the application before its diagnostic UI can start.
                handoff::show_error(&error);
                return Ok(DataSession {
                    _lock: lock,
                    skip_legacy_migration: true,
                    completed_reset_token: None,
                });
            }
        };
        if record.as_ref().is_some_and(storage::ResetRecord::pending) {
            // A helper or this startup can win exclusive ownership; both re-read
            // the journal, so a second launch never deletes newly initialized data.
            drop(lock);
            let exclusive = paths.open_lock()?;
            storage::wait_for_lock(&exclusive, false, LOCK_TIMEOUT)?;
            if let Some(record) = paths.record()?.filter(storage::ResetRecord::pending) {
                if let Err(error) = handoff::wait_for_owner(&record) {
                    paths.mark_failed(record, &error)?;
                } else if let Err(error) = paths.finish(record) {
                    // finish persists a failed state, so retrying startup does not
                    // repeat a partly failed deletion forever.
                    log::error!("{error}");
                    // Persisting Failed can itself fail (permissions/full disk).
                    // Never loop on an unchanged destructive request in that case.
                    if !paths.record()?.is_some_and(|record| !record.pending()) {
                        return Err(error);
                    }
                }
            }
            drop(exclusive);
            continue;
        }
        if let Some(record) = &record {
            if record.phase == storage::ResetPhase::Failed && !record.reported {
                handoff::show_error(record.error.as_deref().unwrap_or("Data reset failed"));
                let mut reported = record.clone();
                reported.reported = true;
                paths.write_record(&reported)?;
            }
        }
        return Ok(DataSession {
            _lock: lock,
            skip_legacy_migration: record.is_some(),
            completed_reset_token: completed_reset_token(record.as_ref()),
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
    let result = (|| {
        let token = args
            .next()
            .and_then(|arg| arg.into_string().ok())
            .and_then(|arg| uuid::Uuid::parse_str(&arg).ok())
            .ok_or_else(|| failed("Invalid reset-helper arguments"))?;
        if args.next().is_some() {
            return Err(failed("Invalid reset-helper arguments"));
        }
        let paths = storage::ResetPaths::new(&crate::user_paths::root()?)?;
        handoff::run_helper(&paths, token)
    })();
    if let Err(error) = &result {
        handoff::show_error(error);
    }
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

pub(crate) fn show_startup_error(error: &str) {
    handoff::show_error(error);
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
pub(crate) fn clear_and_exit(state: &crate::app_state::AppState) -> Result<(), String> {
    RESETTING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| BUSY.to_string())?;
    let _reset = ResetGuard;
    // Share the updater's atomic exclusion so an update cannot begin mid-reset.
    let _updater = crate::archive::begin_app_update()?;
    let paths = storage::ResetPaths::new(&crate::user_paths::root()?)?;
    let executable = std::env::current_exe().map_err(failed)?;
    validate_protected_paths(&paths, state, &executable)?;
    // Keep the established repository_cache -> operation order. Do not wait for
    // an installation/download to finish while a destructive confirmation is open.
    let _repositories = state.repository_cache.try_lock().map_err(|_| BUSY)?;
    {
        let _operation = state.operation.try_lock().map_err(|_| BUSY)?;
    }
    let mut helper = handoff::PreparedHelper::start(&paths, &executable)?;
    crate::level_sets::recompress::shutdown().map_err(|error| {
        log::error!("地图资源恢复未完成，保留管理器数据：{error}");
        "assetBundle.recoveryPending".to_string()
    })?;
    let _operation = state.operation.try_lock().map_err(|_| BUSY)?;
    // Helper preparation/shutdown may take time. Re-read both game-path sources
    // under operation so a settings change cannot bypass the final handoff gate.
    validate_protected_paths(&paths, state, &executable)?;
    helper.commit(&paths)?;
    log::info!("管理器数据清理已交接；退出后由原生辅助进程清理固定数据根");
    log::logger().flush();
    std::process::exit(0)
}
