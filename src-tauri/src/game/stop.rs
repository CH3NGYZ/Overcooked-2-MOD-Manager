use super::paths::normalize_path;
use super::process::{is_game_running, process_looks_running, running_processes};
use std::path::Path;
use sysinfo::{ProcessRefreshKind, System, UpdateKind};

/// 结束正在运行的《胡闹厨房2》进程。
///
/// Steam 没有「停止游戏」的 URL 协议（只有 `steam://rungameid/<appid>` 用于启动），
/// 所以「停止」只能由管理器结束游戏进程。
///
/// 流程是**先温和、再强制**：
///   1. Windows 上给游戏主窗口投递 `WM_CLOSE`，等同于用户点窗口右上角的 ×，
///      为游戏自行退出、保存进度和卸载框架留出机会，投递成功不证明这些已完成；
///   2. 至少有一个窗口投递成功时最多等 [`GRACEFUL_EXIT_TIMEOUT`]；一个都没投递
///      成功时也先等 [`GRACEFUL_GRACE_TIMEOUT`] 短宽限（消息可能已经入队）。
///      短宽限只属于 Windows：macOS 上没有窗口消息可投递，保持旧行为直接强杀；
///   3. 仍未退出才 `kill()`（Windows 上是 TerminateProcess，Unix 上是 SIGKILL）。
///
/// 为什么必须两步：直接 `kill()` 会跳过关卡存档写入、Steam 云同步钩子与
/// BepInEx 的 Chainloader 卸载（Harmony 逐条 unpatch），玩家正在打的那一关
/// 进度可能丢失、MOD 对游戏逻辑的改写也来不及还原。而只发 WM_CLOSE 又不可靠 ——
/// 游戏卡在无响应状态时窗口消息根本不会被处理，此时只能强制结束。
///
/// 每一步都写日志（投递结果、等待时长、放弃原因）：这条路径曾经是「找不到窗口
/// 就静默强杀」，用户只能看到游戏被直接关掉，无法判断卡在哪一步。
pub fn stop_game(game: &Path) -> Result<usize, String> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    let normalized = normalize_path(game);
    // Process 不实现 Clone，只保留 PID；kill 时从 system 里按 PID 取回。
    let targets = system
        .processes()
        .iter()
        .filter(|(_, process)| process_looks_running(&normalized, process))
        .map(|(pid, _)| *pid)
        .collect::<Vec<_>>();
    if targets.is_empty() {
        log::info!("停止游戏：当前没有运行中的厨房进程");
        return Ok(0);
    }
    log::info!("停止游戏：发现运行中的进程，数量={}", targets.len());

    // 第一步：温和关闭
    let report = request_graceful_close(&targets);
    let last_error = match report.last_error {
        Some(code) => code.to_string(),
        None => "无".to_string(),
    };
    log::info!(
        "停止游戏：温和关闭投递结果：目标进程 {} 个，顶层窗口 {} 个（可见 {} 个），投递成功 {} 个、失败 {} 个，最后错误码={}",
        report.targets,
        report.windows,
        report.visible_windows,
        report.posted,
        report.failed,
        last_error
    );
    if report.last_error == Some(ERROR_ACCESS_DENIED) {
        log::warn!(
            "停止游戏：WM_CLOSE 被系统拒绝（ERROR_ACCESS_DENIED）：游戏进程权限高于管理器，无法温和关闭；请用相同的权限级别（都以管理员或都不提权）启动管理器与游戏"
        );
    }

    let plan = graceful_wait_plan(&report);
    let grace_timeout = plan.timeout();
    match plan {
        GracefulWait::Full => log::info!(
            "停止游戏：已投递 {} 个关闭请求，最多等待 {:.1} 秒让游戏自行退出",
            report.posted,
            grace_timeout.as_secs_f64()
        ),
        GracefulWait::ShortGrace => log::info!(
            "停止游戏：没有投递成功任何关闭请求，先等 {:.1} 秒宽限再强制结束（留给已经入队的 WM_CLOSE）",
            grace_timeout.as_secs_f64()
        ),
        GracefulWait::Immediate => {
            log::info!("停止游戏：当前平台不投递窗口关闭消息，直接进入强制结束")
        }
    }
    let started = std::time::Instant::now();
    let exited = plan != GracefulWait::Immediate
        && wait_until_stopped_with_progress(
            game,
            grace_timeout,
            Some(GRACEFUL_WAIT_PROGRESS_INTERVAL),
        );
    let waited = started.elapsed().as_secs_f64();
    if exited {
        log::info!("停止游戏：游戏已自行退出，等待耗时 {waited:.1} 秒");
        return Ok(targets.len());
    }
    match plan {
        GracefulWait::Full => {
            log::warn!("停止游戏：温和关闭等待 {waited:.1} 秒后游戏仍未退出")
        }
        GracefulWait::ShortGrace => log::info!("停止游戏：短宽限 {waited:.1} 秒内游戏未退出"),
        // 没有等过，紧接着的强制结束日志会说明原因，这里不重复。
        GracefulWait::Immediate => {}
    }

    // 第二步：强制结束
    /*
     * sysinfo 的 kill() 在 Windows 上是 TerminateProcess、在 Unix 上是
     * SIGKILL，都会跳过游戏自己的存档与云同步流程，只在温和关闭失败后用。
     * 先重新刷新进程表：等待期间自己退出的进程不该再被 kill 一次。
     */
    system.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    let alive = targets
        .iter()
        .copied()
        .filter(|pid| system.process(*pid).is_some())
        .collect::<Vec<_>>();
    if alive.is_empty() {
        log::info!("停止游戏：进程已自行退出，无需强制结束");
        return Ok(targets.len());
    }
    let alive_pids = alive.iter().map(|pid| pid.as_u32()).collect::<Vec<_>>();
    match plan {
        GracefulWait::Full => {
            log::warn!("停止游戏：温和关闭超时，强制结束进程，PID={alive_pids:?}")
        }
        GracefulWait::ShortGrace => {
            log::warn!("停止游戏：温和关闭没有可投递的窗口，强制结束进程，PID={alive_pids:?}")
        }
        GracefulWait::Immediate => {
            log::warn!("停止游戏：直接强制结束进程，PID={alive_pids:?}")
        }
    }
    for pid in &alive {
        match system.process(*pid) {
            Some(process) if process.kill() => {}
            Some(_) => log::warn!("停止游戏：强制结束进程失败，PID={pid}"),
            None => log::warn!("停止游戏：强制结束前进程已退出，PID={pid}"),
        }
    }
    if wait_until_stopped(game, FORCE_EXIT_TIMEOUT) {
        log::info!("停止游戏：进程已强制退出");
        return Ok(targets.len());
    }
    log::warn!("停止游戏：强制结束后进程仍在运行");
    Err("game.stillRunning".into())
}

/// 温和关闭的等待上限。
///
/// Unity 退出要跑完 BepInEx 的 Chainloader 卸载（Harmony 逐条 unpatch）、
/// 存档写入与 Steam 云同步回调，装了较多 MOD 的存档经常超过 8 秒 —— 旧值一到点
/// 就 TerminateProcess，用户看到的就是「游戏直接关闭、Harmony 没来得及 unpatch」。
const GRACEFUL_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 强制结束的等待上限（TerminateProcess 基本是立即生效的，留一点余量）。
const FORCE_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// 一个关闭请求都没投递成功时的短宽限（仅 Windows）。
///
/// `WM_CLOSE` 可能已经进了目标线程的消息队列，只是窗口在枚举那一刻不可见或已被
/// 销毁（`GetWindowThreadProcessId` / `IsWindowVisible` 都看不到它）。既然已经
/// 请求过一次「温和关闭」，直接 TerminateProcess 会把这条边界情况也归进强杀。
/// 1.5 秒够消息泵取走一条已入队的消息，又不至于让用户明显干等。
const GRACEFUL_GRACE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// 温和关闭等待期间上报「还在等、游戏还活着」的日志间隔。
const GRACEFUL_WAIT_PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// 轮询游戏进程是否退出的间隔。
const EXIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// `ERROR_ACCESS_DENIED`：UIPI（用户界面特权隔离）会用它拒绝低权限进程向高权限
/// 游戏窗口投递消息。只在 Windows 上会出现，常量本身与平台无关。
const ERROR_ACCESS_DENIED: u32 = 5;

/// 兜底同步发送 `WM_CLOSE` 的等待上限（毫秒）。
#[cfg(windows)]
const SEND_CLOSE_TIMEOUT_MS: u32 = 5_000;

/// 温和关闭的等待计划，由投递结果决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GracefulWait {
    /// 至少有一个窗口投递成功：等待游戏自行退出，不保证已处理消息。
    Full,
    /// 一个关闭请求都没投递成功：只等短宽限。
    ShortGrace,
    /// 非 Windows 不投递窗口关闭消息，原生 macOS 与 Wine 都走此分支：
    /// 连短宽限也不等，直接进强制结束 —— 旧行为就是立刻 kill，白等 1.5 秒只会让
    /// 用户觉得「点了停止却卡住一下」。
    Immediate,
}

impl GracefulWait {
    fn timeout(self) -> std::time::Duration {
        match self {
            GracefulWait::Full => GRACEFUL_EXIT_TIMEOUT,
            GracefulWait::ShortGrace => GRACEFUL_GRACE_TIMEOUT,
            GracefulWait::Immediate => std::time::Duration::ZERO,
        }
    }
}

/// 投递成功过窗口就等完整超时，一个都没有就只等短宽限。
///
/// 抽成纯函数是为了让「短宽限只在真的没投递出去时使用」这条规则能被单测钉住：
/// 旧实现只看 `posted > 0`，为 0 时连一行日志都不打就静默强杀。
fn graceful_wait_plan(report: &GracefulCloseReport) -> GracefulWait {
    if report.posted > 0 {
        GracefulWait::Full
    } else if cfg!(windows) {
        GracefulWait::ShortGrace
    } else {
        GracefulWait::Immediate
    }
}

/// 一次温和关闭尝试的结果。
///
/// 旧实现只返回「成功投递了几个窗口」，而且这个数字为 0 时调用方直接静默强杀 ——
/// 用户报的「游戏直接关了、Harmony 没来得及 unpatch」在日志里完全看不出是
/// 「找不到窗口」「消息被 UIPI 拦掉」还是「游戏收到了但自己不退」。逐项计数就是
/// 为了让下一条 `rust.log` 能直接回答这个问题。
#[derive(Debug, Default, PartialEq, Eq)]
struct GracefulCloseReport {
    /// 命中的目标进程数（可能一个窗口都没有，例如 Wine 宿主进程）。
    targets: usize,
    /// 目标进程名下的顶层窗口总数（含隐藏窗口）。
    windows: usize,
    /// 其中 `IsWindowVisible` 为真的窗口数。
    visible_windows: usize,
    /// 成功投递（`PostMessageW` 或 `SendMessageTimeoutW` 兜底）的窗口数。
    posted: usize,
    /// 两次尝试都失败的窗口数。
    failed: usize,
    /// 最后一次失败的系统错误码（`GetLastError`）。
    last_error: Option<u32>,
}

/// 某个进程的顶层窗口里，哪些该收到 `WM_CLOSE`。
///
/// `visible` 按枚举顺序记录每个窗口是否可见，返回值是同一顺序的下标。
///
/// 优先只投递可见窗口（等价于用户点 ×）。一个可见的都没有时退回**全部**顶层窗口：
/// 游戏最小化到托盘、主窗口被插件隐藏、或以无窗口方式启动时 `IsWindowVisible`
/// 全是 0 —— 旧实现据此认定「没有窗口」，跳过所有等待直接 TerminateProcess。
/// 退回全部而不是随便挑一个，是因为隐藏窗口里哪个是真正的游戏窗口无法从外部判断，
/// 全部投递的成本只是一次 `PostMessageW`。
///
/// 只在枚举 Windows 窗口的地方与单测里用得到，macOS 构建不该为它留死代码。
#[cfg(any(windows, test))]
fn close_window_indexes(visible: &[bool]) -> Vec<usize> {
    let visible_indexes = visible
        .iter()
        .enumerate()
        .filter(|(_, visible)| **visible)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if visible_indexes.is_empty() {
        (0..visible.len()).collect()
    } else {
        visible_indexes
    }
}

/// 轮询等待游戏进程退出，返回是否已退出。
fn wait_until_stopped(game: &Path, timeout: std::time::Duration) -> bool {
    wait_until_stopped_with_progress(game, timeout, None)
}

/// 同一套轮询，可选每隔 `progress` 打一条 debug（剩余进程数）。
///
/// 只有温和关闭阶段传进度：那个上限是 20 秒，界面此时没有任何反馈，日志需要能
/// 证明「还在等、游戏还活着」而不是管理器卡死。强制结束阶段不传。
fn wait_until_stopped_with_progress(
    game: &Path,
    timeout: std::time::Duration,
    progress: Option<std::time::Duration>,
) -> bool {
    let started = std::time::Instant::now();
    let deadline = started + timeout;
    let mut next_progress = progress.map(|every| started + every);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(EXIT_POLL_INTERVAL);
        if !is_game_running(game) {
            return true;
        }
        if let (Some(every), Some(next)) = (progress, next_progress)
            && std::time::Instant::now() >= next
        {
            next_progress = Some(next + every);
            log::debug!(
                "停止游戏：温和关闭等待中，已等待 {:.1} 秒，仍在运行的游戏进程 {} 个",
                started.elapsed().as_secs_f64(),
                running_processes(game).len()
            );
        }
    }
    false
}

/// 请求游戏窗口关闭，返回逐项计数（见 [`GracefulCloseReport`]）。
///
/// Windows：枚举全部顶层窗口，按 PID 归到目标进程；优先投递可见窗口，没有可见
/// 窗口时退回该进程的全部顶层窗口。投递失败时再用 `SendMessageTimeoutW` 兜底。
/// 投递旨在让游戏自行退出；不证明消息已被处理或存档、云同步、卸载已完成。
///
/// 非 Windows：不投递任何窗口消息，`posted` 恒为 0。
/// 原生 macOS 与 Wine 均未实现温和信号，保持原有的强制结束行为。
#[cfg(windows)]
fn request_graceful_close(targets: &[sysinfo::Pid]) -> GracefulCloseReport {
    use winapi::shared::minwindef::{BOOL, LPARAM};
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};

    /// `EnumWindows` 回调：收集**全部**顶层窗口，可见性单独记一列。
    ///
    /// 过滤放到枚举之后做：旧实现在回调里只留可见窗口，一旦游戏没有可见窗口就
    /// 得到空集，调用方据此静默跳过所有等待直接强杀。
    ///
    /// 安全性：`lparam` 由本函数传入，指向在整个枚举期间都存活的 `windows`；
    /// 回调只在 `EnumWindows` 内部同步调用，不存在跨线程访问。
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = unsafe { &mut *(lparam as *mut Vec<(HWND, u32, bool)>) };
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid != 0 {
            let visible = unsafe { IsWindowVisible(hwnd) } != 0;
            windows.push((hwnd, pid, visible));
        }
        // 返回 1 继续枚举
        1
    }

    let mut windows: Vec<(HWND, u32, bool)> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut windows as *mut _ as LPARAM);
    }

    let mut report = GracefulCloseReport {
        targets: targets.len(),
        ..GracefulCloseReport::default()
    };
    for pid in targets {
        let pid = pid.as_u32();
        let owned = windows
            .iter()
            .filter(|(_, window_pid, _)| *window_pid == pid)
            .collect::<Vec<_>>();
        if owned.is_empty() {
            continue;
        }
        report.windows += owned.len();
        let visible_flags = owned
            .iter()
            .map(|(_, _, visible)| *visible)
            .collect::<Vec<_>>();
        report.visible_windows += visible_flags.iter().filter(|visible| **visible).count();
        for index in close_window_indexes(&visible_flags) {
            let hwnd = owned[index].0;
            match send_close(hwnd) {
                Ok(()) => report.posted += 1,
                Err(code) => {
                    report.failed += 1;
                    report.last_error = Some(code);
                    log::debug!("停止游戏：WM_CLOSE 投递失败，PID={pid}，错误码={code}");
                }
            }
        }
    }
    report
}

/// 向一个窗口请求关闭，返回失败时的系统错误码。
///
/// 先 `PostMessageW`（等价于点 ×，只入队不阻塞）；失败时再用
/// `SendMessageTimeoutW` 兜底：同步送进目标窗口过程并最多等 [`SEND_CLOSE_TIMEOUT_MS`]。
/// 对「消息队列异常 / 目标窗口刚创建还没消息泵」这一种是第二次机会。
/// 两者都失败才算这个窗口失败。
#[cfg(windows)]
fn send_close(hwnd: winapi::shared::windef::HWND) -> Result<(), u32> {
    use winapi::um::winuser::{
        PostMessageW, SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_CLOSE,
    };

    let posted = unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    if posted != 0 {
        return Ok(());
    }
    let post_error = last_system_error();
    /*
     * LRESULT 的成功判定是「非 0」，结果值本身（游戏窗口过程的返回值）不用。
     * `lpdwResult` 的类型是 `*mut usize`（winapi 的 `PDWORD_PTR`）。
     */
    let mut result: usize = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_CLOSE,
            0,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            SEND_CLOSE_TIMEOUT_MS,
            &mut result,
        )
    };
    if sent != 0 {
        log::debug!(
            "停止游戏：PostMessageW 失败（错误码={post_error}），SendMessageTimeoutW 兜底成功"
        );
        return Ok(());
    }
    let send_error = last_system_error();
    Err(if send_error != 0 {
        send_error
    } else {
        post_error
    })
}

/// 取上一条系统调用的错误码（Windows 上是 `GetLastError`）。
///
/// 走 `std::io::Error::last_os_error()` 而不是 winapi 的 `GetLastError`：std 在
/// Windows 上就是直接读 `GetLastError`，且不必为 winapi 再开 `errhandlingapi` feature。
/// 必须紧接失败调用之后取，中间不能插入任何可能改写 last-error 的调用。
#[cfg(windows)]
fn last_system_error() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}

#[cfg(not(windows))]
fn request_graceful_close(targets: &[sysinfo::Pid]) -> GracefulCloseReport {
    GracefulCloseReport {
        targets: targets.len(),
        ..GracefulCloseReport::default()
    }
}

#[cfg(test)]
mod tests;
