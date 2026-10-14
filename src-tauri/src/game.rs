use crate::bepinex::Package;
use crate::models::{BepInExInfo, GameConfig, GameInfo, WinhttpState};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, System, UpdateKind};

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
            complete: false,
            has_config: false,
            version: None,
            version_source: None,
            backups: Vec::new(),
        });
    let winhttp_state = path
        .as_ref()
        .map(|value| crate::bepinex::state(value, config))
        .unwrap_or(WinhttpState::Missing);
    let result = GameInfo {
        path: path.map(|p| p.to_string_lossy().to_string()),
        steam_library: detected
            .as_ref()
            .map(|location| location.library.to_string_lossy().to_string()),
        steam_root: detected
            .as_ref()
            .map(|location| location.steam_root.to_string_lossy().to_string()),
        wine_prefix: detected.and_then(|location| {
            location
                .wine_prefix
                .map(|prefix| prefix.to_string_lossy().to_string())
        }),
        game_exists,
        bepinex,
        kitchen_running,
        winhttp_state,
    };
    log::info!(
        "游戏状态探测完成：已找到路径={}，游戏文件存在={}，BepInEx已安装={}，BepInEx完整={}，游戏运行={}，WinHTTP={:?}",
        result.path.is_some(),
        result.game_exists,
        result.bepinex.installed,
        result.bepinex.complete,
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
    let root = if package == Package::MacOS
        && selected
            .file_name()
            .is_some_and(|name| name == package.executable())
    {
        selected.parent().ok_or("所选厨房目录无效")?
    } else {
        selected
    };
    if !executable_exists_for_package(root, package) {
        return Err(match package {
            Package::Windows => "所选目录中没有 Overcooked2.exe",
            Package::Linux => "所选目录中没有 Overcooked2.x86_64",
            Package::MacOS => "所选目录中没有 Overcooked2.app",
        }
        .into());
    }
    fs::canonicalize(root).map_err(|_| "所选厨房目录无效".into())
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
        let stored = PathBuf::from(&config.path);
        log::debug!("使用配置中的游戏路径：存在={}", stored.is_dir());
        // A Windows Steam library may live on a mapped drive outside drive_c.
        for steam in steam_root_paths(config) {
            for library in steam_libraries_from_roots(std::slice::from_ref(&steam)) {
                if normalize_path(&stored).starts_with(normalize_path(&library)) {
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
            if game_executable_exists(parent, config) {
                log::debug!("从程序目录探测到游戏");
                return Some(location(
                    parent.to_path_buf(),
                    parent.to_path_buf(),
                    parent,
                    config,
                ));
            }
        }
    }
    for steam in steam_root_paths(config) {
        for library in steam_libraries_from_roots(std::slice::from_ref(&steam)) {
            let game = library.join("steamapps/common/Overcooked! 2");
            if game_executable_exists(&game, config) {
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

    #[cfg(target_os = "linux")]
    {
        if let Some(home_path) = dirs::home_dir() {
            // Primary Steam installation paths on Linux
            roots.push(home_path.join(".steam/steam"));
            roots.push(home_path.join(".local/share/Steam"));
            // Flatpak Steam installation
            roots.push(home_path.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
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
    let installed = root.is_dir();
    let complete = primary.is_file() || secondary.is_file();
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
                .map(|entry| entry.path().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    backups.sort();
    let result = BepInExInfo {
        installed,
        complete,
        has_config: crate::mod_config::exists(game, "BepInEx"),
        version,
        version_source: source,
        backups,
    };
    log::info!(
        "BepInEx 探测完成：已安装={}，完整={}，版本={}",
        result.installed,
        result.complete,
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
        .any(|process| process_matches_game(&game, process.name(), process.exe(), process.cmd()))
}

fn process_matches_game(
    game: &Path,
    name: &str,
    executable: Option<&Path>,
    args: &[String],
) -> bool {
    let name = name.to_ascii_lowercase();
    let game_process = name.starts_with("overcooked");
    let wine_process = name.contains("wine") || name.contains("cxwinehelper");
    if !game_process && !wine_process {
        return false;
    }
    if let Some(executable) = executable {
        let executable = normalize_path(executable);
        if [
            game.join("Overcooked2.exe"),
            game.join("Overcooked2.x86_64"),
        ]
        .iter()
        .any(|target| normalize_path(target) == executable)
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

pub(crate) fn steam_launch_command(
    settings: &GameConfig,
    info: &GameInfo,
) -> Result<std::process::Command, String> {
    #[cfg(unix)]
    if !Package::current(settings).native() {
        let prefix = info
            .wine_prefix
            .as_deref()
            .ok_or("无法确定游戏使用的 Wine 容器，请重新选择厨房目录")?;
        return crate::wine::steam_command(Path::new(prefix));
    }
    #[cfg(windows)]
    let _ = settings;
    native_steam_launch_command(
        std::env::consts::OS,
        info.steam_root.as_deref().map(Path::new),
    )
}

fn native_steam_launch_command(
    platform: &str,
    steam_root: Option<&Path>,
) -> Result<std::process::Command, String> {
    let mut command = match platform {
        "windows" => {
            let mut command = std::process::Command::new("explorer.exe");
            command.arg("steam://rungameid/728880");
            command
        }
        "linux" => {
            // Avoid the steam:// handler, which Wine Steam may have registered.
            let flatpak = steam_root.is_some_and(|root| {
                root.components()
                    .any(|part| part.as_os_str() == "com.valvesoftware.Steam")
            });
            let mut command = if flatpak {
                let mut command = std::process::Command::new("flatpak");
                command.args(["run", "com.valvesoftware.Steam"]);
                command
            } else {
                std::process::Command::new("steam")
            };
            command.args(["-applaunch", "728880"]);
            command
        }
        "macos" => {
            let mut command = std::process::Command::new("open");
            // CrossOver can export a same-named Steam app; target the native bundle.
            command.args(["-b", "com.valvesoftware.steam", "steam://rungameid/728880"]);
            command
        }
        _ => return Err("当前平台不支持启动厨房".into()),
    };
    command.stdin(std::process::Stdio::null());
    Ok(command)
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
        Err("《胡闹厨房2》正在运行，请先关闭后重试".into())
    } else {
        log::debug!("确认游戏已停止，可以执行文件操作");
        Ok(())
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
        return Err("路径必须是游戏目录内的相对路径".into());
    }
    let root = resolve_existing_ancestor(game)?;
    let candidate = resolve_existing_ancestor(&root.join(rel))?;
    if !candidate.starts_with(&root) {
        log::warn!("拒绝逃逸游戏目录的路径：{}", relative);
        return Err("路径逃逸游戏目录".into());
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
    Err("路径逃逸游戏目录".into())
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
/// On Linux/macOS: Returns empty metadata.
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
    fn native_mac_selection_accepts_game_root_and_actual_app_bundle() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/native-mac-selection")
            .join(uuid::Uuid::new_v4().to_string());
        let root = fixture.join("Steam/steamapps/common/Overcooked! 2");
        fs::create_dir_all(&root).unwrap();
        assert!(!executable_exists_for_package(&root, Package::MacOS));
        assert_eq!(
            selected_root_for_package(&root, Package::MacOS).unwrap_err(),
            "所选目录中没有 Overcooked2.app"
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
        assert!(executable_exists_for_package(&root, Package::MacOS));
        assert!(!executable_exists_for_package(&root, Package::Windows));
        assert!(!executable_exists_for_package(&root, Package::Linux));
        fs::write(root.join("Overcooked2.exe"), b"fixture").unwrap();
        fs::write(root.join("Overcooked2.x86_64"), b"fixture").unwrap();
        for package in [Package::Windows, Package::Linux] {
            assert_eq!(selected_root_for_package(&root, package).unwrap(), expected);
            assert!(selected_root_for_package(&app, package).is_err());
        }
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn native_steam_launch_does_not_delegate_to_a_wine_url_handler() {
        let command = native_steam_launch_command("linux", None).unwrap();
        assert_eq!(command.get_program(), "steam");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-applaunch", "728880"]
        );
        let command = native_steam_launch_command(
            "linux",
            Some(Path::new(
                "/home/user/.var/app/com.valvesoftware.Steam/.local/share/Steam",
            )),
        )
        .unwrap();
        assert_eq!(command.get_program(), "flatpak");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["run", "com.valvesoftware.Steam", "-applaunch", "728880"]
        );
        let command = native_steam_launch_command("macos", None).unwrap();
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-b", "com.valvesoftware.steam", "steam://rungameid/728880"]
        );
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

    #[test]
    fn game_process_matching_covers_native_and_wine_without_treating_steam_as_the_game() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/game-processes");
        for (name, executable) in [
            ("Overcooked2.exe", root.join("Overcooked2.exe")),
            ("Overcooked2.x86_64", root.join("Overcooked2.x86_64")),
            (
                "Overcooked2",
                root.join("Overcooked2.app/Contents/MacOS/Overcooked2"),
            ),
        ] {
            assert!(process_matches_game(&root, name, Some(&executable), &[]));
        }
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
