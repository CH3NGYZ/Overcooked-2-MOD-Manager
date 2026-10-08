use crate::models::{BepInExInfo, GameInfo, WinhttpState};
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
    let path = detected.as_ref().map(|(_, game)| game.clone());
    let game_exists = path
        .as_ref()
        .map(|p| p.join("Overcooked2.exe").is_file())
        .unwrap_or(false);
    let kitchen_running = path.as_ref().map(|p| is_game_running(p)).unwrap_or(false);
    let backup_root = std::env::current_exe()
        .ok()
        .and_then(|value| value.parent().map(|parent| parent.join("backups")));
    let bepinex = path
        .as_ref()
        .map(|value| detect_bepinex(value, backup_root.as_deref()))
        .unwrap_or_else(|| BepInExInfo {
            installed: false,
            complete: false,
            version: None,
            version_source: None,
            backups: Vec::new(),
        });
    let winhttp_state = path
        .as_ref()
        .map(|value| winhttp_state(value))
        .unwrap_or(WinhttpState::Missing);
    let result = GameInfo {
        path: path.map(|p| p.to_string_lossy().to_string()),
        steam_library: detected.map(|(library, _)| library.to_string_lossy().to_string()),
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

fn find_game_path(config: &crate::models::GameConfig) -> Option<(PathBuf, PathBuf)> {
    if !config.path.is_empty() {
        let stored = PathBuf::from(&config.path);
        log::debug!("使用配置中的游戏路径：存在={}", stored.is_dir());
        return Some((stored.clone(), stored));
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            if parent.join("Overcooked2.exe").is_file() {
                log::debug!("从程序目录探测到游戏");
                return Some((parent.to_path_buf(), parent.to_path_buf()));
            }
        }
    }
    let detected = steam_libraries().into_iter().find_map(|library| {
        let game = library.join("steamapps/common/Overcooked! 2");
        game.join("Overcooked2.exe")
            .is_file()
            .then_some((library, game))
    });
    if detected.is_some() {
        log::debug!("从 Steam 库探测到游戏");
    } else {
        log::warn!("未探测到 Overcooked! 2 游戏目录");
    }
    detected
}

fn steam_libraries() -> Vec<PathBuf> {
    steam_libraries_from_roots(&steam_root_paths())
}

/// Read only the small registry/environment hints. The state observer uses
/// these to notice a newly installed Steam client without reparsing its VDF.
pub(crate) fn steam_root_paths() -> Vec<PathBuf> {
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
    }
    if let Some(program_files) = std::env::var_os("PROGRAMFILES(X86)") {
        roots.push(PathBuf::from(program_files).join("Steam"));
    }
    if let Some(program_files) = std::env::var_os("PROGRAMFILES") {
        roots.push(PathBuf::from(program_files).join("Steam"));
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
                        let path =
                            normalize_path(PathBuf::from(value.as_str().replace("\\\\", "\\")));
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

pub fn winhttp_state(game: &Path) -> WinhttpState {
    let enabled = game.join("winhttp.dll").is_file();
    let disabled = game.join("winhttp.dll.disabled").is_file();
    let state = match (enabled, disabled) {
        (true, false) => WinhttpState::ModEnabled,
        (false, true) => WinhttpState::ModDisabled,
        (false, false) => WinhttpState::Missing,
        (true, true) => WinhttpState::Conflict,
    };
    log::trace!("WinHTTP 状态：{:?}", state);
    state
}

pub fn is_game_running(game: &Path) -> bool {
    let running = is_game_running_with_system(game, &mut System::new());
    log::trace!("游戏进程检查：运行中={}", running);
    running
}

/// Reuse process metadata in the observer without logging unchanged checks.
/// File operations still call `is_game_running` with a fresh process snapshot.
pub(crate) fn is_game_running_with_system(game: &Path, system: &mut System) -> bool {
    let target = game.join("Overcooked2.exe");
    let target = normalize_path(target);
    system
        .refresh_processes_specifics(ProcessRefreshKind::new().with_exe(UpdateKind::OnlyIfNotSet));
    system.processes().values().any(|process| {
        if !process.name().eq_ignore_ascii_case("Overcooked2.exe") {
            return false;
        }
        process
            .exe()
            .map(|path| normalize_path(path))
            .map(|path| path == target)
            .unwrap_or(false)
    })
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

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
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

/// Reads the Windows PE file version shown by Explorer's Details tab.
/// This does not load or execute the DLL.
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
        let _ = path;
        None
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub product_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub product_version: Option<String>,
    pub original_filename: Option<String>,
}

/// Reads display metadata from the PE version resource without loading the DLL.
pub fn read_file_metadata(path: &Path) -> FileMetadata {
    #[cfg(windows)]
    {
        FileMetadata {
            title: read_version_string(path, "FileDescription"),
            description: read_version_string(path, "Comments"),
            product_name: read_version_string(path, "ProductName"),
            company: read_version_string(path, "CompanyName"),
            copyright: read_version_string(path, "LegalCopyright"),
            product_version: read_version_string(path, "ProductVersion"),
            original_filename: read_version_string(path, "OriginalFilename"),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
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
