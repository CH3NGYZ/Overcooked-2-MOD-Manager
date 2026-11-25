use crate::app_update::windows_session::admin_identity;
use crate::elevation::wide;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{MaybeUninit, size_of};
use std::os::windows::{ffi::OsStringExt, fs::OpenOptionsExt, io::AsRawHandle};
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr::{addr_of, addr_of_mut, null_mut};
use winapi::shared::sddl::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use winapi::shared::winerror::{ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
use winapi::um::accctrl::SE_FILE_OBJECT;
use winapi::um::aclapi::GetSecurityInfo;
use winapi::um::combaseapi::CoTaskMemFree;
use winapi::um::fileapi::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, GetDriveTypeW, GetFileInformationByHandle,
    GetFinalPathNameByHandleW,
};
use winapi::um::knownfolders::FOLDERID_ProgramFiles;
use winapi::um::minwinbase::SECURITY_ATTRIBUTES;
use winapi::um::securitybaseapi::{
    GetAce, GetAclInformation, GetSecurityDescriptorControl, IsWellKnownSid,
};
use winapi::um::shlobj::{KF_FLAG_DEFAULT_PATH, SHGetKnownFolderPath};
use winapi::um::winbase::{
    DRIVE_FIXED, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, LocalFree,
};
use winapi::um::winnt::{
    ACCESS_ALLOWED_ACE, ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE, ACE_HEADER,
    ACL_SIZE_INFORMATION, AclSizeInformation, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION,
    DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_DELETE_CHILD, FILE_GENERIC_READ,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_EA,
    GENERIC_ALL, GENERIC_WRITE, INHERIT_ONLY_ACE, OBJECT_INHERIT_ACE, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, READ_CONTROL, SE_DACL_PROTECTED, WRITE_DAC, WRITE_OWNER,
    WinBuiltinAdministratorsSid, WinCreatorOwnerRightsSid, WinCreatorOwnerSid, WinLocalSystemSid,
};

const UNAVAILABLE: &str = "update.installFailedManual";
const TRUSTED_INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
const PRIVATE_DACL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;RC;;;OW)";
pub(super) const EVENT_DACL: &str = "O:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)";

pub(crate) struct ProtectedAttemptsDir {
    path: PathBuf,
    _handles: Vec<File>,
}

impl ProtectedAttemptsDir {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn attempts_dir(create: bool) -> Result<Option<ProtectedAttemptsDir>, String> {
    if !crate::is_elevated() {
        return Ok(None);
    }
    locked_attempts_dir(create).map_err(manual)
}

fn locked_attempts_dir(create: bool) -> io::Result<Option<ProtectedAttemptsDir>> {
    let sid = admin_identity()?;
    let mut raw = null_mut();
    let result = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_ProgramFiles,
            KF_FLAG_DEFAULT_PATH,
            null_mut(),
            &mut raw,
        )
    };
    if result < 0 || raw.is_null() {
        return Err(denied(
            "The machine Program Files directory could not be located",
        ));
    }
    let mut path = PathBuf::from(unsafe { from_wide(raw) });
    unsafe { CoTaskMemFree(raw.cast()) };
    let mut handles = pin_directories(&path)?;
    for component in ["OC2ModManager-updates", "app-update-attempts", &sid] {
        path.push(component);
        if create {
            create_private_directory(&path)?;
        }
        match pin(&path, true, false, true) {
            Ok(handle) => handles.push(handle),
            Err(error) if !create && missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    Ok(Some(ProtectedAttemptsDir {
        path,
        _handles: handles,
    }))
}

pub(super) fn create_work(path: &Path) -> io::Result<()> {
    if !create_private_directory(path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Update directory already exists",
        ));
    }
    pin_work(path)?;
    Ok(())
}

pub(super) fn pin_directories(path: &Path) -> io::Result<Vec<File>> {
    let mut chain = path.ancestors().collect::<Vec<_>>();
    chain.reverse();
    let root = chain
        .first()
        .filter(|root| disk_root(root))
        .ok_or_else(|| denied("The update path is not on a local drive"))?;
    if unsafe { GetDriveTypeW(wide(root.as_os_str()).as_ptr()) } != DRIVE_FIXED
        || path.components().any(|component| match component {
            Component::Prefix(_) | Component::RootDir => false,
            Component::Normal(name) => name.to_str().is_none_or(|name| {
                name.ends_with(['.', ' '])
                    || name.contains([':', '"', '<', '>', '|', '?', '*', '\r', '\n', '\0'])
            }),
            _ => true,
        })
    {
        return Err(denied("The update path is not on a fixed, ordinary drive"));
    }
    let handles = chain
        .into_iter()
        .map(|directory| pin(directory, true, directory != path, false))
        .collect::<io::Result<Vec<_>>>()?;
    if !is_canonical_endpoint(
        path,
        handles
            .last()
            .ok_or_else(|| denied("Missing update directory handle"))?,
    )? {
        return Err(denied(
            "The update path uses a redirected drive or alternate name",
        ));
    }
    crate::file_metadata::write_volume(path)
        .map_err(|error| io::Error::other(format!("Unverified update volume: {error:?}")))?;
    Ok(handles)
}

fn is_canonical_endpoint(path: &Path, file: &File) -> io::Result<bool> {
    let mut buffer = vec![0u16; 32768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle().cast(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    let resolved = String::from_utf16(&buffer[..length]).map_err(io::Error::other)?;
    Ok(resolved.strip_prefix(r"\\?\").is_some_and(|resolved| {
        path.to_str()
            .is_some_and(|requested| resolved.eq_ignore_ascii_case(requested))
    }))
}

pub(super) fn pin_install_file(path: &Path) -> io::Result<File> {
    pin(path, false, false, false)
}

pub(super) fn has_one_link(file: &File) -> io::Result<bool> {
    let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { info.assume_init() }.nNumberOfLinks == 1)
}

pub(super) fn pin_work(path: &Path) -> io::Result<File> {
    pin(path, true, false, true)
}

fn disk_root(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
        && components.next() == Some(Component::RootDir)
        && components.next().is_none()
}

fn missing(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32)
}

fn create_private_directory(path: &Path) -> io::Result<bool> {
    let security = SecurityDescriptor::new(PRIVATE_DACL)?;
    let mut attributes = security.attributes();
    let path = wide(path.as_os_str());
    if unsafe { CreateDirectoryW(path.as_ptr(), &mut attributes) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error);
        }
        return Ok(false);
    }
    Ok(true)
}

fn pin(path: &Path, directory: bool, allow_create: bool, private: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).access_mode(if directory {
        READ_CONTROL | FILE_READ_ATTRIBUTES
    } else {
        FILE_GENERIC_READ
    });
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };
    let sharing = FILE_SHARE_READ | if directory { FILE_SHARE_WRITE } else { 0 };
    let file = options.custom_flags(flags).share_mode(sharing).open(path)?;
    super::check_type(&file, directory)?;
    verify_acl(&file, allow_create, private)?;
    Ok(file)
}

fn verify_acl(file: &File, allow_create: bool, private: bool) -> io::Result<()> {
    let (mut owner, mut dacl) = (null_mut(), null_mut());
    let mut descriptor = SecurityDescriptor(null_mut());
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle().cast(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor.0,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    if owner.is_null() || dacl.is_null() || !trusted_sid(owner, private) {
        return Err(denied(
            "An update path is not owned by Windows or Administrators",
        ));
    }
    if private {
        let (mut control, mut revision) = (0, 0);
        if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0
            || control & SE_DACL_PROTECTED == 0
        {
            return Err(denied("The private update directory has no protected ACL"));
        }
    }
    let mut info = ACL_SIZE_INFORMATION {
        AceCount: 0,
        AclBytesInUse: 0,
        AclBytesFree: 0,
    };
    if unsafe {
        GetAclInformation(
            dacl,
            addr_of_mut!(info).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut inheritable_owner_rights = false;
    for index in 0..info.AceCount {
        let mut raw = null_mut();
        if unsafe { GetAce(dacl, index, &mut raw) } == 0 || raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let header = unsafe { &*raw.cast::<ACE_HEADER>() };
        if header.AceType == ACCESS_DENIED_ACE_TYPE {
            continue;
        }
        let inherit_only = header.AceFlags & INHERIT_ONLY_ACE != 0;
        if inherit_only && allow_create {
            continue;
        }
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE
            || usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>()
        {
            return Err(denied(
                "An unsupported ACL entry was found on an update path",
            ));
        }
        let ace = raw.cast::<ACCESS_ALLOWED_ACE>();
        let sid = unsafe { addr_of!((*ace).SidStart).cast_mut().cast() };
        let access = unsafe { (*ace).Mask };
        let safe_owner_rights =
            access == READ_CONTROL && unsafe { IsWellKnownSid(sid, WinCreatorOwnerRightsSid) } != 0;
        if safe_owner_rights
            && header.AceFlags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE)
                == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE)
        {
            inheritable_owner_rights = true;
        }
        let safe_creator_owner =
            inherit_only && !private && unsafe { IsWellKnownSid(sid, WinCreatorOwnerSid) } != 0;
        if !safe_creator_owner
            && !safe_owner_rights
            && !trusted_sid(sid, private)
            && (private || writable(access, allow_create))
        {
            return Err(denied(
                "An update path can be changed without administrator permissions",
            ));
        }
    }
    if private && !inheritable_owner_rights {
        return Err(denied(
            "Private update children would inherit implicit owner permissions",
        ));
    }
    Ok(())
}

fn writable(access: u32, allow_create: bool) -> bool {
    let mut writes = GENERIC_ALL
        | GENERIC_WRITE
        | DELETE
        | WRITE_DAC
        | WRITE_OWNER
        | FILE_WRITE_ATTRIBUTES
        | FILE_WRITE_EA
        | FILE_DELETE_CHILD;
    if !allow_create {
        writes |= FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY;
    }
    access & writes != 0
}

fn trusted_sid(sid: PSID, private: bool) -> bool {
    if unsafe {
        IsWellKnownSid(sid, WinLocalSystemSid) != 0
            || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
    } {
        return true;
    }
    let mut string = null_mut();
    if private || unsafe { ConvertSidToStringSidW(sid, &mut string) } == 0 || string.is_null() {
        return false;
    }
    let trusted = unsafe { from_wide(string) } == TRUSTED_INSTALLER;
    unsafe { LocalFree(string.cast()) };
    trusted
}

pub(super) struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    pub(super) fn new(sddl: &str) -> io::Result<Self> {
        let mut result = Self(null_mut());
        let sddl = wide(sddl.as_ref());
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut result.0,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(result)
    }

    pub(super) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0.cast()) };
        }
    }
}

unsafe fn from_wide(string: *const u16) -> OsString {
    let mut length = 0;
    while unsafe { *string.add(length) } != 0 {
        length += 1;
    }
    OsString::from_wide(unsafe { std::slice::from_raw_parts(string, length) })
}

fn manual(error: io::Error) -> String {
    log::warn!("无法验证管理员更新暂存目录：{error}");
    UNAVAILABLE.into()
}

fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_drive_roots_and_non_mutating_ancestors_are_allowed() {
        assert!(disk_root(Path::new(r"C:\")));
        assert!(!disk_root(Path::new(r"\\server\share\")));
        assert!(!disk_root(Path::new(r"\\?\C:\")));
        assert!(!writable(0x1200a9, false));
        assert!(!writable(FILE_ADD_SUBDIRECTORY, true));
        assert!(writable(FILE_ADD_SUBDIRECTORY, false));
        assert!(writable(FILE_DELETE_CHILD | WRITE_DAC, true));
        assert!(writable(FILE_WRITE_ATTRIBUTES | FILE_WRITE_EA, true));
    }

    #[test]
    fn refuses_a_user_owned_temporary_target() {
        let untrusted = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
        assert!(pin_directories(untrusted.path()).is_err());
    }

    #[test]
    fn descriptors_are_accepted_by_windows() {
        assert!(SecurityDescriptor::new(PRIVATE_DACL).is_ok());
        assert!(SecurityDescriptor::new(EVENT_DACL).is_ok());
    }
}
