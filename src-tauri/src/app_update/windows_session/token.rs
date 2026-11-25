use crate::elevation::{owned_handle, raw};
use std::{io, mem::size_of, os::windows::io::OwnedHandle, ptr::null_mut};
use winapi::{
    shared::{sddl::ConvertSidToStringSidW, winerror::ERROR_INSUFFICIENT_BUFFER},
    um::{
        processthreadsapi::{GetCurrentProcess, OpenProcessToken},
        securitybaseapi::{
            CreateWellKnownSid, EqualSid, GetSidSubAuthority, GetSidSubAuthorityCount,
            GetTokenInformation, IsValidSid,
        },
        winbase::LocalFree,
        winnt::{
            HANDLE, PSID, SE_GROUP_ENABLED, SE_GROUP_USE_FOR_DENY_ONLY,
            SECURITY_MANDATORY_HIGH_RID, SECURITY_MANDATORY_MEDIUM_RID,
            SECURITY_MANDATORY_SYSTEM_RID, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, TOKEN_GROUPS,
            TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TokenElevation, TokenGroups, TokenIntegrityLevel,
            TokenSessionId, TokenUIAccess, TokenUser, WinBuiltinAdministratorsSid,
        },
    },
};

#[derive(Debug)]
pub(super) struct Identity {
    pub(super) sid: String,
    pub(super) session: u32,
    pub(super) integrity: u32,
    pub(super) elevated: bool,
    pub(super) administrator_group: bool,
    pub(super) ui_access: bool,
}

impl Identity {
    pub(super) fn same_interactive_user(&self, other: &Self) -> bool {
        self.session != 0 && self.session == other.session && self.sid == other.sid
    }

    pub(super) fn ordinary(&self) -> bool {
        self.session != 0
            && self.integrity == SECURITY_MANDATORY_MEDIUM_RID
            && !self.elevated
            && !self.administrator_group
            && !self.ui_access
    }

    pub(super) fn administrator(&self) -> bool {
        self.session != 0
            && (SECURITY_MANDATORY_HIGH_RID..SECURITY_MANDATORY_SYSTEM_RID)
                .contains(&self.integrity)
            && self.elevated
            && self.administrator_group
            && !self.ui_access
    }
}

pub(super) struct Token(OwnedHandle);

impl Token {
    pub(super) fn current() -> io::Result<Self> {
        Self::of(unsafe { GetCurrentProcess() })
    }

    pub(super) fn of(process: HANDLE) -> io::Result<Self> {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(owned_handle(token)?))
    }

    fn handle(&self) -> HANDLE {
        raw(&self.0)
    }

    fn dword(&self, class: TOKEN_INFORMATION_CLASS) -> io::Result<u32> {
        let mut value = 0u32;
        self.read_info(class, std::slice::from_mut(&mut value))?;
        Ok(value)
    }

    fn read_info<T>(&self, class: TOKEN_INFORMATION_CLASS, data: &mut [T]) -> io::Result<()> {
        let mut length = 0;
        if unsafe {
            GetTokenInformation(
                self.handle(),
                class,
                data.as_mut_ptr().cast(),
                std::mem::size_of_val(data) as u32,
                &mut length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn administrator_group(&self) -> io::Result<bool> {
        let mut group = [0u32; SECURITY_MAX_SID_SIZE.div_ceil(size_of::<u32>())];
        let mut size = std::mem::size_of_val(&group) as u32;
        let group = group.as_mut_ptr().cast();
        if unsafe { CreateWellKnownSid(WinBuiltinAdministratorsSid, null_mut(), group, &mut size) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut length = 0;
        if unsafe { GetTokenInformation(self.handle(), TokenGroups, null_mut(), 0, &mut length) }
            != 0
            || io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || length < size_of::<TOKEN_GROUPS>() as u32
            || length > 1024 * 1024
        {
            return Err(io::Error::other(
                "Could not determine the Windows token group list size",
            ));
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        self.read_info(TokenGroups, &mut buffer)?;
        let groups = buffer.as_ptr().cast::<TOKEN_GROUPS>();
        let count = unsafe { (*groups).GroupCount as usize };
        let offset = std::mem::offset_of!(TOKEN_GROUPS, Groups);
        if count > (length as usize - offset) / size_of::<SID_AND_ATTRIBUTES>() {
            return Err(io::Error::other("The Windows token group list is invalid"));
        }
        let groups = unsafe {
            std::slice::from_raw_parts(
                buffer
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset)
                    .cast::<SID_AND_ATTRIBUTES>(),
                count,
            )
        };
        Ok(groups.iter().any(|entry| {
            entry.Attributes & (SE_GROUP_ENABLED | SE_GROUP_USE_FOR_DENY_ONLY) == SE_GROUP_ENABLED
                && unsafe { EqualSid(entry.Sid, group) } != 0
        }))
    }

    pub(super) fn identity(&self) -> io::Result<Identity> {
        // `GetTokenInformation` embeds a pointer into the returned buffer. Keep
        // each buffer at its original address while consuming the SID.
        let mut buffer = [0usize; 24];
        self.read_info(TokenUser, &mut buffer)?;
        let sid = sid_as_string(sid_in_buffer(&buffer)?)?;
        self.read_info(TokenIntegrityLevel, &mut buffer)?;
        let integrity_sid = sid_in_buffer(&buffer)?;
        let count = unsafe { *GetSidSubAuthorityCount(integrity_sid) };
        if count == 0 {
            return Err(io::Error::other("The Windows token has no integrity level"));
        }
        Ok(Identity {
            sid,
            session: self.dword(TokenSessionId)?,
            integrity: unsafe { *GetSidSubAuthority(integrity_sid, u32::from(count - 1)) },
            elevated: self.dword(TokenElevation)? != 0,
            administrator_group: self.administrator_group()?,
            ui_access: self.dword(TokenUIAccess)? != 0,
        })
    }
}

fn sid_in_buffer(buffer: &[usize]) -> io::Result<PSID> {
    let sid = unsafe { (*buffer.as_ptr().cast::<SID_AND_ATTRIBUTES>()).Sid };
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return Err(io::Error::other(
            "The Windows token contains an invalid SID",
        ));
    }
    Ok(sid)
}

fn sid_as_string(sid: PSID) -> io::Result<String> {
    let mut wide = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    while unsafe { *wide.add(length) } != 0 {
        length += 1;
    }
    let output = String::from_utf16(unsafe { std::slice::from_raw_parts(wide, length) });
    unsafe { LocalFree(wide.cast()) };
    output.map_err(io::Error::other)
}
