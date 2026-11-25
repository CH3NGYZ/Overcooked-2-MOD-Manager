use super::{lock_file, matches_installed_version};
use crate::app_update::receipt::{Attempt, Outcome};
use crate::app_update::{deferred_nsis_args, installer_directory};
use crate::{file_metadata, shell_open};
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::os::windows::{ffi::OsStringExt, io::AsRawHandle, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use winapi::um::processthreadsapi::GetExitCodeProcess;
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::{INFINITE, WAIT_OBJECT_0};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_READ},
};

const NSIS_RESULT: &str = "update-result.txt";
const MSI_LOG: &str = "update-msi.log";
pub(super) const WINDOWS_RESULT: &str = "windows-install-result.txt";

pub(super) fn supports_background(path: &Path, kind: &str) -> bool {
    match kind {
        "exe" => {
            file_metadata::read_version_string(path, "OC2DeferredUpdate").as_deref() == Some("1")
        }
        "msi" => true,
        _ => false,
    }
}

pub(super) fn supports_admin(path: &Path, kind: &str, version: &str) -> bool {
    kind == "exe"
        && supports_background(path, kind)
        && file_metadata::read_version_string(path, "OC2AdminDeferredUpdate").as_deref()
            == Some("1")
        && matches_installed_version(path, version)
}

pub(super) fn matches_nsis_registration(source: &Path) -> bool {
    [
        winreg::enums::KEY_WOW64_32KEY,
        winreg::enums::KEY_WOW64_64KEY,
    ]
    .into_iter()
    .any(|view| {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall\OC2ModManager",
                KEY_READ | view,
            )
            .ok()
            .filter(|key| {
                key.get_value::<String, _>("MainBinaryName").as_deref().ok()
                    == Some("OC2ModManager.exe")
            })
            .is_some_and(|key| registered_directory(&key, source))
    })
}

fn registered_directory(key: &RegKey, source: &Path) -> bool {
    let Some(directory) = source.parent().and_then(Path::to_str) else {
        return false;
    };
    key.get_value::<String, _>("InstallLocation")
        .is_ok_and(|value| value.trim_matches('"').eq_ignore_ascii_case(directory))
}

pub(super) fn apply_and_observe(
    attempt: &Attempt,
    package: &Path,
    owner_pid: u32,
    administrator: bool,
) -> (Outcome, String) {
    let msi_log = attempt.work_path().join(MSI_LOG);
    let exit = match attempt.installer_type.as_str() {
        "exe" => run_nsis(&attempt.source, package, owner_pid, administrator),
        "msi" if !administrator => run_msi(&attempt.destination, package, &msi_log),
        _ => Err(io::Error::other("unsupported installer format")),
    };
    let nsis_report = if attempt.installer_type == "exe" {
        installer_directory(&attempt.destination)
            .map_err(io::Error::other)
            .and_then(|directory| {
                read_nsis_outcome(
                    &attempt.work_path().join(NSIS_RESULT),
                    &attempt.version,
                    directory,
                )
            })
    } else {
        Ok(None)
    };
    let target_is_installed = lock_file(&attempt.destination)
        .is_ok_and(|_lock| matches_installed_version(&attempt.destination, &attempt.version));
    let outcome = classify(
        &attempt.installer_type,
        exit.as_ref().ok().copied(),
        nsis_report.as_ref().map(Option::as_deref).map(|report| {
            if administrator && report.is_none() {
                Some("missing_receipt")
            } else {
                report
            }
        }),
        target_is_installed,
    );
    let details = format!(
        "version={} format={} exit={exit:?} nsis_report={nsis_report:?} destination_matches={target_is_installed} outcome={outcome:?}",
        attempt.version, attempt.installer_type,
    );
    (outcome, details)
}

pub(super) fn write_diagnostic(attempt: &Attempt, name: &str, details: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(attempt.work_path().join(name))
    {
        let _ = writeln!(file, "{details}");
    }
}

fn classify(
    kind: &str,
    exit: Option<u32>,
    nsis_report: Result<Option<&str>, &io::Error>,
    installed: bool,
) -> Outcome {
    match (kind, exit, nsis_report, installed) {
        ("msi", Some(1641 | 3010), _, _) => Outcome::RebootRequired,
        ("msi", Some(0), _, true) | ("exe", Some(0), Ok(None | Some("installed")), true) => {
            Outcome::Installed
        }
        _ => Outcome::Failed,
    }
}

fn run_nsis(source: &Path, package: &Path, owner_pid: u32, administrator: bool) -> io::Result<u32> {
    let directory = installer_directory(source).map_err(io::Error::other)?;
    let ordinary = deferred_nsis_args(directory, owner_pid);
    let mut command = if administrator {
        let work = package
            .parent()
            .ok_or_else(|| io::Error::other("Missing protected installer directory"))?;
        let mut command = protected_command(package, work)?;
        command.raw_arg("/OC2AdminDeferred");
        command
    } else {
        Command::new(package)
    };
    command
        .raw_arg(ordinary)
        .status()?
        .code()
        .map(|code| code as u32)
        .ok_or_else(|| io::Error::other("NSIS exit status missing"))
}

fn run_msi(destination: &Path, package: &Path, log: &Path) -> io::Result<u32> {
    use std::os::windows::fs::OpenOptionsExt;
    use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log)?;
    let _log = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(winapi::um::winbase::FILE_FLAG_OPEN_REPARSE_POINT)
        .open(log)?;
    super::check_type(&_log, false)?;
    let executable = system_msiexec()?;
    let args = msi_parameters(destination, package, log)?;
    let working_dir = executable
        .parent()
        .ok_or_else(|| io::Error::other("invalid system directory"))?;
    let child = shell_open::launch_elevated(&executable, &args, Some(working_dir))?;
    let handle = child.as_raw_handle().cast();
    if unsafe { WaitForSingleObject(handle, INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    let mut code = 0;
    if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}

pub(super) fn protected_command(executable: &Path, work: &Path) -> io::Result<Command> {
    let system = system_msiexec()?;
    let system = system
        .parent()
        .ok_or_else(|| io::Error::other("Missing Windows system directory"))?;
    let windows = system
        .parent()
        .ok_or_else(|| io::Error::other("Missing Windows directory"))?;
    let mut command = Command::new(executable);
    command
        .current_dir(work)
        .env_clear()
        .env("SystemRoot", windows)
        .env("WINDIR", windows)
        .env("PATH", system)
        .env("TEMP", work)
        .env("TMP", work);
    Ok(command)
}

fn msi_parameters(destination: &Path, package: &Path, log: &Path) -> io::Result<OsString> {
    let directory = installer_directory(destination).map_err(io::Error::other)?;
    let mut args = OsString::from("/i ");
    args.push(quote_path(package)?);
    args.push(" /quiet /norestart INSTALLDIR=");
    args.push(quote_path(directory)?);
    args.push(" /L*v ");
    args.push(quote_path(log)?);
    Ok(args)
}

fn quote_path(path: &Path) -> io::Result<OsString> {
    if !path.is_absolute() || path.to_string_lossy().contains(['"', '\r', '\n', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid MSI path",
        ));
    }
    use std::os::windows::ffi::OsStrExt;
    let mut wide = vec![u16::from(b'"')];
    wide.extend(path.as_os_str().encode_wide());
    let trailing_backslashes = wide
        .iter()
        .rev()
        .take_while(|unit| **unit == u16::from(b'\\'))
        .count();
    wide.extend(std::iter::repeat_n(u16::from(b'\\'), trailing_backslashes));
    wide.push(u16::from(b'"'));
    Ok(OsString::from_wide(&wide))
}

fn system_msiexec() -> io::Result<PathBuf> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }
    let mut path = vec![0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if length == 0 || length >= path.len() {
        return Err(io::Error::last_os_error());
    }
    Ok(PathBuf::from(OsString::from_wide(&path[..length])).join(OsStr::new("msiexec.exe")))
}

fn read_nsis_outcome(
    path: &Path,
    expected_version: &str,
    expected_directory: &Path,
) -> io::Result<Option<String>> {
    let file = match lock_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(32_770).read_to_end(&mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    parse_nsis_outcome(&bytes, expected_version, expected_directory)
        .map(Some)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid NSIS update receipt"))
}

fn parse_nsis_outcome(
    bytes: &[u8],
    expected_version: &str,
    expected_directory: &Path,
) -> Option<String> {
    if bytes.len() > 32_768 || bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<_> = bytes
        .chunks_exact(2)
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
        .collect();
    let record = String::from_utf16(&units).ok()?;
    let mut fields = record
        .trim_start_matches('\u{feff}')
        .trim_end_matches(['\r', '\n'])
        .split('|');
    let version = fields.next()?;
    let outcome = fields.next()?;
    let directory = Path::new(fields.next()?);
    if version != expected_version || fields.next()? != "restart=0" || fields.next().is_some() {
        return None;
    }
    if outcome == "installed"
        && (!directory.is_absolute()
            || !same_file::is_same_file(directory, expected_directory).unwrap_or(false))
    {
        return None;
    }
    Some(outcome.to_owned())
}

#[cfg(test)]
mod tests;
