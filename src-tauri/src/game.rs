use crate::bepinex::Package;
use crate::display_path::display_path;
use crate::models::{BepInExInfo, GameConfig, GameInfo, WinhttpState};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{Process, ProcessRefreshKind, ProcessStatus, System, UpdateKind};

pub fn detect_game(config: &crate::models::AppConfig) -> GameInfo {
    detect_game_from_settings(&config.game)
}

pub(crate) fn detect_game_from_settings(config: &crate::models::GameConfig) -> GameInfo {
    log::debug!(
        "开始探测游戏状态：配置路径非空={}",
        !config.path.trim().is_empty()
    );
    let detected = find_game_path(config);
    let path = detected.as_ref().map(|location| location.game.clone());
    let game_exists = path
        .as_ref()
        .map(|p| game_executable_exists(p, config))
        .unwrap_or(false);
    let kitchen_running = path.as_ref().map(|p| is_game_running(p)).unwrap_or(false);
    let backup_root = crate::user_paths::backups_dir().ok();
    let bepinex = path
        .as_ref()
        .map(|value| detect_bepinex(value, backup_root.as_deref()))
        .unwrap_or_else(|| BepInExInfo {
            installed: false,
            has_residue: false,
            has_config: false,
            version: None,
            version_source: None,
            backups: Vec::new(),
        });
    let winhttp_state = path
        .as_ref()
        .map(|value| crate::bepinex::state(value, config))
        .unwrap_or(WinhttpState::Missing);
    /*
     * 送给前端的路径统一走 display_path（剥 Windows verbatim 前缀、去多余
     * 尾分隔符）。去掉前缀不改变指向，但**改变字符串形态**：`fs::canonicalize`
     * 返回的是 `\\?\E:\...`，本字段是 `E:\...`。任何拿它与 canonicalize 结果
     * 做 `strip_prefix`/`starts_with` 比较的地方都必须改用 `safe_game_path`
     * 的返回值做基准，否则前缀会静默失配（批量开关曾因此收集不到任何插件）。
     */
    let path_text = path.as_ref().map(|p| display_path(p));
    let steam_library = detected
        .as_ref()
        .map(|location| display_path(&location.library));
    let steam_root = detected
        .as_ref()
        .map(|location| display_path(&location.steam_root));
    let wine_prefix = detected
        .and_then(|location| location.wine_prefix)
        .map(|prefix| display_path(prefix));
    let result = GameInfo {
        path: path_text,
        steam_library,
        steam_root,
        wine_prefix,
        game_exists,
        bepinex,
        kitchen_running,
        winhttp_state,
    };
    log::info!(
        "游戏状态探测完成：已找到路径={}，游戏文件存在={}，BepInEx已安装={}，残留目录={}，游戏运行={}，WinHTTP={:?}",
        result.path.is_some(),
        result.game_exists,
        result.bepinex.installed,
        result.bepinex.has_residue,
        result.kitchen_running,
        result.winhttp_state
    );
    result
}

pub(crate) fn game_executable_exists(game_dir: &Path, settings: &GameConfig) -> bool {
    executable_exists_for_package(game_dir, Package::current(settings))
}

fn executable_exists_for_package(game_dir: &Path, package: Package) -> bool {
    let executable = game_dir.join(package.executable());
    if package == Package::MacOS {
        executable.is_dir() && executable.join("Contents/MacOS").is_dir()
    } else {
        executable.is_file()
    }
}

pub(crate) fn selected_game_root(
    selected: &Path,
    settings: &GameConfig,
) -> Result<PathBuf, String> {
    selected_root_for_package(selected, Package::current(settings))
}

fn selected_root_for_package(selected: &Path, package: Package) -> Result<PathBuf, String> {
    let root = selection_root(selected, package);
    if !executable_exists_for_package(&root, package) {
        return Err(match package {
            Package::Windows => "game.exeNotFound",
            Package::MacOS => "game.appNotFound",
        }
        .into());
    }
    fs::canonicalize(&root).map_err(|_| "game.invalidDirectory".into())
}

/// 用户选中的路径 → 游戏根目录候选（纯路径运算，不访问磁盘）。
///
/// macOS 允许直接选游戏根、`Overcooked2.app` 包、包内的 `Contents/MacOS`
/// 甚至其中的可执行文件：后三种都向上回溯到包含 `.app` 的那一层，和自动
/// 探测得到同一个根字符串。Windows 包只接受游戏根。
fn selection_root(selected: &Path, package: Package) -> PathBuf {
    let selected = expand_home(selected);
    if package != Package::MacOS {
        return selected;
    }
    // 向上找 `.app` 组件本身（不是只比较最后一段），`Contents/MacOS` 也能覆盖。
    let Some(app) = selected.ancestors().find(|candidate| {
        candidate
            .file_name()
            .is_some_and(|name| name == package.executable())
    }) else {
        return selected;
    };
    app.parent().map(Path::to_path_buf).unwrap_or(selected)
}

/// 展开配置/选择器里的 `~`。
///
/// 只认开头的 `~` 或 `~/`、`~\`；`~user` 需要真实账户查询，不做猜测，原样返回。
fn expand_home(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    let rest = match text {
        "~" => "",
        value if value.starts_with("~/") || value.starts_with("~\\") => &value[1..],
        _ => return path.to_path_buf(),
    };
    let Some(home) = dirs::home_dir() else {
        return path.to_path_buf();
    };
    home.join(rest.trim_start_matches(['/', '\\']))
}

struct GameLocation {
    library: PathBuf,
    steam_root: PathBuf,
    game: PathBuf,
    wine_prefix: Option<PathBuf>,
}

fn location(
    library: PathBuf,
    game: PathBuf,
    steam_root: &Path,
    settings: &GameConfig,
) -> GameLocation {
    #[cfg(unix)]
    let wine_prefix = (!Package::current(settings).native())
        .then(|| {
            crate::wine::prefix_for_path(steam_root).or_else(|| crate::wine::prefix_for_path(&game))
        })
        .flatten();
    #[cfg(not(unix))]
    let wine_prefix = {
        let _ = (steam_root, settings);
        None
    };
    GameLocation {
        library,
        steam_root: steam_root.to_owned(),
        game,
        wine_prefix,
    }
}

fn find_game_path(config: &GameConfig) -> Option<GameLocation> {
    if !config.path.trim().is_empty() {
        /*
         * 配置里的路径可能是旧版本存下的原样字符串：带 `\\?\` 前缀、结尾
         * 斜杠、`~`，或 macOS 上直接指到 `Overcooked2.app` / `Contents/MacOS`。
         * 先收敛成游戏根并 canonicalize（解析符号链接：macOS 的
         * `/var` → `/private/var`），这样「用户选中」与「自动探测」落到同一个
         * 字符串；路径不存在时 canonicalize 失败，退回原样路径，与旧行为一致。
         */
        let stored = normalize_path(selection_root(
            Path::new(&config.path),
            Package::current(config),
        ));
        log::debug!("使用配置中的游戏路径：存在={}", stored.is_dir());
        // A Windows Steam library may live on a mapped drive outside drive_c.
        for steam in steam_root_paths(config) {
            for library in steam_libraries_from_roots(std::slice::from_ref(&steam)) {
                if stored.starts_with(normalize_path(&library)) {
                    return Some(location(library, stored, &steam, config));
                }
            }
        }
        return Some(location(stored.clone(), stored.clone(), &stored, config));
    }
    if !config.auto_detect {
        return None;
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            // 便携目录同样 canonicalize，避免 `\\?\` 与普通写法混用。
            let parent = normalize_path(parent);
            if game_executable_exists(&parent, config) {
                log::debug!("从程序目录探测到游戏");
                return Some(location(parent.clone(), parent.clone(), &parent, config));
            }
        }
    }
    for steam in steam_root_paths(config) {
        for library in steam_libraries_from_roots(std::slice::from_ref(&steam)) {
            let candidate = library.join("steamapps/common/Overcooked! 2");
            if game_executable_exists(&candidate, config) {
                let game = normalize_path(&candidate);
                log::debug!("从当前运行模式的 Steam 库探测到游戏");
                return Some(location(library, game, &steam, config));
            }
        }
    }
    log::warn!("未探测到当前运行模式的 Overcooked! 2 游戏目录");
    None
}

/// Read only the small registry/environment hints. The state observer uses
/// these to notice a newly installed Steam client without reparsing its VDF.
pub(crate) fn steam_root_paths(settings: &GameConfig) -> Vec<PathBuf> {
    #[cfg(unix)]
    if !Package::current(settings).native() {
        return crate::wine::steam_roots();
    }
    #[cfg(windows)]
    let _ = settings;
    let mut roots = Vec::new();

    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            for value in ["SteamPath", "InstallPath"] {
                if let Ok(path) = key.get_value::<String, _>(value) {
                    roots.push(PathBuf::from(path));
                }
            }
        }
        if let Some(program_files) = std::env::var_os("PROGRAMFILES(X86)") {
            roots.push(PathBuf::from(program_files).join("Steam"));
        }
        if let Some(program_files) = std::env::var_os("PROGRAMFILES") {
            roots.push(PathBuf::from(program_files).join("Steam"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home_path) = dirs::home_dir() {
            roots.push(home_path.join("Library/Application Support/Steam"));
        }
    }

    roots
}

pub(crate) fn steam_libraries_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for root in roots {
        let root = normalize_path(root);
        if !libraries.contains(&root) {
            libraries.push(root.clone());
        }
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(vdf) {
            if let Ok(regex) = Regex::new(r#""path"\s+"([^"]+)""#) {
                for capture in regex.captures_iter(&text) {
                    if let Some(value) = capture.get(1) {
                        #[cfg(unix)]
                        let path = if crate::wine::prefix_for_path(&root).is_some() {
                            crate::wine::host_library_path(&root, value.as_str())
                        } else {
                            let path = PathBuf::from(value.as_str());
                            path.is_absolute().then_some(path)
                        };
                        #[cfg(not(unix))]
                        let path = Some(PathBuf::from(value.as_str().replace("\\\\", "\\")));
                        let Some(path) = path.map(normalize_path) else {
                            continue;
                        };
                        if !libraries.contains(&path) {
                            libraries.push(path);
                        }
                    }
                }
            }
        }
    }
    log::trace!("Steam 库探测完成：候选数量={}", libraries.len());
    libraries
}

pub fn detect_bepinex(game: &Path, backup_root: Option<&Path>) -> BepInExInfo {
    log::debug!("开始探测 BepInEx：游戏目录={}", game.display());
    let root = game.join("BepInEx");
    let core = root.join("core");
    let primary = core.join("BepInEx.dll");
    let secondary = core.join("BepInEx.Core.dll");
    /*
     * 「已安装」= 框架核心程序集存在，**不是** `BepInEx/` 目录存在。
     *
     * 装任何 MOD 都会创建 `BepInEx/plugins/`，那个空壳会一直留在游戏目录里。
     * 旧实现用 `root.is_dir()` 判断，于是没装框架也报「已安装」，
     * 界面给出启用开关和卸载按钮 —— 而开关必然失败（找不到加载器）。
     *
     * 目录在但核心不在 = 残缺（用户装过 MOD、没装框架，或框架被手工删了一半），
     * 单独用 `has_residue` 表达，界面据此提示「安装会保留 plugins/config」。
     */
    let installed = primary.is_file() || secondary.is_file();
    let has_residue = !installed && root.is_dir();
    let (version, source) = [primary, secondary]
        .into_iter()
        .find_map(|file| {
            read_file_version(&file)
                .filter(|value| value != "0.0.0" && value != "0.0.0.0")
                .map(|v| (Some(v), Some("FileVersion".into())))
        })
        .unwrap_or((None, None));
    let mut backup_roots = vec![game.to_path_buf()];
    if let Some(root) = backup_root {
        if root != game {
            backup_roots.push(root.to_path_buf());
        }
    }
    let mut backups = backup_roots
        .into_iter()
        .flat_map(|root| {
            fs::read_dir(root)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.flatten())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("BepInEx.backup-")
                        && entry.path().is_dir()
                })
                .map(|entry| display_path(entry.path()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    backups.sort();
    let result = BepInExInfo {
        installed,
        has_residue,
        has_config: crate::mod_config::exists(game, "BepInEx"),
        version,
        version_source: source,
        backups,
    };
    log::info!(
        "BepInEx 探测完成：已安装={}，残留目录={}，版本={}",
        result.installed,
        result.has_residue,
        result.version.as_deref().unwrap_or("未知")
    );
    result
}

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
fn process_looks_running(game: &Path, process: &Process) -> bool {
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

pub(crate) fn steam_launch_command(settings: &GameConfig, info: &GameInfo) -> Result<(), String> {
    #[cfg(unix)]
    if !Package::current(settings).native() {
        let prefix = info.wine_prefix.as_deref().ok_or("wine.prefixUnknown")?;
        return crate::wine::steam_command(Path::new(prefix))?
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string());
    }
    /*
     * Windows 与 macOS 原生走平台自己的启动方式。`settings` / `info`
     * 在这条路径上都不用，显式标记避免未使用警告 —— 它们的取值只在
     * macOS 的 Wine 分支里需要。
     */
    #[cfg(windows)]
    let _ = (settings, info);
    native_steam_launch(std::env::consts::OS)
}

/// 启动厨房时请求的 Steam URL。
const STEAM_RUN_URL: &str = "steam://rungameid/728880";

/// 原生模式的启动方式。
///
/// 抽成纯函数是为了让「Windows 走协议关联、macOS 指名 Steam bundle」这条规则
/// 能被测试钉住 —— 真正执行的两个入口一个走 ShellExecute、一个起子进程，
/// 单测里都断言不了。
#[derive(Debug, PartialEq, Eq)]
enum SteamStrategy {
    /// 交给系统默认协议处理器（Windows：`ShellExecuteW`）。
    ///
    /// 不要退回 `explorer.exe steam://...`：那是借资源管理器当中转，绕过了
    /// 协议关联，用户把 Steam 关联改到别处时就会失效。
    ProtocolHandler,
    /// 用 `open -b <bundle>` 指名应用：CrossOver 会导出同名 Steam app，
    /// 只按协议打开有几率起错那个。
    OpenBundle(&'static str),
}

fn steam_launch_strategy(platform: &str) -> Result<SteamStrategy, String> {
    match platform {
        "windows" => Ok(SteamStrategy::ProtocolHandler),
        "macos" => Ok(SteamStrategy::OpenBundle("com.valvesoftware.steam")),
        _ => Err("game.unsupportedPlatform".into()),
    }
}

/// 原生模式下启动厨房：只有 Windows 与 macOS 两条路径。
fn native_steam_launch(platform: &str) -> Result<(), String> {
    match steam_launch_strategy(platform)? {
        SteamStrategy::ProtocolHandler => crate::shell_open::open_url(STEAM_RUN_URL),
        SteamStrategy::OpenBundle(bundle) => std::process::Command::new("open")
            .args(["-b", bundle, STEAM_RUN_URL])
            .stdin(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string()),
    }
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

/// 结束正在运行的《胡闹厨房2》进程。
///
/// Steam 没有「停止游戏」的 URL 协议（只有 `steam://rungameid/<appid>` 用于启动），
/// 所以「停止」只能由管理器结束游戏进程。
///
/// 流程是**先温和、再强制**：
///   1. Windows 上给游戏主窗口投递 `WM_CLOSE`，等同于用户点窗口右上角的 ×，
///      游戏会走自己的退出流程（保存进度、BepInEx/Harmony 卸载、释放资源）；
///   2. 至少有一个窗口收到消息时最多等 [`GRACEFUL_EXIT_TIMEOUT`]；一个都没投递
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
    /// 至少有一个窗口收到了关闭请求：等游戏走完自己的退出流程。
    Full,
    /// 一个关闭请求都没投递成功：只等短宽限。
    ShortGrace,
    /// 这个平台不投递窗口关闭消息（macOS 的游戏在 Wine 容器里、进程模型不同）：
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
/// 游戏收到 `WM_CLOSE` 后走自己的退出流程，存档、云同步与 BepInEx 卸载都能完成。
///
/// 非 Windows：不投递任何窗口消息，`posted` 恒为 0。
/// macOS 原生版的游戏在 Wine 容器里跑（进程模型不同），发 SIGTERM 不等价，
/// 暂不实现，保持原有的强制结束行为。
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

pub fn safe_game_path(game: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || relative.contains(':')
        || rel
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        log::warn!("拒绝不安全游戏相对路径：{}", relative);
        return Err("game.relativePathRequired".into());
    }
    let root = resolve_existing_ancestor(game)?;
    let candidate = resolve_existing_ancestor(&root.join(rel))?;
    if !candidate.starts_with(&root) {
        log::warn!("拒绝逃逸游戏目录的路径：{}", relative);
        return Err("archive.pathEscape".into());
    }
    log::trace!("解析游戏相对路径：{}", relative);
    Ok(candidate)
}

pub(crate) fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    // A new file cannot be canonicalized yet. Resolve its nearest existing
    // ancestor so a junction cannot hide an escape behind a missing suffix.
    for ancestor in absolute.ancestors() {
        match fs::canonicalize(ancestor) {
            Ok(resolved) => {
                let suffix = absolute.strip_prefix(ancestor).map_err(|e| e.to_string())?;
                return Ok(if suffix.as_os_str().is_empty() {
                    resolved
                } else {
                    resolved.join(suffix)
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("archive.pathEscape".into())
}

/// BepInEx and MOD assemblies remain PE files even for native Unix games.
pub fn read_file_version(path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use winapi::shared::minwindef::{DWORD, LPVOID, UINT};
        use winapi::um::winver::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut handle = 0_u32;
        let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle) };
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0_u8; size as usize];
        let loaded = unsafe {
            GetFileVersionInfoW(wide.as_ptr(), handle, size, buffer.as_mut_ptr() as *mut _)
        };
        if loaded == 0 {
            return None;
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct FixedFileInfo {
            signature: DWORD,
            _struct_version: DWORD,
            file_version_ms: DWORD,
            file_version_ls: DWORD,
            _product_version_ms: DWORD,
            _product_version_ls: DWORD,
            _file_flags_mask: DWORD,
            _file_flags: DWORD,
            _file_os: DWORD,
            _file_type: DWORD,
            _file_subtype: DWORD,
            _file_date_ms: DWORD,
            _file_date_ls: DWORD,
        }
        let query = [b'\\' as u16, 0];
        let mut value: LPVOID = null_mut();
        let mut value_len: UINT = 0;
        let queried = unsafe {
            VerQueryValueW(
                buffer.as_ptr() as *const _,
                query.as_ptr(),
                &mut value,
                &mut value_len,
            )
        };
        if queried == 0
            || value.is_null()
            || value_len < std::mem::size_of::<FixedFileInfo>() as u32
        {
            return None;
        }
        let info = unsafe { std::ptr::read_unaligned(value as *const FixedFileInfo) };
        if info.signature != 0xFEEF04BD {
            return None;
        }
        Some(format!(
            "{}.{}.{}.{}",
            info.file_version_ms >> 16,
            info.file_version_ms & 0xFFFF,
            info.file_version_ls >> 16,
            info.file_version_ls & 0xFFFF
        ))
    }
    #[cfg(not(windows))]
    {
        if fs::metadata(path).ok()?.len() > 64 * 1024 * 1024 {
            return None;
        }
        portable_pe_version(&fs::read(path).ok()?)
    }
}

#[cfg(any(not(windows), test))]
fn portable_pe_version(bytes: &[u8]) -> Option<String> {
    if bytes.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if bytes.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let offset = bytes
        .windows(8)
        .position(|window| window == [0xbd, 0x04, 0xef, 0xfe, 0x00, 0x00, 0x01, 0x00])?;
    let version = bytes.get(offset + 8..offset + 16)?;
    let ms = u32::from_le_bytes(version[..4].try_into().ok()?);
    let ls = u32::from_le_bytes(version[4..].try_into().ok()?);
    (ms != 0 || ls != 0)
        .then(|| format!("{}.{}.{}.{}", ms >> 16, ms & 0xffff, ls >> 16, ls & 0xffff))
}

#[derive(Debug, Clone, Default)]
pub struct FileMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub product_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub original_filename: Option<String>,
}

/// Reads display metadata from file version resources.
/// On Windows: Reads PE version resource metadata.
/// On macOS: Returns empty metadata.
pub fn read_file_metadata(path: &Path) -> FileMetadata {
    #[cfg(windows)]
    {
        FileMetadata {
            title: read_version_string(path, "FileDescription"),
            description: read_version_string(path, "Comments"),
            product_name: read_version_string(path, "ProductName"),
            company: read_version_string(path, "CompanyName"),
            copyright: read_version_string(path, "LegalCopyright"),
            original_filename: read_version_string(path, "OriginalFilename"),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        log::trace!("文件元数据读取：非 Windows 平台返回空元数据");
        FileMetadata::default()
    }
}

#[cfg(windows)]
fn read_version_string(path: &Path, key: &str) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::shared::minwindef::{LPVOID, UINT};
    use winapi::um::winver::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut handle = 0_u32;
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0_u8; size as usize];
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), handle, size, buffer.as_mut_ptr() as *mut _) }
        == 0
    {
        return None;
    }
    for language in [
        "040904b0", "080404b0", "040904e4", "080904b0", "000004b0", "000004e4",
    ] {
        let query = format!(r"\StringFileInfo\{language}\{key}");
        let query_w: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();
        let mut value: LPVOID = null_mut();
        let mut value_len: UINT = 0;
        let queried = unsafe {
            VerQueryValueW(
                buffer.as_ptr() as *const _,
                query_w.as_ptr(),
                &mut value,
                &mut value_len,
            )
        };
        if queried != 0 && !value.is_null() && value_len > 1 {
            let slice =
                unsafe { std::slice::from_raw_parts(value as *const u16, value_len as usize) };
            let end = slice
                .iter()
                .position(|item| *item == 0)
                .unwrap_or(slice.len());
            let value = String::from_utf16_lossy(&slice[..end]).trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn graceful_close_prefers_visible_windows_then_falls_back_to_all() {
        // 没有任何顶层窗口：没有可投递的目标
        assert!(close_window_indexes(&[]).is_empty());
        // 有可见窗口：只投递可见的（等价于用户点 ×）
        assert_eq!(close_window_indexes(&[true, false]), vec![0]);
        assert_eq!(close_window_indexes(&[false, true]), vec![1]);
        assert_eq!(close_window_indexes(&[true, false, true]), vec![0, 2]);
        // 一个可见窗口都没有（最小化到托盘 / 主窗口被插件隐藏）：退回全部顶层窗口
        assert_eq!(close_window_indexes(&[false]), vec![0]);
        assert_eq!(close_window_indexes(&[false, false]), vec![0, 1]);
    }

    #[test]
    fn graceful_wait_timeout_follows_successfully_posted_windows() {
        // 完整超时要留给 Unity 退出 + BepInEx/Harmony 卸载 + 存档写入
        assert_eq!(GRACEFUL_EXIT_TIMEOUT, std::time::Duration::from_secs(20));
        // 一个都没投递成功时只等短宽限，而且必须明显短于完整超时
        assert_eq!(
            GRACEFUL_GRACE_TIMEOUT,
            std::time::Duration::from_millis(1500)
        );
        assert!(GRACEFUL_GRACE_TIMEOUT < GRACEFUL_EXIT_TIMEOUT);

        // 一个都没投递成功时怎么等：Windows 上有 WM_CLOSE 可能还在队列里，等短宽限；
        // macOS 的游戏在 Wine 容器里、没有窗口消息可投递，保持旧行为立刻强杀。
        let not_posted_plan = if cfg!(windows) {
            GracefulWait::ShortGrace
        } else {
            GracefulWait::Immediate
        };
        assert_eq!(GracefulWait::Immediate.timeout(), std::time::Duration::ZERO);

        // 目标进程一个窗口都没有（例如 Wine 宿主进程）
        let no_windows = GracefulCloseReport {
            targets: 2,
            ..GracefulCloseReport::default()
        };
        assert_eq!(graceful_wait_plan(&no_windows), not_posted_plan);
        assert_eq!(GracefulWait::ShortGrace.timeout(), GRACEFUL_GRACE_TIMEOUT);

        // 有窗口但一个都没投递出去（例如全部被 UIPI 拒绝）：同样走「没投递成功」分支
        let all_failed = GracefulCloseReport {
            targets: 1,
            windows: 2,
            visible_windows: 2,
            posted: 0,
            failed: 2,
            last_error: Some(ERROR_ACCESS_DENIED),
        };
        assert_eq!(graceful_wait_plan(&all_failed), not_posted_plan);

        // PostMessageW 失败、SendMessageTimeoutW 兜底成功：算投递成功，等完整超时
        let retried = GracefulCloseReport {
            targets: 1,
            windows: 1,
            visible_windows: 1,
            posted: 1,
            failed: 0,
            last_error: None,
        };
        assert_eq!(graceful_wait_plan(&retried), GracefulWait::Full);
        assert_eq!(GracefulWait::Full.timeout(), GRACEFUL_EXIT_TIMEOUT);
    }

    #[test]
    fn native_mac_selection_accepts_game_root_and_actual_app_bundle() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/native-mac-selection")
            .join(uuid::Uuid::new_v4().to_string());
        let root = fixture.join("Steam/steamapps/common/Overcooked! 2");
        fs::create_dir_all(&root).unwrap();
        assert!(!executable_exists_for_package(&root, Package::MacOS));
        assert_eq!(
            selected_root_for_package(&root, Package::MacOS).unwrap_err(),
            "game.appNotFound"
        );
        let app = root.join("Overcooked2.app");
        fs::create_dir_all(&app).unwrap();
        assert!(selected_root_for_package(&root, Package::MacOS).is_err());
        assert!(selected_root_for_package(&app, Package::MacOS).is_err());
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("Contents/MacOS/Overcooked2"), b"fixture").unwrap();
        let expected = fs::canonicalize(&root).unwrap();
        assert_eq!(
            selected_root_for_package(&root, Package::MacOS).unwrap(),
            expected
        );
        assert_eq!(
            selected_root_for_package(&app, Package::MacOS).unwrap(),
            expected
        );
        // 选到包内 `Contents/MacOS`（甚至其中的可执行文件）同样回到游戏根。
        for selected in [
            app.join("Contents/MacOS"),
            app.join("Contents/MacOS/Overcooked2"),
        ] {
            assert_eq!(
                selected_root_for_package(&selected, Package::MacOS).unwrap(),
                expected
            );
        }
        assert!(executable_exists_for_package(&root, Package::MacOS));
        assert!(!executable_exists_for_package(&root, Package::Windows));
        fs::write(root.join("Overcooked2.exe"), b"fixture").unwrap();
        // 两个包都能接受游戏根目录
        for package in [Package::Windows, Package::MacOS] {
            assert_eq!(selected_root_for_package(&root, package).unwrap(), expected);
        }
        /*
         * `.app` bundle 只在 macOS 包下被接受（上面已断言它会向上回溯到
         * 游戏根）；Windows 包必须拒绝 —— 它不是游戏目录。
         */
        assert!(selected_root_for_package(&app, Package::Windows).is_err());
        fs::remove_dir_all(fixture).unwrap();
    }

    /// 用户选中的 `.app` / `Contents/MacOS` 必须回到同一个游戏根字符串。
    #[test]
    fn selection_root_maps_macos_bundle_and_contents_paths_to_the_game_root() {
        let root = Path::new("/Games/Overcooked! 2").to_path_buf();
        let app = root.join("Overcooked2.app");
        for selected in [
            root.clone(),
            app.clone(),
            app.join("Contents/MacOS"),
            app.join("Contents/MacOS/Overcooked2"),
        ] {
            assert_eq!(selection_root(&selected, Package::MacOS), root);
        }
        // Windows 包不做向上回溯：`.app` 不是合法游戏根。
        assert_eq!(selection_root(&app, Package::Windows), app);
    }

    #[test]
    fn selection_expands_home_prefix_but_not_named_accounts() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        assert_eq!(expand_home(Path::new("~")), home);
        assert_eq!(
            expand_home(Path::new("~/Games/Overcooked! 2")),
            home.join("Games/Overcooked! 2")
        );
        // `~user` 需要真实账户查询，不猜测。
        assert_eq!(
            expand_home(Path::new("~someone/game")),
            PathBuf::from("~someone/game")
        );
    }

    /// 配置里历史写法的路径（结尾斜杠、`.`/`..` 段）与预期根收敛到同一字符串，
    /// 输出给前端时再统一成 display_path 形态（无 verbatim 前缀）。
    #[test]
    fn configured_path_variants_converge_on_one_game_root() {
        let fixture =
            std::env::temp_dir().join(format!("oc2-path-variants-{}", uuid::Uuid::new_v4()));
        let root = fixture.join("Overcooked! 2");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("Overcooked2.exe"), b"fixture").unwrap();
        let canonical = fs::canonicalize(&root).unwrap();
        let expected = display_path(&canonical);
        for variant in [
            root.to_string_lossy().into_owned(),
            format!("{}/", root.to_string_lossy()),
            format!("{}/sub/..", root.to_string_lossy()),
        ] {
            let config = GameConfig {
                path: variant.clone(),
                auto_detect: false,
                runtime: crate::models::GameRuntime::Wine,
            };
            let detected = find_game_path(&config).expect(&variant);
            assert_eq!(detected.game, canonical, "{variant}");
            let info = detect_game_from_settings(&config);
            assert_eq!(info.path.as_deref(), Some(expected.as_str()), "{variant}");
            assert!(
                !info
                    .path
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with(r"\\?\")
            );
        }
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn native_steam_launch_uses_the_platform_handler() {
        assert_eq!(
            steam_launch_strategy("windows").unwrap(),
            SteamStrategy::ProtocolHandler
        );
        assert_eq!(
            steam_launch_strategy("macos").unwrap(),
            SteamStrategy::OpenBundle("com.valvesoftware.steam")
        );
        // 未列入的平台名一律拒绝：本项目只支持 Windows 与 macOS。
        assert!(steam_launch_strategy("linux").is_err());
    }

    fn winhttp_state(root: &Path) -> WinhttpState {
        crate::bepinex::state(
            root,
            &GameConfig {
                path: String::new(),
                auto_detect: false,
                runtime: crate::models::GameRuntime::Wine,
            },
        )
    }

    /// macOS 上 Wine/CrossOver 的常驻宿主进程会带着启动过的程序路径活很久，
    /// 不能算成「游戏在运行」—— 否则游戏退出后按钮永远停在「停止游戏」，
    /// 所有要求游戏已停止的操作都会被拒。
    #[test]
    fn wine_host_processes_never_count_as_the_running_game() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/game-processes");
        let windows_path =
            r"C:\Program Files (x86)\Steam\steamapps\common\Overcooked! 2\Overcooked2.exe";
        for name in [
            "wineserver",
            "wineboot.exe",
            "winedevice.exe",
            "winemenubuilder.exe",
            "winedbg",
            "services.exe",
            "explorer.exe",
            "cxstart.exe",
            "steam.exe",
        ] {
            assert!(
                !process_matches_game(&root, name, None, &[windows_path.into()]),
                "{name} 是 Wine 宿主进程，不能算作游戏"
            );
        }
        // 真正承载游戏的宿主进程仍然算运行中（macOS CrossOver 只有它们可判）
        for name in ["wine64-preloader", "cxwinehelper"] {
            assert!(process_matches_game(
                &root,
                name,
                None,
                &[windows_path.into()]
            ));
        }
    }

    #[test]
    fn game_process_matching_covers_native_and_wine_without_treating_steam_as_the_game() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/game-processes");
        for (name, executable) in [
            ("Overcooked2.exe", root.join("Overcooked2.exe")),
            (
                "Overcooked2",
                root.join("Overcooked2.app/Contents/MacOS/Overcooked2"),
            ),
        ] {
            assert!(process_matches_game(&root, name, Some(&executable), &[]));
        }
        // 平台已不发布的原生可执行文件名不再被当作本游戏的进程
        assert!(!process_matches_game(
            &root,
            "Overcooked2.x86_64",
            Some(&root.join("Overcooked2.x86_64")),
            &[]
        ));
        assert!(process_matches_game(
            &root,
            "wine-preloader",
            None,
            &[r"C:\Games\Overcooked2.exe".into()]
        ));
        assert!(!process_matches_game(
            &root,
            "steam.exe",
            None,
            &["-applaunch".into(), "728880".into()]
        ));
        assert!(!process_matches_game(
            &root,
            "wine",
            None,
            &["steam.exe".into()]
        ));
        assert!(!process_matches_game(
            &root,
            "Overcooked2.exe",
            Some(&root.join("other/Overcooked2.exe")),
            &[]
        ));
    }

    #[test]
    fn reads_fixed_pe_file_version_without_windows_apis() {
        let mut bytes = vec![0_u8; 80];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&64_u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes.extend([0xbd, 0x04, 0xef, 0xfe, 0x00, 0x00, 0x01, 0x00]);
        bytes.extend(((5_u32 << 16) | 4).to_le_bytes());
        bytes.extend(((23_u32 << 16) | 5).to_le_bytes());
        assert_eq!(portable_pe_version(&bytes).as_deref(), Some("5.4.23.5"));
        assert!(portable_pe_version(&bytes[..88]).is_none());
        bytes[64] = 0;
        assert!(portable_pe_version(&bytes).is_none());
        assert!(portable_pe_version(b"ELF fixture").is_none());
    }

    #[test]
    fn rejects_parent_paths() {
        let root = std::env::temp_dir().join("oc2-manager-test");
        assert!(safe_game_path(&root, "../outside").is_err());
        assert!(safe_game_path(&root, "BepInEx/plugins/mod.dll:stream").is_err());
        assert!(safe_game_path(&root, "BepInEx/plugins/mod.dll").is_ok());
    }

    #[test]
    #[cfg(windows)]
    fn rejects_new_files_under_a_junction_outside_the_game() {
        let workspace =
            std::env::temp_dir().join(format!("oc2-path-test-{}", uuid::Uuid::new_v4()));
        let game = workspace.join("game");
        let outside = workspace.join("outside");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let link = game.join("BepInEx");
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(output.status.success(), "failed to create test junction");
        let rejected = safe_game_path(&game, "BepInEx/plugins/New.dll").is_err();
        // Remove the junction itself before deleting either real directory.
        fs::remove_dir(link).unwrap();
        fs::remove_dir(game).unwrap();
        fs::remove_dir(outside).unwrap();
        fs::remove_dir(workspace).unwrap();
        assert!(
            rejected,
            "a missing file must not bypass its existing ancestor's containment check"
        );
    }

    #[test]
    fn winhttp_state_machine() {
        let root = std::env::temp_dir().join(format!("oc2-winhttp-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::Missing);
        fs::write(root.join("winhttp.dll"), b"loader").unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
        fs::rename(root.join("winhttp.dll"), root.join("winhttp.dll.disabled")).unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::ModDisabled);
        fs::rename(root.join("winhttp.dll.disabled"), root.join("winhttp.dll")).unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
        assert_eq!(fs::read(root.join("winhttp.dll")).unwrap(), b"loader");
        fs::write(root.join("winhttp.dll.disabled"), b"disabled loader").unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::Conflict);
        fs::remove_file(root.join("winhttp.dll")).unwrap();
        fs::remove_file(root.join("winhttp.dll.disabled")).unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::Missing);
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn winhttp_state_does_not_recognize_obsolete_marker() {
        let root =
            std::env::temp_dir().join(format!("oc2-winhttp-marker-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let obsolete = root.join("winhttp.dll.oc2mod.disabled");
        fs::write(&obsolete, b"obsolete loader").unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::Missing);
        fs::write(root.join("winhttp.dll"), b"loader").unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
        fs::remove_file(root.join("winhttp.dll")).unwrap();
        fs::remove_file(obsolete).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
