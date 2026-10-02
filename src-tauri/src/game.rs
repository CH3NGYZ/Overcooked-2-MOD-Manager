use crate::models::{BepInExInfo, GameInfo, WinhttpState};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::System;

pub fn detect_game(config: &crate::models::AppConfig) -> GameInfo {
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
    GameInfo {
        path: path.map(|p| p.to_string_lossy().to_string()),
        steam_library: detected.map(|(library, _)| library.to_string_lossy().to_string()),
        game_exists,
        bepinex,
        kitchen_running,
        winhttp_state,
    }
}

pub fn find_game_path(config: &crate::models::AppConfig) -> Option<(PathBuf, PathBuf)> {
    if !config.game.path.is_empty() {
        let stored = PathBuf::from(&config.game.path);
        if stored.join("Overcooked2.exe").is_file() {
            return Some((stored.clone(), stored));
        }
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            if parent.join("Overcooked2.exe").is_file() {
                return Some((parent.to_path_buf(), parent.to_path_buf()));
            }
        }
    }
    steam_libraries().into_iter().find_map(|library| {
        let game = library.join("steamapps/common/Overcooked! 2");
        game.join("Overcooked2.exe")
            .is_file()
            .then_some((library, game))
    })
}

fn steam_libraries() -> Vec<PathBuf> {
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
    libraries
}

pub fn detect_bepinex(game: &Path, backup_root: Option<&Path>) -> BepInExInfo {
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
                    entry.file_name().to_string_lossy().starts_with("BepInEx.backup-")
                        && entry.path().is_dir()
                })
                .map(|entry| entry.path().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    backups.sort();
    BepInExInfo {
        installed,
        complete,
        version,
        version_source: source,
        backups,
    }
}

pub fn winhttp_state(game: &Path) -> WinhttpState {
    let enabled = game.join("winhttp.dll").is_file();
    let disabled = game.join("winhttp.dll.oc2mod.disabled").is_file();
    match (enabled, disabled) {
        (true, false) => WinhttpState::ModEnabled,
        (false, true) => WinhttpState::ModDisabled,
        (false, false) => WinhttpState::Missing,
        (true, true) => WinhttpState::Conflict,
    }
}

pub fn is_game_running(game: &Path) -> bool {
    let target = game.join("Overcooked2.exe");
    let target = normalize_path(target);
    let mut system = System::new_all();
    system.refresh_processes();
    system.processes().values().any(|process| {
        process
            .exe()
            .map(|path| normalize_path(path))
            .map(|path| path == target)
            .unwrap_or(false)
    })
}

pub fn ensure_game_stopped(game: &Path) -> Result<(), String> {
    if is_game_running(game) {
        Err("厨房正在运行，请先关闭厨房后重试".into())
    } else {
        Ok(())
    }
}

pub fn safe_game_path(game: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || rel
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("路径必须是游戏目录内的相对路径".into());
    }
    let root = normalize_path(game.to_path_buf());
    let candidate = normalize_path(root.join(rel));
    if !candidate.starts_with(&root) {
        return Err("路径逃逸游戏目录".into());
    }
    Ok(candidate)
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
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    pub original_filename: Option<String>,
}

/// Reads display metadata from the PE version resource without loading the DLL.
pub fn read_file_metadata(path: &Path) -> FileMetadata {
    #[cfg(windows)]
    {
        FileMetadata {
            product_name: read_version_string(path, "ProductName"),
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
    for language in ["040904b0", "040904e4", "080904b0", "000004b0", "000004e4"] {
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
        assert!(safe_game_path(&root, "BepInEx/plugins/mod.dll").is_ok());
    }

    #[test]
    fn winhttp_state_machine() {
        let root = std::env::temp_dir().join("oc2-winhttp-test");
        let _ = fs::create_dir_all(&root);
        let _ = fs::remove_file(root.join("winhttp.dll"));
        let _ = fs::remove_file(root.join("winhttp.dll.oc2mod.disabled"));
        assert_eq!(winhttp_state(&root), WinhttpState::Missing);
        fs::write(root.join("winhttp.dll"), []).unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
        let _ = fs::remove_file(root.join("winhttp.dll"));
        fs::write(root.join("winhttp.dll.oc2mod.disabled"), []).unwrap();
        assert_eq!(winhttp_state(&root), WinhttpState::ModDisabled);
    }
}
