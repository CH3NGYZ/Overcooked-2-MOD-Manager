//! Only the same interactive account and session may take over an update.
mod handoff;
mod token;

use self::token::{Identity, Token};
use crate::elevation::{ManagerProcess, owned_handle, process_image, raw, wide};
use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::io::OwnedHandle,
    path::Path,
    ptr::null_mut,
    sync::atomic::{AtomicI32, Ordering},
};
use winapi::{
    shared::winerror::{
        ERROR_ACCESS_DENIED, ERROR_ACCESS_DISABLED_BY_POLICY, ERROR_ELEVATION_REQUIRED,
        ERROR_PRIVILEGE_NOT_HELD,
    },
    um::{
        processthreadsapi::{OpenProcess, ResumeThread, STARTUPINFOW, TerminateProcess},
        winbase::{CREATE_SUSPENDED, CreateProcessWithTokenW},
        winnt::{PROCESS_QUERY_LIMITED_INFORMATION, TOKEN_DUPLICATE, TOKEN_QUERY},
        winuser::{GetShellWindow, GetWindowThreadProcessId},
    },
};

const FLAG: &str = "--oc2-ordinary-relaunch";
const HANDOFF_TIMEOUT_MS: u32 = 15_000;
static ORDINARY_LAUNCH_FAILURE: AtomicI32 = AtomicI32::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermissionRoute {
    User,
    RestartAsUser,
    Administrator,
}

pub(crate) fn permission_route() -> io::Result<PermissionRoute> {
    let current = Token::current()?.identity()?;
    if current.ordinary() {
        return Ok(PermissionRoute::User);
    }
    if !current.administrator() {
        return Err(denied(
            "The manager token is not an interactive update token",
        ));
    }
    let desktop = desktop_token(TOKEN_QUERY)?.identity()?;
    elevated_route(
        &current,
        &desktop,
        ORDINARY_LAUNCH_FAILURE.load(Ordering::Relaxed),
    )
}

fn elevated_route(
    current: &Identity,
    desktop: &Identity,
    launch_failure: i32,
) -> io::Result<PermissionRoute> {
    if !current.same_interactive_user(desktop) {
        return Err(denied(
            "The manager and desktop use different Windows identities",
        ));
    }
    if launch_failure != 0 && launch_failure != ERROR_ELEVATION_REQUIRED as i32 {
        return Err(io::Error::from_raw_os_error(launch_failure));
    }
    if desktop.administrator() || (desktop.ordinary() && launch_failure != 0) {
        Ok(PermissionRoute::Administrator)
    } else if desktop.ordinary() {
        Ok(PermissionRoute::RestartAsUser)
    } else {
        Err(denied(
            "The desktop token is not an interactive update token",
        ))
    }
}

fn desktop_token(access: u32) -> io::Result<Token> {
    let shell = unsafe { GetShellWindow() };
    if shell.is_null() {
        return Err(denied("No interactive desktop shell was found"));
    }
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(shell, &mut pid) } == 0 || pid == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = owned_handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })?;
    let token = Token::of(raw(&process), access)?;
    let mut rechecked_pid = 0;
    if unsafe { GetShellWindow() } != shell
        || unsafe { GetWindowThreadProcessId(shell, &mut rechecked_pid) } == 0
        || rechecked_pid != pid
    {
        return Err(denied(
            "The desktop shell changed during token verification",
        ));
    }
    Ok(token)
}

pub(crate) fn launch_unelevated() -> io::Result<()> {
    let current = Token::current()?.identity()?;
    if !current.administrator() {
        return Err(denied("An elevated interactive manager is required"));
    }
    let desktop = desktop_token(TOKEN_QUERY | TOKEN_DUPLICATE)
        .inspect_err(|error| remember_ordinary_launch_failure(error, false))?;
    let desktop_identity = desktop.identity()?;
    if elevated_route(
        &current,
        &desktop_identity,
        ORDINARY_LAUNCH_FAILURE.load(Ordering::Relaxed),
    )? != PermissionRoute::RestartAsUser
    {
        return Err(denied("No verified ordinary desktop token is available"));
    }
    let executable = std::env::current_exe()?;
    let owner = ManagerProcess::current()?;
    let nonce = uuid::Uuid::new_v4();
    let pipe = handoff::Server::create(&current.sid, nonce)?;
    let primary = desktop
        .duplicate_primary()
        .inspect_err(|error| remember_ordinary_launch_failure(error, false))?;
    let (child, thread, pid) = start_suspended(&primary, &executable, &owner, nonce)
        .inspect_err(|error| remember_ordinary_launch_failure(error, true))?;

    let result = (|| {
        let child_identity = Token::of(raw(&child), TOKEN_QUERY)?.identity()?;
        if !child_identity.ordinary()
            || !child_identity.same_interactive_user(&current)
            || !same_file::is_same_file(&executable, process_image(raw(&child))?)?
        {
            return Err(denied("The successor is not the matching ordinary manager"));
        }
        if unsafe { ResumeThread(raw(&thread)) } != 1 {
            return Err(io::Error::other(
                "The ordinary manager could not be started",
            ));
        }
        pipe.commit_to_child(&child, pid)
    })();
    if result.is_err() {
        unsafe { TerminateProcess(raw(&child), 1) };
    }
    result
}

fn remember_ordinary_launch_failure(error: &io::Error, starting_process: bool) {
    if let Some(code) = ordinary_launch_failure(error, starting_process) {
        ORDINARY_LAUNCH_FAILURE.store(code, Ordering::Relaxed);
    }
}

fn ordinary_launch_failure(error: &io::Error, starting_process: bool) -> Option<i32> {
    let code = error.raw_os_error()?;
    match code as u32 {
        ERROR_ELEVATION_REQUIRED if starting_process => Some(code),
        ERROR_ACCESS_DENIED | ERROR_ACCESS_DISABLED_BY_POLICY | ERROR_PRIVILEGE_NOT_HELD => {
            Some(code)
        }
        _ => None,
    }
}

fn start_suspended(
    token: &Token,
    executable: &Path,
    owner: &ManagerProcess,
    nonce: uuid::Uuid,
) -> io::Result<(OwnedHandle, OwnedHandle, u32)> {
    let arguments = format!("{FLAG} {} {} {nonce}", owner.pid, owner.created);
    let mut command = vec![u16::from(b'"')];
    let directory = executable
        .parent()
        .map(|path| wide(path.as_os_str()))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "The manager has no directory")
        })?;
    let executable = wide(executable.as_os_str());
    command.extend_from_slice(&executable[..executable.len() - 1]);
    command.extend("\" ".encode_utf16());
    command.extend(arguments.encode_utf16().chain([0]));
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of_val(&startup) as u32;
    let mut process = unsafe { std::mem::zeroed() };
    if unsafe {
        CreateProcessWithTokenW(
            token.handle(),
            0,
            executable.as_ptr(),
            command.as_mut_ptr(),
            CREATE_SUSPENDED,
            null_mut(),
            directory.as_ptr(),
            &mut startup,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((
        owned_handle(process.hProcess)?,
        owned_handle(process.hThread)?,
        process.dwProcessId,
    ))
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
    let owner_identity = Token::of(raw(&owner), TOKEN_QUERY)?.identity()?;
    if !owner_identity.administrator() || !owner_identity.same_interactive_user(&current) {
        return Err(denied(
            "The previous manager is not the matching administrator",
        ));
    }
    handoff::wait_for_commit(nonce, manager.pid, &owner)
}

fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

pub(crate) fn admin_identity() -> io::Result<String> {
    let identity = Token::current()?.identity()?;
    if !identity.administrator() {
        return Err(denied("The current process is not an administrator"));
    }
    Ok(identity.sid)
}

pub(crate) fn ensure_same_admin(owner: &OwnedHandle) -> io::Result<()> {
    let current = Token::current()?.identity()?;
    let owner = Token::of(raw(owner), TOKEN_QUERY)?.identity()?;
    if !current.administrator() || !owner.administrator() || !current.same_interactive_user(&owner)
    {
        return Err(denied(
            "The administrator identity does not match the owner",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
