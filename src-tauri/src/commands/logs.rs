use crate::models::{LogFileEntry, RustLog};
use chrono::Utc;
use history::{History, MAX_RUST_BYTES};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri_plugin_log::{Target, TargetKind, fern};

mod files;
mod history;

static HISTORY: OnceLock<Mutex<History>> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogKind {
    Rust,
    Frontend,
    Crash,
}

impl LogKind {
    const ALL: [Self; 3] = [Self::Rust, Self::Frontend, Self::Crash];

    fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Frontend => "frontend",
            Self::Crash => "crash",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| "log.unknownKind".into())
    }
}

fn unavailable() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "Unknown or replaced log file")
}

fn file_error(error: io::Error) -> String {
    if error.kind() == io::ErrorKind::NotFound {
        "log.fileUnavailable".into()
    } else {
        // The caller must no longer be holding HISTORY before logging again.
        log::warn!("Log operation failed: {error}");
        "log.fileOperationFailed".into()
    }
}

pub(crate) fn start(directory: &Path) -> io::Result<Target> {
    let history = History::start(directory, MAX_RUST_BYTES)?;
    HISTORY
        .set(Mutex::new(history))
        .map_err(|_| io::Error::other("Log history already initialized"))?;
    // Keep the plugin's formatting, stdout and filters. Its built-in rotator
    // recycles the active path, whereas a selected history entry must keep its identity.
    let writer = fern::Output::call(|record| {
        let line = format!("{}\n", record.args());
        if let Err(error) = with_history(|history| history.append(LogKind::Rust, line.as_bytes())) {
            eprintln!("Could not write Rust log: {error}");
        }
    });
    Ok(Target::new(TargetKind::Dispatch(
        fern::Dispatch::new().chain(writer),
    )))
}

fn with_history<T>(action: impl FnOnce(&mut History) -> io::Result<T>) -> io::Result<T> {
    let mut history = HISTORY
        .get()
        .ok_or_else(unavailable)?
        .lock()
        .map_err(|_| io::Error::other("Log history lock poisoned"))?;
    action(&mut history)
}

async fn log_operation<T: Send + 'static>(
    kind: String,
    action: impl FnOnce(&mut History, LogKind) -> io::Result<T> + Send + 'static,
) -> Result<T, String> {
    let kind = LogKind::parse(&kind)?;
    tauri::async_runtime::spawn_blocking(move || {
        with_history(|history| action(history, kind)).map_err(file_error)
    })
    .await
    .map_err(|_| "log.fileOperationFailed".to_owned())?
}

#[tauri::command]
pub async fn list_log_files(kind: String) -> Result<Vec<LogFileEntry>, String> {
    log_operation(kind, History::list).await
}

#[tauri::command]
pub async fn read_log(kind: String, id: String) -> Result<RustLog, String> {
    log_operation(kind, move |history, kind| history.read(kind, &id)).await
}

#[tauri::command]
pub async fn clear_log(kind: String, id: String) -> Result<RustLog, String> {
    let log = log_operation(kind, move |history, kind| history.clear(kind, &id)).await?;
    // Read the truncated file under the writer lock before writing this audit line.
    log::info!("清空日志：文件={}", log.path);
    Ok(log)
}

#[tauri::command]
pub async fn copy_log_file(kind: String, id: String) -> Result<(), String> {
    let kind = LogKind::parse(&kind)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = with_history(|history| history.selected_path(kind, &id)).map_err(file_error)?;
        copy_to_clipboard(&path).map_err(file_error)
    })
    .await
    .map_err(|_| "log.fileOperationFailed".to_owned())?
}

fn copy_to_clipboard(path: &Path) -> io::Result<()> {
    // Native clipboard libraries may themselves log when the clipboard is busy.
    let mut clipboard = arboard::Clipboard::new().map_err(io::Error::other)?;
    clipboard.set().file_list(&[path]).map_err(io::Error::other)
}

#[tauri::command]
pub async fn write_frontend_log(level: String, message: String) -> Result<(), String> {
    log::trace!("写入前端日志：等级={}，长度={}", level, message.len());
    const MAX_FRONTEND_LOG_BYTES: usize = 64 * 1024;
    let message = if message.len() > MAX_FRONTEND_LOG_BYTES {
        crate::locale_tag::with_params(
            "log.frontendTruncated",
            &[(
                "text",
                &&message[..message.floor_char_boundary(MAX_FRONTEND_LOG_BYTES)].to_string(),
            )],
        )
    } else {
        message
    };
    let line = format!(
        "[{}][{}] {message}\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        level.to_ascii_lowercase()
    );
    log_operation("frontend".into(), move |history, kind| {
        history.append(kind, line.as_bytes())
    })
    .await
}

pub(crate) fn install_panic_logger() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        previous(panic);
        let line = format!(
            "[{}][panic] {panic}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );
        // A panic could be unwinding the thread currently holding HISTORY.
        if let Some(mut history) = HISTORY.get().and_then(|history| history.try_lock().ok())
            && history.append(LogKind::Crash, line.as_bytes()).is_ok()
        {
            return;
        }
        let fallback = std::env::temp_dir().join("oc2-modmanager-crash.log");
        if let Err(error) = append_fallback(&fallback, line.as_bytes()) {
            eprintln!("panic 日志写入失败：{}（{error}）", fallback.display());
        }
    }));
}

fn append_fallback(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match OpenOptions::new().create_new(true).append(true).open(path) {
        Ok(mut file) => file.write_all(bytes),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let mut options = OpenOptions::new();
            options.append(true);
            files::open_plain_file(path, options)?.write_all(bytes)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod log_tests;
