use super::paths::normalize_path;
use std::path::Path;
use sysinfo::{Process, ProcessRefreshKind, ProcessStatus, System, UpdateKind};

pub fn is_game_running(game: &Path) -> bool {
    let running = is_game_running_with_system(game, &mut System::new());
    log::trace!("游戏进程检查：运行中={}", running);
    running
}

/// Reuse process metadata in the observer without logging unchanged checks.
/// File operations still call `is_game_running` with a fresh process snapshot.
pub(crate) fn is_game_running_with_system(game: &Path, system: &mut System) -> bool {
    system.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    let game = normalize_path(game);
    system
        .processes()
        .values()
        .any(|process| process_looks_running(&game, process))
}

/// 当前被判定成「游戏」的进程，格式 `名字(PID)`，仅供诊断日志使用。
///
/// 误判时最需要知道的正是「到底是哪个进程」，而它只在这类低频路径
/// （停止失败、用户手动重算）里取，不进入每秒一次的观察器热路径。
pub(crate) fn running_processes(game: &Path) -> Vec<String> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    let game = normalize_path(game);
    system
        .processes()
        .values()
        .filter(|process| process_looks_running(&game, process))
        .map(|process| format!("{}({})", process.name(), process.pid()))
        .collect()
}

/// 活着、且就是这款游戏的进程才算数。
///
/// 僵尸进程（已退出、父进程还没回收）仍留在进程表里，名字和命令行都还在，
/// 但 `exe()` 取不到 —— 正好落进下面「名字像游戏、拿不到路径」的兜底分支，
/// 于是游戏退出后永远显示运行中。macOS 上这类残留比 Windows 常见得多。
pub(super) fn process_looks_running(game: &Path, process: &Process) -> bool {
    if matches!(
        process.status(),
        ProcessStatus::Zombie | ProcessStatus::Dead
    ) {
        return false;
    }
    process_matches_game(game, process.name(), process.exe(), process.cmd())
}

/// Wine / CrossOver 的常驻宿主进程：容器开着它们就在，与游戏是否运行无关。
///
/// macOS 上它们会长期带着「这次容器里启动过的程序路径」存活（`winemenubuilder`
/// 生成 macOS 快捷方式时尤其容易卡住不退），若把名字带 wine 的进程一律当成
/// 游戏，游戏退出后仍会报告运行中 —— 按钮停在「停止游戏」，所有要求游戏已
/// 停止的操作全部被拒。Windows 没有这层宿主进程，所以只出现在 Mac。
///
/// 不包含 `wine` / `wine64-preloader` / `cxwinehelper`：CrossOver 里真正承载
/// 游戏的那个进程就是它们之一，去掉会把「游戏确实在跑」也判成没跑。
const WINE_HOST_PROCESSES: &[&str] = &[
    "wineserver",
    "wineboot",
    "winedevice",
    "winedbg",
    "winemenubuilder",
    "services.exe",
    "explorer.exe",
    "plugplay.exe",
    "svchost.exe",
    "rpcss.exe",
    "conhost.exe",
    "start.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "steamservice.exe",
];

fn is_wine_host_process(name: &str) -> bool {
    WINE_HOST_PROCESSES.iter().any(|host| name.contains(host))
}

fn process_matches_game(
    game: &Path,
    name: &str,
    executable: Option<&Path>,
    args: &[String],
) -> bool {
    let name = name.to_ascii_lowercase();
    if is_wine_host_process(&name) {
        return false;
    }
    let game_process = name.starts_with("overcooked");
    let wine_process = name.contains("wine") || name.contains("cxwinehelper");
    if !game_process && !wine_process {
        return false;
    }
    if let Some(executable) = executable {
        let executable = normalize_path(executable);
        // 只认本项目会出现的主程序：Windows 的 exe 与 macOS 应用包内可执行文件。
        // Linux 原生的 `Overcooked2.x86_64` 随发布目标一起移除。
        if normalize_path(&game.join("Overcooked2.exe")) == executable
            || executable.starts_with(game.join("Overcooked2.app/Contents/MacOS"))
        {
            return true;
        }
        if game_process
            && !wine_process
            && !executable
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("wine")
        {
            return false;
        }
    }
    // Wine may report the interpreter instead of the Windows game image.
    // If process metadata is inaccessible, fail closed for a named game process.
    game_process && executable.is_none()
        || args
            .iter()
            .any(|arg| arg.to_ascii_lowercase().contains("overcooked2.exe"))
}

/// Check if CrossOver is running (macOS only)
#[cfg(target_os = "macos")]
pub fn is_crossover_running() -> bool {
    let mut system = System::new();
    system
        .refresh_processes_specifics(ProcessRefreshKind::new().with_exe(UpdateKind::OnlyIfNotSet));
    let running = system.processes().values().any(|process| {
        let name = process.name();
        name.contains("CrossOver") || name.contains("wineserver") || name.contains("wine")
    });
    log::trace!("CrossOver 运行检查：运行中={}", running);
    running
}

#[cfg(not(target_os = "macos"))]
pub fn is_crossover_running() -> bool {
    false
}

pub fn ensure_game_stopped(game: &Path) -> Result<(), String> {
    if is_game_running(game) {
        log::warn!("文件操作被拒绝：游戏仍在运行");
        Err("game.running".into())
    } else {
        log::debug!("确认游戏已停止，可以执行文件操作");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
