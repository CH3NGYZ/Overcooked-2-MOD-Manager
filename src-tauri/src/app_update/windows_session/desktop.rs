//! Dispatch through the verified desktop's Explorer automation object.

use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::HWND,
        System::{
            Com::{
                CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch, IServiceProvider,
            },
            Variant::VARIANT,
        },
        UI::{
            Shell::{
                IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellWindows,
                SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH,
                ShellWindows,
            },
            WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId, SW_SHOWNORMAL},
        },
    },
    core::{BSTR, Interface},
};

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn com<T>(stage: &str, result: windows::core::Result<T>) -> io::Result<T> {
    result.map_err(|error| {
        io::Error::other(format!(
            "Explorer update launch: {stage} (HRESULT 0x{:08X})",
            error.code().0 as u32
        ))
    })
}

fn automation_string(value: &OsStr) -> io::Result<BSTR> {
    let units: Vec<_> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "An Explorer update launch argument contains a NUL",
        ));
    }
    Ok(BSTR::from_wide(&units))
}

fn verify_window(window: HWND, shell_pid: u32, stage: &str) -> io::Result<()> {
    let mut pid = 0;
    if !window.is_invalid()
        && unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) } != 0
        && pid == shell_pid
    {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("Explorer update launch: {stage} did not match the verified desktop process"),
    ))
}

/// `Ok` only means Explorer accepted the request; the caller must authenticate the successor.
pub(super) fn launch(executable: &Path, arguments: &OsStr, shell_pid: u32) -> io::Result<()> {
    let directory = executable.parent().filter(|_| executable.is_absolute());
    let directory = directory.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Explorer update launch requires an absolute executable path with a parent directory",
        )
    })?;
    let file = automation_string(executable.as_os_str())?;
    let arguments = VARIANT::from(automation_string(arguments)?);
    let directory = VARIANT::from(automation_string(directory.as_os_str())?);
    let operation = VARIANT::from("open");
    let show = VARIANT::from(SW_SHOWNORMAL.0);

    verify_window(unsafe { GetShellWindow() }, shell_pid, "current shell")?;
    com("initialize STA", unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()
    })?;
    let _apartment = Apartment;
    let shell_windows: IShellWindows = com("create ShellWindows", unsafe {
        CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)
    })?;
    let empty = VARIANT::default();
    let mut desktop_window = 0;
    let desktop = com("find desktop window", unsafe {
        shell_windows.FindWindowSW(
            &empty,
            &empty,
            SWC_DESKTOP,
            &mut desktop_window,
            SWFO_NEEDDISPATCH,
        )
    })?;
    let desktop_window = HWND(desktop_window as isize as *mut _);
    verify_window(desktop_window, shell_pid, "desktop automation window")?;

    let services: IServiceProvider = com("get desktop services", desktop.cast())?;
    let browser = com("query top-level shell browser", unsafe {
        services.QueryService::<IShellBrowser>(&SID_STopLevelBrowser)
    })?;
    let view = com("query active desktop view", unsafe {
        browser.QueryActiveShellView()
    })?;
    let view_window = com("get desktop view window", unsafe { view.GetWindow() })?;
    verify_window(view_window, shell_pid, "active desktop view")?;
    let background = com("get desktop background IDispatch", unsafe {
        view.GetItemObject::<IDispatch>(SVGIO_BACKGROUND)
    })?;
    let background: IShellFolderViewDual =
        com("query desktop folder view interface", background.cast())?;
    let application = com("get desktop application", unsafe {
        background.Application()
    })?;
    let dispatch: IShellDispatch2 = com("get desktop execution interface", application.cast())?;

    verify_window(desktop_window, shell_pid, "desktop before dispatch")?;
    verify_window(
        unsafe { GetShellWindow() },
        shell_pid,
        "shell before dispatch",
    )?;
    com("dispatch open", unsafe {
        dispatch.ShellExecute(&file, &arguments, &directory, &operation, &show)
    })
}
