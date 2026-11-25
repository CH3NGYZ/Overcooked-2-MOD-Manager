use super::{HANDOFF_TIMEOUT_MS, denied};
use crate::elevation::{owned_handle, raw, wait_for, wide};
use std::{ffi::OsStr, io, os::windows::io::OwnedHandle, ptr::null_mut, time::Instant};
use winapi::{
    shared::{
        sddl::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        winerror::{ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, WAIT_TIMEOUT},
    },
    um::{
        fileapi::{CreateFileW, OPEN_EXISTING, ReadFile, WriteFile},
        handleapi::INVALID_HANDLE_VALUE,
        ioapiset::{CancelIoEx, GetOverlappedResult},
        minwinbase::{OVERLAPPED, SECURITY_ATTRIBUTES},
        namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW},
        synchapi::{CreateEventW, WaitForMultipleObjects, WaitForSingleObject},
        winbase::{
            FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, GetNamedPipeClientProcessId,
            GetNamedPipeServerProcessId, LocalFree, PIPE_ACCESS_DUPLEX, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WAIT_OBJECT_0,
        },
        winnt::{
            FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_WRITE_DATA, HANDLE, PSECURITY_DESCRIPTOR,
            SYNCHRONIZE,
        },
    },
};

const READY: u8 = 1;
const COMMIT: u8 = 2;
const ACKNOWLEDGED: u8 = 3;
const CLIENT_ACCESS: u32 = FILE_READ_ATTRIBUTES | FILE_READ_DATA | FILE_WRITE_DATA | SYNCHRONIZE;

pub(super) struct Server(OwnedHandle);

impl Server {
    pub(super) fn create(sid: &str, nonce: uuid::Uuid) -> io::Result<Self> {
        let descriptor = wide(OsStr::new(&format!(
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x{CLIENT_ACCESS:08x};;;{sid})S:(ML;;NW;;;ME)"
        )));
        let mut allocated: PSECURITY_DESCRIPTOR = null_mut();
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
        let mut attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: allocated,
            bInheritHandle: 0,
        };
        let handle = pipe_handle(unsafe {
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
        });
        unsafe { LocalFree(allocated) };
        Ok(Self(handle?))
    }

    pub(super) fn commit_to_child(&self, child: &OwnedHandle, pid: u32) -> io::Result<()> {
        let link = Link::new(&self.0, child);
        link.operation(|pipe, overlap| {
            if unsafe { ConnectNamedPipe(pipe, overlap) } != 0 {
                Ok(false)
            } else {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                    Ok(false)
                } else {
                    Err(error)
                }
            }
        })?;
        verify_peer(raw(&self.0), pid, GetNamedPipeClientProcessId)?;
        link.expect(READY)?;
        link.send(COMMIT)?;
        link.expect(ACKNOWLEDGED)?;
        ensure_child_alive(child)
    }
}

fn ensure_child_alive(child: &OwnedHandle) -> io::Result<()> {
    match unsafe { WaitForSingleObject(raw(child), 0) } {
        WAIT_TIMEOUT => Ok(()),
        WAIT_OBJECT_0 => Err(io::Error::other(
            "The ordinary manager exited during the handoff",
        )),
        _ => Err(io::Error::last_os_error()),
    }
}

pub(super) fn wait_for_commit(
    nonce: uuid::Uuid,
    owner_pid: u32,
    owner: &OwnedHandle,
) -> io::Result<()> {
    let pipe = pipe_handle(unsafe {
        CreateFileW(
            pipe_name(nonce).as_ptr(),
            CLIENT_ACCESS,
            0,
            null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            null_mut(),
        )
    })?;
    verify_peer(raw(&pipe), owner_pid, GetNamedPipeServerProcessId)?;
    let link = Link::new(&pipe, owner);
    link.send(READY)?;
    link.expect(COMMIT)?;
    link.send(ACKNOWLEDGED)?;
    wait_for(
        owner,
        HANDOFF_TIMEOUT_MS,
        "The previous manager did not exit",
    )
}

fn verify_peer(
    pipe: HANDLE,
    expected: u32,
    get_pid: unsafe extern "system" fn(HANDLE, *mut u32) -> i32,
) -> io::Result<()> {
    let mut pid = 0;
    if unsafe { get_pid(pipe, &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if pid != expected {
        return Err(denied(
            "The update handoff pipe peer is not the expected process",
        ));
    }
    Ok(())
}

struct Link<'a> {
    pipe: &'a OwnedHandle,
    peer: &'a OwnedHandle,
    started: Instant,
}

impl<'a> Link<'a> {
    fn new(pipe: &'a OwnedHandle, peer: &'a OwnedHandle) -> Self {
        Self {
            pipe,
            peer,
            started: Instant::now(),
        }
    }

    fn operation(
        &self,
        start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> io::Result<bool>,
    ) -> io::Result<u32> {
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
        let timeout = HANDOFF_TIMEOUT_MS.saturating_sub(
            self.started
                .elapsed()
                .as_millis()
                .min(u128::from(HANDOFF_TIMEOUT_MS)) as u32,
        );
        let handles = [raw(self.peer), raw(&event)];
        let status = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) };
        if status == WAIT_OBJECT_0 + 1 {
            if unsafe { GetOverlappedResult(pipe, &mut overlap, &mut transferred, 0) } != 0 {
                return Ok(transferred);
            }
            return Err(io::Error::last_os_error());
        }
        let error = match status {
            WAIT_OBJECT_0 => io::Error::other("The update handoff peer exited"),
            WAIT_TIMEOUT => io::Error::new(io::ErrorKind::TimedOut, "The update handoff timed out"),
            _ => io::Error::last_os_error(),
        };
        unsafe {
            CancelIoEx(pipe, &mut overlap);
            GetOverlappedResult(pipe, &mut overlap, &mut transferred, 1);
        }
        Err(error)
    }

    fn expect(&self, expected: u8) -> io::Result<()> {
        let mut message = 0u8;
        let transferred = self.operation(|pipe, overlap| {
            started(unsafe {
                ReadFile(
                    pipe,
                    (&mut message as *mut u8).cast(),
                    1,
                    null_mut(),
                    overlap,
                )
            })
        })?;
        if transferred != 1 || message != expected {
            return Err(denied("The update handoff peer sent an unexpected message"));
        }
        Ok(())
    }

    fn send(&self, message: u8) -> io::Result<()> {
        let transferred = self.operation(|pipe, overlap| {
            started(unsafe {
                WriteFile(pipe, (&message as *const u8).cast(), 1, null_mut(), overlap)
            })
        })?;
        if transferred != 1 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Could not send update handoff message",
            ));
        }
        Ok(())
    }
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
mod tests {
    use super::*;
    use crate::{app_update::windows_session::token::Token, elevation::ManagerProcess};
    use std::process::{Child, Command};

    const NONCE: &str = "OC2_TEST_UPDATE_HANDOFF_NONCE";

    struct Fixture(Child);

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn fixture(nonce: uuid::Uuid) -> Fixture {
        let owner = ManagerProcess::current().unwrap();
        Fixture(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "app_update::windows_session::handoff::tests::pipe_fixture",
                ])
                .env(NONCE, nonce.to_string())
                .env("OC2_TEST_UPDATE_HANDOFF_PID", owner.pid.to_string())
                .env(
                    "OC2_TEST_UPDATE_HANDOFF_CREATION",
                    owner.created.to_string(),
                )
                .spawn()
                .unwrap(),
        )
    }

    #[test]
    fn live_peer_authenticates_and_acknowledges_commit() {
        let nonce = uuid::Uuid::new_v4();
        let identity = Token::current().unwrap().identity().unwrap();
        let server = Server::create(&identity.sid, nonce).unwrap();
        assert!(Server::create(&identity.sid, nonce).is_err());
        let mut child = fixture(nonce);
        let handle = owned_handle(unsafe {
            winapi::um::processthreadsapi::OpenProcess(SYNCHRONIZE, 0, child.0.id())
        })
        .unwrap();
        server.commit_to_child(&handle, child.0.id()).unwrap();
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn finished_child_is_not_eligible_for_handoff_even_after_ack() {
        let event = owned_handle(unsafe { CreateEventW(null_mut(), 1, 0, null_mut()) }).unwrap();
        ensure_child_alive(&event).unwrap();
        assert_ne!(unsafe { winapi::um::synchapi::SetEvent(raw(&event)) }, 0);
        assert!(ensure_child_alive(&event).is_err());
    }

    #[test]
    fn live_peer_with_wrong_pid_is_not_committed() {
        let nonce = uuid::Uuid::new_v4();
        let identity = Token::current().unwrap().identity().unwrap();
        let server = Server::create(&identity.sid, nonce).unwrap();
        let child = fixture(nonce);
        let handle = owned_handle(unsafe {
            winapi::um::processthreadsapi::OpenProcess(SYNCHRONIZE, 0, child.0.id())
        })
        .unwrap();
        let error = server
            .commit_to_child(&handle, std::process::id())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    #[ignore = "spawned by native update handoff tests"]
    fn pipe_fixture() {
        let nonce = std::env::var(NONCE).unwrap().parse().unwrap();
        let owner = ManagerProcess::parse(
            std::env::var_os("OC2_TEST_UPDATE_HANDOFF_PID"),
            std::env::var_os("OC2_TEST_UPDATE_HANDOFF_CREATION"),
        )
        .unwrap();
        let process = owner.open(&std::env::current_exe().unwrap()).unwrap();
        wait_for_commit(nonce, owner.pid, &process).unwrap();
    }
}
