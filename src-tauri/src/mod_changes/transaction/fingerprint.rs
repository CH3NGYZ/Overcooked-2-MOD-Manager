use super::paths::{MODIFIED, UNSAFE, io_error, is_link, validate_absolute};
use super::types::{Fingerprint, PermissionsSnapshot, StagedFile};
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
        validate_absolute(path)?;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::Missing),
            Err(error) => return Err(io_error(error)),
        };
        if is_link(&metadata) {
            return Err(UNSAFE.into());
        }
        let permissions = PermissionsSnapshot::capture(path)?;
        if metadata.is_file() {
            let mut input = File::open(path).map_err(io_error)?;
            let mut digest = Sha256::new();
            let mut size = 0;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count = input.read(&mut buffer).map_err(io_error)?;
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
            Ok(fingerprint)
        } else if metadata.is_dir() {
            let mut children = fs::read_dir(path)
                .map_err(io_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(io_error)?;
            children.sort_by_key(|entry| entry.file_name());
            let mut digest = Sha256::new();
            for entry in children {
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| UNSAFE.to_string())?;
                let child = Self::capture_with_files(&entry.path(), visit)?;
                if child == Self::Missing {
                    return Err(MODIFIED.into());
                }
                let bytes = serde_json::to_vec(&(name, child)).map_err(io_error)?;
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
            Ok(Self::Directory {
                sha256: format!("{:x}", digest.finalize()),
                permissions,
            })
        } else {
            Err(UNSAFE.into())
        }
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
        let fingerprint = Fingerprint::capture(&path)?;
        if !matches!(fingerprint, Fingerprint::File { .. }) {
            return Err("modChanges.payloadChanged".into());
        }
        Ok(Self { path, fingerprint })
    }
}
