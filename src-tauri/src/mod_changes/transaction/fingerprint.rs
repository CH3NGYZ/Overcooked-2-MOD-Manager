use super::paths::{MODIFIED, UNSAFE, io_at, io_error, is_link, validate_absolute};
use super::types::{Fingerprint, PermissionsSnapshot, StagedFile, StagedTree};
use crate::file_metadata::Stamp;
use sha2::{Digest, Sha256};
use std::{fs, fs::File, io::Read, path::Path};

impl Fingerprint {
    pub(crate) fn capture(path: &Path) -> Result<Self, String> {
        Self::capture_with_files(path, &mut |_, _| Ok(()))
    }

    /// Record regular-file fingerprints during the same walk that hashes their tree.
    pub(crate) fn capture_with_files(
        path: &Path,
        visit: &mut impl FnMut(&Path, &Self) -> Result<(), String>,
    ) -> Result<Self, String> {
        Self::capture_inner(path, visit, None)
    }

    /// Use the existing validation read to collect native stamps for watched endpoints.
    pub(super) fn capture_for_watch(
        path: &Path,
        committed: &mut crate::directory_watch::CommittedPaths,
    ) -> Result<Self, String> {
        Self::capture_inner(path, &mut |_, _| Ok(()), Some(committed))
    }

    fn capture_inner(
        path: &Path,
        visit: &mut impl FnMut(&Path, &Self) -> Result<(), String>,
        mut committed: Option<&mut crate::directory_watch::CommittedPaths>,
    ) -> Result<Self, String> {
        validate_absolute(path)?;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::Missing),
            Err(error) => return Err(io_at(path, "fingerprint_metadata", error)),
        };
        if is_link(&metadata) {
            return Err(UNSAFE.into());
        }
        // An external writer racing this check must not get its new stamp
        // attached to the bytes verified earlier. Unsupported metadata merely
        // opts this path out of deduplication.
        let stamp = committed.as_ref().and_then(|_| Stamp::capture(path).ok());
        let permissions = PermissionsSnapshot::capture(path)?;
        let fingerprint = if metadata.is_file() {
            let mut input =
                File::open(path).map_err(|error| io_at(path, "fingerprint_open", error))?;
            let mut digest = Sha256::new();
            let mut size = 0;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count = input
                    .read(&mut buffer)
                    .map_err(|error| io_at(path, "fingerprint_read", error))?;
                if count == 0 {
                    break;
                }
                digest.update(&buffer[..count]);
                size += count as u64;
            }
            if size != metadata.len() || PermissionsSnapshot::capture(path)? != permissions {
                return Err(MODIFIED.into());
            }
            let fingerprint = Self::File {
                size,
                sha256: format!("{:x}", digest.finalize()),
                permissions,
            };
            visit(path, &fingerprint)?;
            fingerprint
        } else if metadata.is_dir() {
            let read_error = |error| io_at(path, "fingerprint_list", error);
            let mut children = fs::read_dir(path)
                .map_err(read_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(read_error)?;
            children.sort_by_key(|entry| entry.file_name());
            let mut digest = Sha256::new();
            for entry in children {
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| UNSAFE.to_string())?;
                let child = Self::capture_inner(&entry.path(), visit, committed.as_deref_mut())?;
                if child == Self::Missing {
                    return Err(MODIFIED.into());
                }
                let bytes = serde_json::to_vec(&(name, child)).map_err(io_error)?;
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
            Self::Directory {
                sha256: format!("{:x}", digest.finalize()),
                permissions,
            }
        } else {
            return Err(UNSAFE.into());
        };
        if let (Some(committed), Some(stamp)) = (committed, stamp)
            && Stamp::capture(path).is_ok_and(|current| current == stamp)
        {
            committed.present(path, stamp);
        }
        Ok(fingerprint)
    }

    pub(super) fn permissions(&self) -> Option<&PermissionsSnapshot> {
        match self {
            Self::Missing => None,
            Self::File { permissions, .. } | Self::Directory { permissions, .. } => {
                Some(permissions)
            }
        }
    }

    pub(crate) fn same_content(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Missing, Self::Missing) => true,
            (
                Self::File {
                    size: a, sha256: x, ..
                },
                Self::File {
                    size: b, sha256: y, ..
                },
            ) => a == b && x == y,
            (Self::Directory { sha256: x, .. }, Self::Directory { sha256: y, .. }) => x == y,
            _ => false,
        }
    }
}

impl StagedFile {
    pub(crate) fn capture(path: std::path::PathBuf) -> Result<Self, String> {
        use super::check;
        use crate::file_check;
        let fingerprint = check::capture(&path, check::STAGED)?;
        match fingerprint {
            Fingerprint::File { .. } => Ok(Self { path, fingerprint }),
            Fingerprint::Missing => Err(file_check::report(
                check::STAGED,
                &path,
                file_check::MISSING,
            )),
            Fingerprint::Directory { .. } => {
                Err(file_check::report(check::STAGED, &path, file_check::TYPE))
            }
        }
    }
}

impl StagedTree {
    pub(crate) fn capture(path: std::path::PathBuf) -> Result<Self, String> {
        use super::check;
        use crate::file_check;
        let fingerprint = check::capture(&path, check::STAGED)?;
        match fingerprint {
            Fingerprint::Directory { .. } => Ok(Self { path, fingerprint }),
            Fingerprint::Missing => Err(file_check::report(
                check::STAGED,
                &path,
                file_check::MISSING,
            )),
            Fingerprint::File { .. } => {
                Err(file_check::report(check::STAGED, &path, file_check::TYPE))
            }
        }
    }
}
