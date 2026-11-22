use super::types::{Mutation, RecordedMutation, RootKind, TransactionPath, TransactionRoots};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) const UNSAFE: &str = "recycle.unsafePath";
pub(super) const CONFLICT: &str = "recycle.restoreTargetExists";
pub(super) const MODIFIED: &str = "recycle.payloadModified";
pub(super) const INVALID: &str = "recycle.invalidRecord";
pub(super) const IO_ERROR: &str = "recycle.fileOperationFailed";

pub(super) fn io_error(error: impl std::fmt::Display) -> String {
    log::error!(
        "批次事务文件操作失败：{}详情={:?}",
        super::diagnostics::Location,
        crate::file_check::log_field(&error.to_string())
    );
    IO_ERROR.into()
}

pub(super) fn io_at(path: &Path, operation: &str, error: std::io::Error) -> String {
    log_io(path, operation, &error);
    IO_ERROR.into()
}

pub(super) fn log_io(path: &Path, operation: &str, error: &std::io::Error) {
    log::warn!(
        target: super::diagnostics::TARGET,
        "整链事务文件操作失败：{}操作={operation}，路径={:?}，OS码={:?}，分类={:?}",
        super::diagnostics::Location, super::diagnostics::path(path), error.raw_os_error(), error.kind()
    );
}

pub(super) fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

#[cfg(unix)]
pub(crate) fn has_single_link(file: &fs::File) -> bool {
    use std::os::unix::fs::MetadataExt;
    file.metadata().is_ok_and(|info| info.nlink() == 1)
}

#[cfg(windows)]
pub(crate) fn has_single_link(file: &fs::File) -> bool {
    use std::{mem::MaybeUninit, os::windows::io::AsRawHandle};
    use winapi::um::fileapi::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};

    let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // The borrowed handle is live; the API initializes the struct on success.
    unsafe {
        GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) != 0
            && info.assume_init().nNumberOfLinks == 1
    }
}

pub(super) fn guard_single_link(path: &Path) -> Result<(), String> {
    use crate::file_check;
    let stage = super::check::TARGET;
    let file = fs::File::open(path)
        .map_err(|error| file_check::report_io(stage, path, error, file_check::READ))?;
    if !has_single_link(&file) {
        return Err(file_check::report(stage, path, file_check::LINK));
    }
    Ok(())
}

pub(super) fn validate_absolute(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(UNSAFE.into());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err(UNSAFE.into());
        }
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link(&metadata) => return Err(UNSAFE.into()),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_at(&current, "validate_path", error)),
        }
    }
    Ok(())
}

pub(super) fn validate_relative(relative: &str) -> Result<(), String> {
    if relative.is_empty()
        || relative.contains(['\\', ':', '\0'])
        || relative.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.chars().any(char::is_control)
        })
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(UNSAFE.into());
    }
    Ok(())
}

impl TransactionPath {
    pub(crate) fn resolve(&self, roots: &TransactionRoots) -> Result<PathBuf, String> {
        validate_relative(&self.relative)?;
        let root = match self.root {
            RootKind::Game => &roots.game_root,
            RootKind::Data => &roots.data_root,
        };
        validate_absolute(root)?;
        let path = root.join(&self.relative);
        validate_absolute(&path)?;
        Ok(path)
    }
}

pub(crate) fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

pub(super) fn overlaps(left: &Path, right: &Path) -> bool {
    let left = key(left);
    let right = key(right);
    left == right || left.starts_with(&(right.clone() + "/")) || right.starts_with(&(left + "/"))
}

pub(super) fn validate_roots(
    transaction_root: &Path,
    roots: &TransactionRoots,
) -> Result<(), String> {
    validate_absolute(transaction_root)?;
    validate_absolute(&roots.game_root)?;
    validate_absolute(&roots.data_root)?;
    if overlaps(transaction_root, &roots.game_root)
        || !transaction_root.starts_with(&roots.data_root)
        || transaction_root == roots.data_root
        || !roots.game_root.is_dir()
        || !roots.data_root.is_dir()
    {
        return Err(UNSAFE.into());
    }
    Ok(())
}

pub(super) fn validate_mutations(
    mutations: &[Mutation],
    transaction_root: &Path,
    roots: &TransactionRoots,
) -> Result<(), String> {
    let resolve = |target| super::check::resolve(target, roots, super::check::TARGET);
    let mut paths = Vec::new();
    for mutation in mutations {
        match mutation {
            Mutation::File(change) => paths.push(resolve(&change.target)?),
            Mutation::Tree(change) => paths.push(resolve(&change.target)?),
            Mutation::Move(change) => {
                if change.from.root != change.to.root {
                    return Err(UNSAFE.into());
                }
                paths.push(resolve(&change.from)?);
                paths.push(resolve(&change.to)?);
            }
        }
    }
    validate_endpoints(&paths, transaction_root)
}

pub(super) fn validate_records(
    records: &[super::types::Record],
    transaction_root: &Path,
    roots: &TransactionRoots,
) -> Result<(), String> {
    let mut paths = Vec::new();
    for record in records {
        match &record.operation {
            RecordedMutation::File {
                target,
                temporary,
                backup,
                payload,
                ..
            } => {
                let target = target.resolve(roots)?;
                let temporary = temporary.resolve(roots)?;
                if target.parent() != temporary.parent()
                    || !temporary
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(".oc2-batch-"))
                {
                    return Err(INVALID.into());
                }
                for relative in backup.iter().chain(payload.iter()) {
                    validate_relative(relative)?;
                }
                paths.extend([target, temporary]);
            }
            RecordedMutation::Move { from, to, .. } => {
                if from.root != to.root {
                    return Err(INVALID.into());
                }
                paths.extend([from.resolve(roots)?, to.resolve(roots)?]);
            }
            RecordedMutation::Tree {
                target,
                temporary,
                backup,
                payload,
                ..
            } => {
                let target = target.resolve(roots)?;
                let temporary = temporary.resolve(roots)?;
                let backup = backup.resolve(roots)?;
                if target.parent() != temporary.parent()
                    || target.parent() != backup.parent()
                    || !temporary
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(".oc2-batch-"))
                    || !backup
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(".oc2-batch-"))
                {
                    return Err(INVALID.into());
                }
                if let Some(payload) = payload {
                    validate_relative(payload)?;
                }
                paths.extend([target, temporary, backup]);
            }
        }
    }
    validate_endpoints(&paths, transaction_root)
}

fn validate_endpoints(paths: &[PathBuf], transaction_root: &Path) -> Result<(), String> {
    for (index, path) in paths.iter().enumerate() {
        if overlaps(path, transaction_root)
            || paths[..index]
                .iter()
                .any(|previous| overlaps(path, previous))
        {
            return Err(crate::mod_changes::mutations::report_install_block(
                "modChanges.installTargetPathCollision",
                "transaction",
                &[("path", &crate::display_path::display_path(path))],
            ));
        }
    }
    Ok(())
}
