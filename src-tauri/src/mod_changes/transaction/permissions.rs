use super::paths::io_at;
use super::types::PermissionsSnapshot;
use std::{fs, path::Path};

impl PermissionsSnapshot {
    pub(super) fn capture(path: &Path) -> Result<Self, String> {
        let metadata =
            fs::metadata(path).map_err(|error| io_at(path, "permission_metadata", error))?;
        Ok(Self {
            readonly: metadata.permissions().readonly(),
            #[cfg(unix)]
            mode: {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode()
            },
            #[cfg(windows)]
            security_descriptor: windows::read_acl(path)?,
        })
    }

    pub(super) fn restore(&self, path: &Path) -> Result<(), String> {
        #[cfg(windows)]
        windows::write_acl(path, &self.security_descriptor)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(self.mode))
                .map_err(|error| io_at(path, "write_permissions", error))?;
        }
        #[cfg(not(unix))]
        {
            let mut permissions = fs::metadata(path)
                .map_err(|error| io_at(path, "permission_metadata", error))?
                .permissions();
            permissions.set_readonly(self.readonly);
            fs::set_permissions(path, permissions)
                .map_err(|error| io_at(path, "write_permissions", error))?;
        }
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::securitybaseapi::{
        GetFileSecurityW, GetSecurityDescriptorControl, SetFileSecurityW,
    };
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
        UNPROTECTED_DACL_SECURITY_INFORMATION,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub(super) fn read_acl(path: &Path) -> Result<Vec<u8>, String> {
        let encoded = wide(path);
        let mut size = 0;
        unsafe {
            GetFileSecurityW(
                encoded.as_ptr(),
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut size,
            )
        };
        if size == 0 || size > 1024 * 1024 {
            return Err(io_at(
                path,
                "read_acl_size",
                std::io::Error::last_os_error(),
            ));
        }
        let mut buffer = vec![0; size as usize];
        if unsafe {
            GetFileSecurityW(
                encoded.as_ptr(),
                DACL_SECURITY_INFORMATION,
                buffer.as_mut_ptr().cast(),
                size,
                &mut size,
            )
        } == 0
        {
            return Err(io_at(path, "read_acl", std::io::Error::last_os_error()));
        }
        buffer.truncate(size as usize);
        // SetFileSecurity may clear AUTO_INHERITED bookkeeping while retaining
        // the same ACEs and protection. Exclude those non-permission flags.
        if buffer.len() >= 4 {
            let control = u16::from_le_bytes([buffer[2], buffer[3]]) & !0x0500;
            buffer[2..4].copy_from_slice(&control.to_le_bytes());
        }
        Ok(buffer)
    }

    pub(super) fn write_acl(path: &Path, descriptor: &[u8]) -> Result<(), String> {
        if descriptor.is_empty() {
            return Err("recycle.invalidRecord".into());
        }
        let mut descriptor = descriptor.to_vec();
        let mut control = 0;
        let mut revision = 0;
        if unsafe {
            GetSecurityDescriptorControl(
                descriptor.as_mut_ptr().cast(),
                &mut control,
                &mut revision,
            )
        } == 0
        {
            return Err(io_at(
                path,
                "read_acl_control",
                std::io::Error::last_os_error(),
            ));
        }
        let protection = if control & SE_DACL_PROTECTED != 0 {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        let encoded = wide(path);
        if unsafe {
            SetFileSecurityW(
                encoded.as_ptr(),
                DACL_SECURITY_INFORMATION | protection,
                descriptor.as_mut_ptr().cast(),
            )
        } == 0
        {
            return Err(io_at(path, "write_acl", std::io::Error::last_os_error()));
        }
        Ok(())
    }
}
