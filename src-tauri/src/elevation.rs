//! The elevated child waits before acquiring the ordinary Tauri single-instance mutex.
use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::null_mut,
};
use winapi::{
    shared::{
        minwindef::FILETIME,
        winerror::{ERROR_ALREADY_EXISTS, WAIT_TIMEOUT},
    },
    um::{
        combaseapi::CoUninitialize,
        errhandlingapi::SetLastError,
        objbase::CoInitialize,
        processthreadsapi::{GetCurrentProcess, GetProcessTimes, OpenProcess},
        shellapi::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            ShellExecuteExW,
        },
        synchapi::{
            CreateEventW, OpenEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
        },
        winbase::{QueryFullProcessImageNameW, WAIT_OBJECT_0},
        winnt::{EVENT_MODIFY_STATE, HANDLE, PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZE},
        winuser::SW_SHOWNORMAL,
    },
};

const FLAG: &str = "--oc2-elevated-relaunch";
const HANDOFF_TIMEOUT_MS: u32 = 15_000;

struct ManagerProcess {
    pid: u32,
    created: u64,
}

impl ManagerProcess {
    fn current() -> io::Result<Self> {
        Ok(Self {
            pid: std::process::id(),
            created: creation_time(unsafe { GetCurrentProcess() })?,
        })
    }

    fn parse(pid: Option<OsString>, created: Option<OsString>) -> io::Result<Self> {
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid administrator restart request",
            )
        };
        let pid = pid
            .and_then(|value| value.to_str()?.parse::<u32>().ok())
            .filter(|pid| *pid != 0 && *pid != std::process::id())
            .ok_or_else(invalid)?;
        let created = created
            .and_then(|value| value.to_str()?.parse::<u64>().ok())
            .filter(|created| *created != 0)
            .ok_or_else(invalid)?;
        Ok(Self { pid, created })
    }

    fn open(self, executable: &Path) -> io::Result<OwnedHandle> {
        // Keep the same handle through the identity checks and wait so a reused
        // PID cannot make this child wait for a different process.
        let owner = owned_handle(unsafe {
            OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, self.pid)
        })?;
        let process = raw(&owner);
        if creation_time(process)? != self.created
            || !same_file::is_same_file(executable, process_image(process)?)?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The restart owner does not match this manager instance",
            ));
        }
        Ok(owner)
    }
}

/// Session-local events, with the OS default DACL. The requester creates both
/// before launch and the child only opens those exact, per-attempt names.
struct HandoffEvents {
    ready: OwnedHandle,
    proceed: OwnedHandle,
}

impl HandoffEvents {
    fn named(token: uuid::Uuid, create: bool) -> io::Result<Self> {
        fn event(name: String, create: bool, access: u32) -> io::Result<OwnedHandle> {
            let name = wide(OsStr::new(&name));
            if !create {
                return owned_handle(unsafe { OpenEventW(access, 0, name.as_ptr()) });
            }
            let (handle, already_exists) = unsafe {
                SetLastError(0);
                let handle = CreateEventW(null_mut(), 1, 0, name.as_ptr());
                let already_exists = !handle.is_null()
                    && io::Error::last_os_error().raw_os_error()
                        == Some(ERROR_ALREADY_EXISTS as i32);
                (handle, already_exists)
            };
            let handle = owned_handle(handle)?;
            if already_exists {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Restart event already exists",
                ));
            }
            Ok(handle)
        }

        let prefix = format!(r"Local\OC2ModManager-Restart-{token}");
        Ok(Self {
            ready: event(format!("{prefix}-Ready"), create, EVENT_MODIFY_STATE)?,
            proceed: event(format!("{prefix}-Proceed"), create, SYNCHRONIZE)?,
        })
    }

    fn commit_when_ready(&self, child: HANDLE, timeout_ms: u32) -> io::Result<()> {
        // If both are signaled when we wake, a dead child wins and we stay open.
        let handles = [child, raw(&self.ready)];
        match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout_ms) } {
            WAIT_OBJECT_0 => Err(io::Error::other(
                "The administrator process exited before it was ready",
            )),
            status if status == WAIT_OBJECT_0 + 1 => signal(&self.proceed),
            WAIT_TIMEOUT => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "The administrator process was not ready",
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn yield_until_owner_exits(&self, owner: &OwnedHandle, timeout_ms: u32) -> io::Result<()> {
        signal(&self.ready)?;
        // Never continue after an abandoned/expired attempt, even if the old
        // instance is later closed by the user for an unrelated reason.
        wait_for(
            &self.proceed,
            timeout_ms,
            "The previous manager did not commit the restart",
        )?;
        wait_for(owner, timeout_ms, "The previous manager did not exit")
    }
}

fn owned_handle(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.cast()) })
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle().cast()
}

fn signal(event: &OwnedHandle) -> io::Result<()> {
    if unsafe { SetEvent(raw(event)) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn wait_for(handle: &OwnedHandle, timeout_ms: u32, message: &str) -> io::Result<()> {
    match unsafe { WaitForSingleObject(raw(handle), timeout_ms) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(io::Error::new(io::ErrorKind::TimedOut, message)),
        _ => Err(io::Error::last_os_error()),
    }
}

fn creation_time(process: HANDLE) -> io::Result<u64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    if unsafe { GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

fn process_image(process: HANDLE) -> io::Result<PathBuf> {
    let mut name = vec![0; 32_768];
    let mut length = name.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(OsString::from_wide(&name[..length as usize]).into())
}

fn wait_for_relaunch(mut args: impl Iterator<Item = OsString>, elevated: bool) -> io::Result<()> {
    if !elevated {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The restart did not receive administrator permission",
        ));
    }
    let manager = ManagerProcess::parse(args.next(), args.next())?;
    let token = args
        .next()
        .and_then(|value| uuid::Uuid::parse_str(value.to_str()?).ok())
        .filter(|token| token.get_version_num() == 4 && args.next().is_none())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid restart token"))?;
    let owner = manager.open(&std::env::current_exe()?)?;
    HandoffEvents::named(token, false)?.yield_until_owner_exits(&owner, HANDOFF_TIMEOUT_MS)
}

pub(crate) fn complete_handoff() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new(FLAG)) {
        return true;
    }
    match wait_for_relaunch(args, crate::is_elevated()) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("OC2 MOD Manager could not restart as administrator: {error}");
            false
        }
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

pub(crate) fn launch() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let owner = ManagerProcess::current()?;
    let token = uuid::Uuid::new_v4();
    let events = HandoffEvents::named(token, true)?;
    let arguments = format!("{FLAG} {} {} {token}", owner.pid, owner.created);
    let verb = wide(OsStr::new("runas"));
    let executable = wide(executable.as_os_str());
    let arguments = wide(OsStr::new(&arguments));
    // An accepted UAC prompt alone is not proof that a new process was created.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of_val(&info) as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    info.lpVerb = verb.as_ptr();
    info.lpFile = executable.as_ptr();
    info.lpParameters = arguments.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    // ShellExecuteEx may use COM on the blocking launch thread.
    struct ShellApartment;
    impl Drop for ShellApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let status = unsafe { CoInitialize(null_mut()) };
    if status < 0 {
        return Err(io::Error::from_raw_os_error(status));
    }
    let _apartment = ShellApartment;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("The administrator process did not start"));
    }
    let process = owned_handle(info.hProcess)?;
    events.commit_when_ready(raw(&process), HANDOFF_TIMEOUT_MS)
}

#[cfg(test)]
mod tests;
