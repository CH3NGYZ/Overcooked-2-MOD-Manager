//! The ordinary successor owns the pipe so Windows assigns its medium label.
use super::{
    HANDOFF_TIMEOUT_MS, denied, stage,
    token::{Identity, Token},
};
use crate::elevation::{creation_time, owned_handle, process_image, raw, wait_for, wide};
use std::{
    ffi::OsStr,
    io,
    os::windows::io::OwnedHandle,
    path::Path,
    ptr::null_mut,
    time::{Duration, Instant},
};
use winapi::{
    shared::{
        sddl::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        winerror::{ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, WAIT_TIMEOUT},
    },
    um::{
        fileapi::{CreateFileW, OPEN_EXISTING, ReadFile, WriteFile},
        handleapi::INVALID_HANDLE_VALUE,
        ioapiset::{CancelIoEx, GetOverlappedResult},
        minwinbase::{OVERLAPPED, SECURITY_ATTRIBUTES},
        namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW},
        processthreadsapi::OpenProcess,
        synchapi::{CreateEventW, WaitForMultipleObjects, WaitForSingleObject},
        winbase::{
            FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, GetNamedPipeClientProcessId,
            GetNamedPipeServerProcessId, LocalFree, PIPE_ACCESS_DUPLEX, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WAIT_OBJECT_0,
        },
        winnt::{
            FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_WRITE_DATA, HANDLE,
            PROCESS_QUERY_LIMITED_INFORMATION, PSECURITY_DESCRIPTOR, SYNCHRONIZE,
        },
    },
};

const READY: u8 = 1;
const PREPARE: u8 = 2;
const ACKNOWLEDGED: u8 = 3;
const COMMIT: u8 = 4;
const CLIENT_ACCESS: u32 = FILE_READ_ATTRIBUTES | FILE_READ_DATA | FILE_WRITE_DATA | SYNCHRONIZE;

struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(text: &str) -> io::Result<Self> {
        let descriptor = wide(OsStr::new(text));
        let mut allocated = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                u32::from(SDDL_REVISION_1),
                &mut allocated,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(allocated))
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

fn create_pipe(sid: &str, nonce: uuid::Uuid) -> io::Result<OwnedHandle> {
    let descriptor = Descriptor::from_sddl(&format!("D:P(A;;GA;;;{sid})"))?;
    let mut attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    pipe_handle(unsafe {
        CreateNamedPipeW(
            pipe_name(nonce).as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            32,
            32,
            HANDOFF_TIMEOUT_MS,
            &mut attrs,
        )
    })
}

pub(super) fn commit_to_child(
    nonce: uuid::Uuid,
    executable: &Path,
    owner: &Identity,
    not_before: u64,
    deadline: Instant,
) -> io::Result<()> {
    negotiate(nonce, deadline, |child| {
        validate_child(child, executable, owner, not_before)
    })
}

fn negotiate(
    nonce: uuid::Uuid,
    deadline: Instant,
    authenticate: impl FnOnce(&OwnedHandle) -> io::Result<u64>,
) -> io::Result<()> {
    let pipe = connect_to_child(nonce, deadline)
        .map_err(|error| stage("connect to ordinary manager", error))?;
    let pid = peer_pid(
        raw(&pipe),
        GetNamedPipeServerProcessId,
        "identify pipe server",
    )?;
    if pid == 0 || pid == std::process::id() {
        return Err(denied(
            "The update handoff server is not a distinct process",
        ));
    }
    let child = owned_handle(unsafe {
        OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
    })
    .map_err(|error| stage("open ordinary manager process", error))?;
    let created =
        authenticate(&child).map_err(|error| stage("authenticate ordinary manager", error))?;
    let link = Link::new(&pipe, Some(&child), deadline);
    let mut ready = [0; 9];
    link.receive(&mut ready)
        .map_err(|error| stage("receive ordinary manager READY", error))?;
    if ready != ready_message(created) {
        return Err(denied(
            "The update handoff READY did not identify the connected process",
        ));
    }
    link.send(&[PREPARE])
        .map_err(|error| stage("send handoff PREPARE", error))?;
    link.expect(ACKNOWLEDGED)
        .map_err(|error| stage("receive handoff ACK", error))?;
    link.send(&[COMMIT])
        .map_err(|error| stage("send handoff COMMIT", error))
}

fn validate_child(
    process: &OwnedHandle,
    executable: &Path,
    owner: &Identity,
    not_before: u64,
) -> io::Result<u64> {
    let handle = raw(process);
    let created = creation_time(handle)?;
    if created < not_before {
        return Err(denied(
            "The handoff client existed before this launch request",
        ));
    }
    if !same_file::is_same_file(executable, process_image(handle)?)? {
        return Err(denied("The ordinary manager executable does not match"));
    }
    let identity = Token::of(handle)?.identity()?;
    if !identity.ordinary() || !identity.same_interactive_user(owner) {
        return Err(denied(
            "The handoff client is not the matching ordinary manager",
        ));
    }
    Ok(created)
}

pub(super) fn ensure_process_alive(process: &OwnedHandle, message: &str) -> io::Result<()> {
    match unsafe { WaitForSingleObject(raw(process), 0) } {
        WAIT_TIMEOUT => Ok(()),
        WAIT_OBJECT_0 => Err(io::Error::other(message.to_owned())),
        _ => Err(io::Error::last_os_error()),
    }
}

pub(super) fn wait_for_commit(
    nonce: uuid::Uuid,
    sid: &str,
    owner_pid: u32,
    owner: &OwnedHandle,
    created: u64,
) -> io::Result<()> {
    let pipe =
        create_pipe(sid, nonce).map_err(|error| stage("create private handoff pipe", error))?;
    let deadline = Instant::now() + Duration::from_millis(u64::from(HANDOFF_TIMEOUT_MS));
    receive_owner_commit(&pipe, owner_pid, owner, created, deadline)?;
    wait_for(
        owner,
        HANDOFF_TIMEOUT_MS,
        "The previous manager did not exit",
    )
}

fn receive_owner_commit(
    pipe: &OwnedHandle,
    owner_pid: u32,
    owner: &OwnedHandle,
    created: u64,
    deadline: Instant,
) -> io::Result<()> {
    let link = connect_to_owner(pipe, owner_pid, owner, deadline)?;
    link.send(&ready_message(created))
        .map_err(|error| stage("send ordinary manager READY", error))?;
    link.expect(PREPARE)
        .map_err(|error| stage("receive handoff PREPARE", error))?;
    link.send(&[ACKNOWLEDGED])
        .map_err(|error| stage("send handoff ACK", error))?;
    Link::new(pipe, None, deadline)
        .expect(COMMIT)
        .map_err(|error| stage("receive handoff COMMIT", error))
}

fn connect_to_owner<'a>(
    pipe: &'a OwnedHandle,
    owner_pid: u32,
    owner: &'a OwnedHandle,
    deadline: Instant,
) -> io::Result<Link<'a>> {
    let link = Link::new(pipe, Some(owner), deadline);
    link.operation(|handle, overlap| {
        if unsafe { ConnectNamedPipe(handle, overlap) } != 0 {
            Ok(false)
        } else {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                Ok(false)
            } else {
                Err(error)
            }
        }
    })
    .map_err(|error| stage("accept previous manager", error))?;
    if peer_pid(
        raw(pipe),
        GetNamedPipeClientProcessId,
        "identify pipe client",
    )? != owner_pid
    {
        return Err(denied(
            "The update handoff pipe client is not the verified owner",
        ));
    }
    Ok(link)
}

fn connect_to_child(nonce: uuid::Uuid, deadline: Instant) -> io::Result<OwnedHandle> {
    let name = pipe_name(nonce);
    loop {
        if Instant::now() >= deadline {
            return Err(handshake_timeout());
        }
        let result = pipe_handle(unsafe {
            CreateFileW(
                name.as_ptr(),
                CLIENT_ACCESS,
                0,
                null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        });
        match result {
            Err(error) if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) => {
                std::thread::sleep(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(20)),
                );
            }
            result => return result,
        }
    }
}

fn ready_message(created: u64) -> [u8; 9] {
    let mut ready = [READY; 9];
    ready[1..].copy_from_slice(&created.to_le_bytes());
    ready
}

fn peer_pid(
    pipe: HANDLE,
    get_pid: unsafe extern "system" fn(HANDLE, *mut u32) -> i32,
    phase: &str,
) -> io::Result<u32> {
    let mut pid = 0;
    if unsafe { get_pid(pipe, &mut pid) } == 0 {
        return Err(stage(phase, io::Error::last_os_error()));
    }
    Ok(pid)
}

struct Link<'a> {
    pipe: &'a OwnedHandle,
    peer: Option<&'a OwnedHandle>,
    deadline: Instant,
}

impl<'a> Link<'a> {
    fn new(pipe: &'a OwnedHandle, peer: Option<&'a OwnedHandle>, deadline: Instant) -> Self {
        Self {
            pipe,
            peer,
            deadline,
        }
    }

    fn operation(
        &self,
        start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> io::Result<bool>,
    ) -> io::Result<u32> {
        if Instant::now() >= self.deadline {
            return Err(handshake_timeout());
        }
        if let Some(peer) = self.peer {
            ensure_process_alive(peer, "The update handoff peer exited")?;
        }
        let pipe = raw(self.pipe);
        let event = owned_handle(unsafe { CreateEventW(null_mut(), 1, 0, null_mut()) })?;
        let mut overlap: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlap.hEvent = raw(&event);
        let mut transferred = 0;
        match start(pipe, &mut overlap) {
            Ok(false) => return Ok(0),
            Ok(true) => {
                if unsafe { GetOverlappedResult(pipe, &mut overlap, &mut transferred, 0) } != 0 {
                    return Ok(transferred);
                }
                return Err(io::Error::last_os_error());
            }
            Err(error) if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) => {}
            Err(error) => return Err(error),
        }
        let timeout = self
            .deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(HANDOFF_TIMEOUT_MS)) as u32;
        let (status, event_status) = if let Some(peer) = self.peer {
            let handles = [raw(peer), raw(&event)];
            (
                unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) },
                WAIT_OBJECT_0 + 1,
            )
        } else {
            (
                unsafe { WaitForSingleObject(raw(&event), timeout) },
                WAIT_OBJECT_0,
            )
        };
        if status == event_status {
            if unsafe { GetOverlappedResult(pipe, &mut overlap, &mut transferred, 0) } != 0 {
                return Ok(transferred);
            }
            return Err(io::Error::last_os_error());
        }
        let error = match status {
            WAIT_OBJECT_0 => io::Error::other("The update handoff peer exited"),
            WAIT_TIMEOUT => handshake_timeout(),
            _ => io::Error::last_os_error(),
        };
        unsafe {
            CancelIoEx(pipe, &mut overlap);
            GetOverlappedResult(pipe, &mut overlap, &mut transferred, 1);
        }
        Err(error)
    }

    fn expect(&self, expected: u8) -> io::Result<()> {
        let mut message = [0];
        self.receive(&mut message)?;
        if message[0] != expected {
            return Err(denied("The update handoff peer sent an unexpected message"));
        }
        Ok(())
    }

    fn receive(&self, mut buffer: &mut [u8]) -> io::Result<()> {
        while !buffer.is_empty() {
            let transferred = self.operation(|pipe, overlap| {
                started(unsafe {
                    ReadFile(
                        pipe,
                        buffer.as_mut_ptr().cast(),
                        buffer.len() as u32,
                        null_mut(),
                        overlap,
                    )
                })
            })? as usize;
            if transferred == 0 || transferred > buffer.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Could not read update handoff message",
                ));
            }
            buffer = &mut buffer[transferred..];
        }
        Ok(())
    }

    fn send(&self, message: &[u8]) -> io::Result<()> {
        let transferred = self.operation(|pipe, overlap| {
            started(unsafe {
                WriteFile(
                    pipe,
                    message.as_ptr().cast(),
                    message.len() as u32,
                    null_mut(),
                    overlap,
                )
            })
        })?;
        if transferred as usize != message.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Could not send update handoff message",
            ));
        }
        Ok(())
    }
}

fn handshake_timeout() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "The update handoff timed out")
}

fn started(success: i32) -> io::Result<bool> {
    if success != 0 {
        Ok(true)
    } else {
        Err(io::Error::last_os_error())
    }
}

fn pipe_name(nonce: uuid::Uuid) -> Vec<u16> {
    wide(OsStr::new(&format!(r"\\.\pipe\OC2Update-Restart-{nonce}")))
}

fn pipe_handle(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    owned_handle(handle)
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod tests;
