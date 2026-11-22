use std::{collections::HashSet, path::PathBuf, sync::OnceLock};
use tauri::{Runtime, ipc::Invoke};

static COMMANDS: OnceLock<HashSet<&'static str>> = OnceLock::new();

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileDropResult {
    UpdateGate,
    NoConsumer,
    NoPath,
    Busy,
    UnsupportedType,
    Dispatched,
    ListenerFailed,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStage {
    Idle,
    Required,
    Installing,
    Handoff,
    Manual,
    BackgroundRequired,
    Downloading,
    StageFailed,
    Ready,
    Applying,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DropFileType {
    Zip,
    Dll,
    Other,
    None,
}

pub(crate) fn trace_native_drop(paths: &[PathBuf]) {
    let file_type = match paths.first() {
        None => DropFileType::None,
        Some(path) => match path.extension() {
            Some(ext) if ext.eq_ignore_ascii_case("zip") => DropFileType::Zip,
            Some(ext) if ext.eq_ignore_ascii_case("dll") => DropFileType::Dll,
            _ => DropFileType::Other,
        },
    };
    log::debug!(
        "[drop] 原生主窗口收到拖放：type={file_type:?}，count={}",
        paths.len()
    );
}

fn command_names(paths: &'static [&'static str]) -> HashSet<&'static str> {
    paths
        .iter()
        .filter_map(|path| path.rsplit("::").next().map(str::trim))
        .collect()
}

pub(super) fn command_name(value: &str) -> Option<&'static str> {
    COMMANDS.get()?.get(value).copied()
}

pub(crate) fn trace_ipc<R: Runtime>(
    handler: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static,
    commands: &'static [&'static str],
) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    let registered = COMMANDS.get_or_init(|| command_names(commands));
    move |invoke| {
        let command = invoke.message.command();
        if !matches!(
            command,
            "list_log_files"
                | "read_log"
                | "write_frontend_log"
                | "record_ipc_failure"
                | "record_file_drop"
                | "get_level_compression_progress"
                | "read_diychef_preview_asset"
                | "get_diychef_preview_fingerprint"
                | "diychef_thumbnail_cache"
        ) {
            let name = registered.get(command).copied().unwrap_or("unregistered");
            log::debug!("[ipc] 收到命令：command={name}");
        }
        handler(invoke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_only_accepts_registered_command_names() {
        let names = command_names(&[
            "commands::mod_settings::save_mod_config",
            "settings :: get_config",
        ]);
        assert!(names.contains("save_mod_config"));
        assert!(names.contains("get_config"));
        assert!(!names.contains("unregistered_command"));
        assert!(!names.contains("ghp_diagnosticFixture"));
    }

    #[test]
    fn native_drop_logs_category_and_count_without_path() {
        let capture = crate::http_diagnostics::capture_test_logs(
            "app_lib::commands::logs::audit",
            "原生主窗口收到拖放",
        );
        trace_native_drop(&[
            PathBuf::from("private-fixture/Map.ZIP"),
            PathBuf::from("b.dll"),
        ]);
        let messages = capture.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("type=Zip，count=2"));
        assert!(!messages[0].contains("private-fixture"));
    }
}
