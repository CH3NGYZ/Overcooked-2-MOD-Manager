//! Mutate the selected data root only after every manager process releases it.
//! A fixed bootstrap lock/journal coordinates reset and migration across roots.
use crate::app_state::AppState;
use crate::file_check::log_path;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod handoff;
mod legacy;
mod migration;
mod notice;
mod receipt;
mod storage;
#[cfg(test)]
mod tests;
mod webview_reset;

pub(crate) use notice::install_language;
#[cfg(any(windows, target_os = "macos"))]
pub(crate) use notice::language as native_notice_language;
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
    pub(crate) startup_diagnostics: String,
    pub(crate) completed_operation_diagnostics: Option<String>,
    pub(crate) failure_diagnostics: Option<String>,
    pub(crate) helper_failure_diagnostics: Option<String>,
}

fn startup_diagnostics(
    paths: &storage::ResetPaths,
    pointer: bool,
    root_dir: Option<bool>,
    journal: &str,
) -> String {
    let source = if pointer {
        "固定定位文件"
    } else {
        "系统默认位置"
    };
    let root_status = match root_dir {
        Some(true) => "目录存在",
        Some(false) => "目录不存在或不可读",
        None => "未检查",
    };
    format!(
        "[data] 启动数据定位：来源={source}，定位文件={:?}，定位文件存在={}，数据根={:?}，根目录状态={root_status}，操作记录={journal}",
        log_path(&paths.parent.join("data-location.json")),
        pointer,
        log_path(&paths.root)
    )
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
                let pointer = paths.parent.join("data-location.json").is_file();
                let root_dir = pointer.then(|| paths.root.is_dir());
                if root_dir == Some(false) {
                    return Err("config.directoryUnavailable".into());
                }
                notice::show_unrecorded(&error, None);
                crate::user_paths::freeze_root(&crate::user_paths::root()?)?;
                return Ok(DataSession {
                    _lock: lock,
                    _legacy_lock: legacy_lock,
                    skip_legacy_migration: true,
                    completed_reset_token: None,
                    startup_diagnostics: startup_diagnostics(&paths, pointer, root_dir, "不可读"),
                    completed_operation_diagnostics: None,
                    failure_diagnostics: Some(format!(
                        "[data] 无法读取管理器数据操作记录：文件={:?}，原因={}",
                        log_path(&paths.journal()),
                        notice::diagnostic_reason(&error)
                    )),
                    helper_failure_diagnostics: receipt::diagnostics(&paths.parent, None),
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
                notice::diagnostics(record, &paths.root)
            });
        // Only a completed clear deliberately leaves the selected root absent.
        let pointer = paths.parent.join("data-location.json").is_file();
        let root_dir = pointer.then(|| paths.root.is_dir());
        if root_dir == Some(false)
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
            startup_diagnostics: startup_diagnostics(
                &paths,
                pointer,
                root_dir,
                if record.is_some() {
                    "存在"
                } else {
                    "不存在"
                },
            ),
            completed_operation_diagnostics: record
                .as_ref()
                .filter(|record| record.phase == storage::ResetPhase::Completed)
                .map(|record| notice::diagnostics(record, &paths.root)),
            failure_diagnostics,
            helper_failure_diagnostics: receipt::diagnostics(&paths.parent, record.as_ref()),
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
        let paths = storage::ResetPaths::current().inspect_err(|error| {
            if let Ok(location) = crate::user_paths::location_file()
                && let Some(control) = location.parent()
            {
                receipt::record_failure(control, token, receipt::Stage::LocateDataRoot, error);
            }
        })?;
        let mut stage = receipt::Stage::ValidateDataRoot;
        if let Err(error) = handoff::run_helper(&paths, token, &mut stage) {
            if let Ok(Some(record)) = paths.record()
                && record.token == token
                && record.phase == storage::ResetPhase::Failed
            {
                notice::show_once(&paths, &record);
            } else {
                receipt::record_failure(&paths.parent, token, stage, &error);
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
    check_step("数据根安全校验", paths.validate_root())?;
    check_step("检查数据根与管理器程序是否交叠", paths.protect(executable))?;
    let config = state.config.load_or_default();
    if !config.game.path.trim().is_empty() {
        check_step(
            "检查数据根与已配置游戏目录是否交叠",
            paths.protect(Path::new(&config.game.path)),
        )?;
    }
    if let Some(game) = state
        .game_state
        .lock()
        .map_err(|_| "lock.poisoned")?
        .as_ref()
        .and_then(|game| game.path.as_ref())
    {
        check_step(
            "检查数据根与已探测游戏目录是否交叠",
            paths.protect(Path::new(game)),
        )?;
    }
    Ok(())
}

fn check_step<Value>(step: &str, result: Result<Value, String>) -> Result<Value, String> {
    result.inspect_err(|error| {
        log::warn!(
            "[data] 管理器数据操作停止：步骤={step}，原因={}",
            notice::diagnostic_reason(error)
        );
    })
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
    check_step(
        "获取数据操作门禁",
        RESETTING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BUSY.to_string()),
    )?;
    let _reset = ResetGuard;
    // Share the updater's atomic exclusion so an update cannot begin mid-reset.
    let _updater = check_step("获取更新互斥门禁", crate::archive::begin_app_update())?;
    let paths = check_step("定位当前数据根", storage::ResetPaths::current())?;
    let executable = check_step(
        "定位当前管理器程序",
        std::env::current_exe().map_err(failed),
    )?;
    log::info!(
        "[data] 准备{}：数据根={:?}，目标父目录={:?}",
        if parent.is_some() {
            "迁移管理器数据"
        } else {
            "清除管理器数据"
        },
        log_path(&paths.root),
        parent.filter(|path| path.is_absolute()).map(log_path)
    );
    validate_protected_paths(&paths, state, &executable)?;
    let action = match parent {
        Some(parent) => storage::DataAction::Migrate {
            source: paths.root.clone(),
            target: check_step(
                "预先验证迁移目标",
                migration::prepare(&paths, parent, state, &executable),
            )?,
        },
        None => storage::DataAction::Clear,
    };
    // Keep the established repository_cache -> operation order. Do not wait for
    // an installation/download to finish while a destructive confirmation is open.
    let _repositories = check_step(
        "获取仓库操作门禁",
        state
            .repository_cache
            .try_lock()
            .map_err(|_| BUSY.to_owned()),
    )?;
    {
        let _operation = check_step(
            "获取数据写入门禁",
            state.operation.try_lock().map_err(|_| BUSY.to_owned()),
        )?;
        check_data_handoff_gate(state)?;
    }
    let mut helper = check_step(
        "等待原生辅助进程就绪",
        handoff::PreparedHelper::start(&paths, &executable),
    )?;
    let compression = crate::level_sets::recompress::quiesce_for_exit();
    let _operation = check_step(
        "交接前重新获取数据写入门禁",
        state.operation.try_lock().map_err(|_| BUSY.to_owned()),
    )?;
    check_data_handoff_gate(state)?;
    compression.restore(state).map_err(|error| {
        log::error!(
            "[data] 地图资源恢复未完成，保留管理器数据：原因={}",
            notice::diagnostic_reason(&error)
        );
        "assetBundle.recoveryPending".to_string()
    })?;
    // Helper preparation/shutdown may take time. Re-read both game-path sources
    // under operation so a settings change cannot bypass the final handoff gate.
    validate_protected_paths(&paths, state, &executable)?;
    if let Some(parent) = parent {
        check_step(
            "交接前复核迁移目标",
            migration::prepare(&paths, parent, state, &executable),
        )?;
    }
    let locale = locale
        .as_deref()
        .and_then(notice::supported_language)
        .map(str::to_owned);
    check_step(
        "写入待处理请求并交接",
        helper.commit(&paths, action, locale),
    )?;
    log::logger().flush();
    std::process::exit(0)
}

fn check_data_handoff_gate(state: &AppState) -> Result<(), String> {
    check_step(
        "恢复既有安装事务",
        crate::mod_changes::guard::ensure_recovered(state),
    )?;
    let retained = crate::mod_changes::transaction::has_retained_material(
        &state.config.mod_change_transactions_dir(),
    )
    .map_err(|_| "modChanges.recoveryRequired")?;
    if retained {
        return Err("modChanges.recoveryRequired".into());
    }
    Ok(())
}
