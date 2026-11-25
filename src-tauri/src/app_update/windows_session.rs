//! Only the same ordinary interactive account and session may take over an update.
mod desktop;
mod handoff;
mod token;

use self::token::{Identity, Token};
use crate::elevation::{ManagerProcess, owned_handle, raw};
use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::io::OwnedHandle,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use winapi::um::{
    processthreadsapi::OpenProcess,
    winnt::{PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZE},
    winuser::{GetShellWindow, GetWindowThreadProcessId},
};

const FLAG: &str = "--oc2-ordinary-relaunch";
const HANDOFF_TIMEOUT_MS: u32 = 15_000;
static DESKTOP_CALL_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermissionRoute {
    User,
    RestartAsUser,
    ManualDownload,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RelaunchError {
    #[error("{0}")]
    Unavailable(#[source] io::Error),
    #[error("{0}")]
    Failed(#[source] io::Error),
}

pub(crate) fn permission_route() -> io::Result<PermissionRoute> {
    let current = Token::current()?.identity()?;
    if current.ordinary() {
        return Ok(PermissionRoute::User);
    }
    if !current.administrator() {
        return Ok(PermissionRoute::ManualDownload);
    }
    let Some(desktop) = Desktop::open()? else {
        return Ok(PermissionRoute::ManualDownload);
    };
    Ok(elevated_route(&current, &desktop.identity))
}

fn elevated_route(current: &Identity, desktop: &Identity) -> PermissionRoute {
    if current.administrator() && desktop.ordinary() && current.same_interactive_user(desktop) {
        PermissionRoute::RestartAsUser
    } else {
        PermissionRoute::ManualDownload
    }
}

struct Desktop {
    pid: u32,
    process: OwnedHandle,
    identity: Identity,
}

impl Desktop {
    fn open() -> io::Result<Option<Self>> {
        let shell = unsafe { GetShellWindow() };
        if shell.is_null() {
            return Ok(None);
        }
        let mut pid = 0;
        if unsafe { GetWindowThreadProcessId(shell, &mut pid) } == 0 || pid == 0 {
            return Err(stage("read desktop process ID", io::Error::last_os_error()));
        }
        let process = owned_handle(unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid)
        })
        .map_err(|error| stage("open desktop process", error))?;
        let identity = Token::of(raw(&process))
            .and_then(|token| token.identity())
            .map_err(|error| stage("verify desktop process identity", error))?;
        handoff::ensure_process_alive(&process, "The desktop process exited")?;
        let mut rechecked = 0;
        if unsafe { GetShellWindow() } != shell
            || unsafe { GetWindowThreadProcessId(shell, &mut rechecked) } == 0
            || rechecked != pid
        {
            return Err(denied("The desktop shell changed during verification"));
        }
        Ok(Some(Self {
            pid,
            process,
            identity,
        }))
    }
}

pub(crate) fn launch_unelevated() -> Result<(), RelaunchError> {
    use RelaunchError::{Failed, Unavailable};

    let current = Token::current()
        .and_then(|token| token.identity())
        .map_err(|error| Unavailable(stage("verify current process identity", error)))?;
    if !current.administrator() {
        return Err(Unavailable(denied(
            "An elevated interactive manager is required",
        )));
    }
    let desktop = Desktop::open()
        .map_err(Unavailable)?
        .ok_or_else(|| Unavailable(denied("No interactive desktop shell is available")))?;
    if elevated_route(&current, &desktop.identity) != PermissionRoute::RestartAsUser {
        return Err(Unavailable(denied(
            "No matching ordinary desktop is available",
        )));
    }

    let executable = std::env::current_exe()
        .map_err(|error| Unavailable(stage("read current executable", error)))?;
    let owner = ManagerProcess::current()
        .map_err(|error| Unavailable(stage("identify current manager", error)))?;
    let nonce = uuid::Uuid::new_v4();
    let arguments = OsString::from(format!("{FLAG} {} {} {nonce}", owner.pid, owner.created));
    let not_before = filetime_now().map_err(Unavailable)?;
    let deadline = Instant::now() + Duration::from_millis(u64::from(HANDOFF_TIMEOUT_MS));
    let invocation = executable.clone();
    if DESKTOP_CALL_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(Failed(io::Error::new(
            io::ErrorKind::WouldBlock,
            "The previous Explorer request is still active",
        )));
    }
    let (result, reply) = mpsc::sync_channel(1);
    if let Err(error) = std::thread::Builder::new()
        .name("oc2-update-desktop".into())
        .spawn(move || {
            struct ReleaseDesktopCall;
            impl Drop for ReleaseDesktopCall {
                fn drop(&mut self) {
                    DESKTOP_CALL_ACTIVE.store(false, Ordering::Release);
                }
            }
            let guard = ReleaseDesktopCall;
            let launched = handoff::ensure_process_alive(
                &desktop.process,
                "The verified desktop process exited",
            )
            .and_then(|()| desktop::launch(&invocation, &arguments, desktop.pid));
            drop(guard);
            let _ = result.send(launched);
        })
    {
        DESKTOP_CALL_ACTIVE.store(false, Ordering::Release);
        return Err(Unavailable(stage("start Explorer STA thread", error)));
    }
    match reply.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => result.map_err(Unavailable)?,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            return Err(Failed(io::Error::new(
                io::ErrorKind::TimedOut,
                "The Explorer launch request timed out",
            )));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err(Failed(io::Error::other(
                "The Explorer launch thread stopped",
            )));
        }
    }
    handoff::commit_to_child(nonce, &executable, &current, not_before, deadline).map_err(Failed)
}

fn filetime_now() -> io::Result<u64> {
    const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    u64::try_from(elapsed.as_nanos() / 100)
        .ok()
        .and_then(|ticks| ticks.checked_add(UNIX_EPOCH_FILETIME))
        .ok_or_else(|| io::Error::other("System time is outside the process timestamp range"))
}

pub(crate) fn complete_unelevated_handoff() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new(FLAG)) {
        return true;
    }
    match wait_for_owner(args) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("OC2 MOD Manager could not restart with ordinary permissions: {error}");
            false
        }
    }
}

fn wait_for_owner(mut args: impl Iterator<Item = OsString>) -> io::Result<()> {
    let current = Token::current()?.identity()?;
    if !current.ordinary() {
        return Err(denied("The successor did not receive ordinary permissions"));
    }
    let manager = ManagerProcess::parse(args.next(), args.next())?;
    let nonce = args
        .next()
        .and_then(|value| uuid::Uuid::parse_str(value.to_str()?).ok())
        .filter(|uuid| uuid.get_version_num() == 4 && args.next().is_none())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid restart nonce"))?;
    let owner = manager.open(&std::env::current_exe()?)?;
    let owner_identity = Token::of(raw(&owner))?.identity()?;
    if !owner_identity.administrator() || !owner_identity.same_interactive_user(&current) {
        return Err(denied(
            "The previous manager is not the matching administrator",
        ));
    }
    handoff::wait_for_commit(
        nonce,
        &current.sid,
        manager.pid,
        &owner,
        ManagerProcess::current()?.created,
    )
}

fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

fn stage(stage: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{stage}: {error}"))
}

#[cfg(test)]
mod tests;
