use crate::display_path::display_path;
use crate::models::RustLog;
use chrono::Utc;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[tauri::command]
pub fn read_rust_log() -> Result<RustLog, String> {
    read_log_file("rust.log")
}

#[tauri::command]
pub fn read_frontend_log() -> Result<RustLog, String> {
    read_log_file("frontend.log")
}

#[tauri::command]
pub fn read_crash_log() -> Result<RustLog, String> {
    read_log_file("crash.log")
}

/// 清空一份日志，并把清空后的内容回读给前端。
///
/// 回读而不是让前端自己再调一次 read：两次 IPC 之间日志插件可能又写进新行，
/// 界面就会显示「点了清空但内容还在」。一次调用拿到的就是那一刻的文件内容。
#[tauri::command]
pub fn clear_log(kind: String) -> Result<RustLog, String> {
    let file_name = log_file_name(&kind)?;
    let path = crate::user_paths::logs_dir()?.join(file_name);
    clear_log_at_path(&path)?;
    // 先回读再记这次清空：回读要给前端一个真正的空内容，而「日志被清空过」
    // 这件事必须留在文件里 —— 否则诊断时只看到日志从中间开始，却没有任何
    // 线索说明是谁在什么时候清的。
    let log = read_log_at_path(&path)?;
    log::info!("清空日志：文件={file_name}");
    Ok(log)
}

/// kind → 文件名。白名单，不接受调用方直接给文件名：
/// `logs_dir().join(任意字符串)` 里的 `..` 能逃出日志目录，而清空不可逆。
fn log_file_name(kind: &str) -> Result<&'static str, String> {
    match kind {
        "rust" => Ok("rust.log"),
        "frontend" => Ok("frontend.log"),
        "crash" => Ok("crash.log"),
        _ => Err("log.unknownKind".to_string()),
    }
}

fn read_log_file(file_name: &str) -> Result<RustLog, String> {
    let path = crate::user_paths::logs_dir()?.join(file_name);
    read_log_at_path(&path)
}

/// Reading diagnostics must never append diagnostics about the read itself.
/// Seek to the tail before reading so large logs do not allocate the full file.
fn read_log_at_path(path: &Path) -> Result<RustLog, String> {
    let content = match fs::File::open(path) {
        Ok(mut file) => {
            const MAX_LOG_BYTES: u64 = 512 * 1024;
            let length = file.metadata().map_err(|error| error.to_string())?.len();
            let start = length.saturating_sub(MAX_LOG_BYTES);
            file.seek(SeekFrom::Start(start))
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::with_capacity(length.min(MAX_LOG_BYTES) as usize);
            file.take(MAX_LOG_BYTES)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    Ok(RustLog {
        path: display_path(path),
        content,
    })
}

/// 把日志文件截断到 0 字节。
///
/// rust.log 正被日志插件以 append 模式持有：外部截断是安全的，插件的写入走
/// O_APPEND 语义，截断后自然从头写。唯一的副作用是插件内部的字节计数不会
/// 跟着重置，可能让下一次轮转提前发生一次 —— 不丢日志，只多一个归档文件。
/// crash.log / frontend.log 每次写入才打开，没有长期句柄。
fn clear_log_at_path(path: &Path) -> Result<(), String> {
    match OpenOptions::new().write(true).truncate(true).open(path) {
        Ok(_) => Ok(()),
        // 文件本来就不存在，等价于已经是空的
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn write_frontend_log(level: String, message: String) -> Result<(), String> {
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
    let level = level.to_ascii_lowercase();
    let path = crate::user_paths::logs_dir()?;
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.join("frontend.log"))
        .map_err(|error| error.to_string())?;
    writeln!(
        file,
        "[{}][{}] {}",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        level,
        message
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod log_tests;
