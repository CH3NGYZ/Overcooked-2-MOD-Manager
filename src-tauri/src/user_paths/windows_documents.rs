use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::{FileTypeExt, MetadataExt};
use std::path::{Component, Path, PathBuf, Prefix};
use winapi::um::fileapi::GetDriveTypeW;
use winapi::um::knownfolders::FOLDERID_Documents;
use winapi::um::shlobj::{KF_FLAG_DEFAULT_PATH, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath};
use winapi::um::winbase::{DRIVE_FIXED, DRIVE_NO_ROOT_DIR, DRIVE_REMOVABLE, DRIVE_UNKNOWN};
use winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::core::{HRESULT, PWSTR};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    Unsafe,
    NotDirectory,
    Unavailable,
}

pub(super) fn default_documents() -> Result<PathBuf, String> {
    documents_path(KF_FLAG_DONT_VERIFY)
}

fn documents_path(flags: u32) -> Result<PathBuf, String> {
    let path = unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
            return Err(super::ROOT_UNAVAILABLE.into());
        }
        let mut wide = std::ptr::null_mut();
        let result = HRESULT(SHGetKnownFolderPath(
            &FOLDERID_Documents,
            flags,
            std::ptr::null_mut(),
            &mut wide,
        ));
        let path = (result.is_ok() && !wide.is_null())
            .then(|| PathBuf::from(OsString::from_wide(PWSTR(wide).as_wide())));
        CoTaskMemFree(Some(wide.cast()));
        if initialized.is_ok() {
            CoUninitialize();
        }
        path
    };
    path.ok_or_else(|| super::ROOT_UNAVAILABLE.into())
}

pub(crate) fn resolve_root(
    raw: &Path,
    system_default: bool,
    allow_missing_root: bool,
) -> Result<(PathBuf, bool), Problem> {
    resolve_root_with(raw, system_default, allow_missing_root, || {
        documents_path(KF_FLAG_DEFAULT_PATH | KF_FLAG_DONT_VERIFY).ok()
    })
}

fn resolve_root_with(
    raw: &Path,
    system_default: bool,
    allow_missing_root: bool,
    unredirected_documents: impl Fn() -> Option<PathBuf>,
) -> Result<(PathBuf, bool), Problem> {
    if raw
        .file_name()
        .is_none_or(|name| name != super::DATA_DIRECTORY)
    {
        return Err(Problem::Unsafe);
    }
    let mut links = 0;
    let parent = raw.parent().ok_or(Problem::Unsafe)?;
    let can_initialize_documents = || {
        system_default
            && allow_missing_root
            && unredirected_documents().is_some_and(|expected| expected == parent)
    };
    let actual = check_directory(
        parent,
        system_default,
        &mut links,
        &can_initialize_documents,
    )?
    .join(super::DATA_DIRECTORY);
    match fs::symlink_metadata(&actual) {
        Ok(metadata)
            if metadata.is_dir()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 =>
        {
            if links != 0 {
                match same_file::is_same_file(raw, &actual) {
                    Ok(true) => (),
                    Ok(false) => return Err(Problem::Unsafe),
                    Err(error) => return Err(path_error(error)),
                }
            }
        }
        Ok(metadata)
            if metadata.is_file()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 =>
        {
            return Err(Problem::NotDirectory);
        }
        Ok(_) => return Err(Problem::Unsafe),
        Err(error) if allow_missing_root && error.kind() == io::ErrorKind::NotFound => {
            if links != 0 {
                match same_file::is_same_file(parent, actual.parent().ok_or(Problem::Unsafe)?) {
                    Ok(true) => (),
                    Ok(false) => return Err(Problem::Unsafe),
                    Err(error) => return Err(path_error(error)),
                }
            }
        }
        Err(error) => return Err(path_error(error)),
    }
    for path in [raw, actual.as_path()] {
        crate::install_guard::validate_install_dir(path).map_err(|error| match error {
            crate::install_guard::InstallDirectoryError::Forbidden => Problem::Unsafe,
            crate::install_guard::InstallDirectoryError::Unavailable(_) => Problem::Unavailable,
        })?;
    }
    Ok(if links == 0 {
        (raw.to_path_buf(), false)
    } else {
        (actual, true)
    })
}

fn check_directory(
    raw: &Path,
    follow_links: bool,
    links: &mut u8,
    can_initialize_documents: &dyn Fn() -> bool,
) -> Result<PathBuf, Problem> {
    let mut components = raw.components().peekable();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(Problem::Unsafe);
    };
    if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        || components.next() != Some(Component::RootDir)
    {
        return Err(Problem::Unsafe);
    }
    let mut current = PathBuf::from(prefix.as_os_str());
    current.push(Component::RootDir.as_os_str());
    check_local_drive(&current)?;
    let root = fs::symlink_metadata(&current).map_err(path_error)?;
    if !root.is_dir() || root.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(Problem::Unsafe);
    }
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(Problem::Unsafe);
        };
        let bytes = name.as_encoded_bytes();
        if bytes.last().is_some_and(|byte| matches!(byte, b'.' | b' '))
            || bytes.iter().any(|byte| {
                *byte < b' '
                    || matches!(byte, b':' | b'<' | b'>' | b'"' | b'|' | b'?' | b'*' | b'/')
            })
        {
            return Err(Problem::Unsafe);
        }
        current.push(name);
        let shortcut_extension = Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound && follow_links => {
                let mut shortcut = name.to_os_string();
                shortcut.push(".lnk");
                match fs::symlink_metadata(current.with_file_name(shortcut)) {
                    Ok(metadata)
                        if metadata.is_file()
                            && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 =>
                    {
                        return Err(Problem::NotDirectory);
                    }
                    Ok(_) => return Err(Problem::Unsafe),
                    Err(error) if error.kind() != io::ErrorKind::NotFound => {
                        return Err(path_error(error));
                    }
                    Err(_) => (),
                }
                if components.peek().is_none()
                    && *links == 0
                    && !shortcut_extension
                    && can_initialize_documents()
                {
                    break;
                }
                return Err(Problem::Unavailable);
            }
            Err(error) => return Err(path_error(error)),
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            if !follow_links || !metadata.file_type().is_symlink_dir() || *links >= 32 {
                return Err(Problem::Unsafe);
            }
            let target = fs::read_link(&current).map_err(|error| {
                if error.raw_os_error().is_none() {
                    Problem::Unsafe
                } else {
                    path_error(error)
                }
            })?;
            let target = link_target(&current, &target)?;
            *links += 1;
            current = check_directory(&target, true, links, &|| false)?;
        } else if !metadata.is_dir() {
            return Err(Problem::NotDirectory);
        }
        if shortcut_extension {
            return Err(Problem::Unsafe);
        }
    }
    Ok(current)
}

fn check_local_drive(root: &Path) -> Result<(), Problem> {
    let Some(Component::Prefix(prefix)) = root.components().next() else {
        return Err(Problem::Unsafe);
    };
    let (Prefix::Disk(letter) | Prefix::VerbatimDisk(letter)) = prefix.kind() else {
        return Err(Problem::Unsafe);
    };
    let drive = [u16::from(letter), u16::from(b':'), u16::from(b'\\'), 0];
    match unsafe { GetDriveTypeW(drive.as_ptr()) } {
        DRIVE_FIXED | DRIVE_REMOVABLE => Ok(()),
        DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR => Err(Problem::Unavailable),
        _ => Err(Problem::Unsafe),
    }
}

fn link_target(link: &Path, target: &Path) -> Result<PathBuf, Problem> {
    if target.is_absolute() {
        return Ok(target.to_path_buf());
    }
    let mut path = link.parent().ok_or(Problem::Unsafe)?.to_path_buf();
    for component in target.components() {
        match component {
            Component::Normal(name) => path.push(name),
            Component::CurDir => (),
            Component::ParentDir if path.pop() => (),
            _ => return Err(Problem::Unsafe),
        }
    }
    Ok(path)
}

fn path_error(error: io::Error) -> Problem {
    use winapi::shared::winerror::{
        ERROR_BAD_PATHNAME, ERROR_CANT_RESOLVE_FILENAME, ERROR_DIRECTORY, ERROR_INVALID_NAME,
        ERROR_INVALID_REPARSE_DATA, ERROR_NOT_A_REPARSE_POINT, ERROR_REPARSE_TAG_INVALID,
        ERROR_REPARSE_TAG_MISMATCH,
    };
    match error.raw_os_error().map(|code| code as u32) {
        Some(
            ERROR_BAD_PATHNAME
            | ERROR_CANT_RESOLVE_FILENAME
            | ERROR_DIRECTORY
            | ERROR_INVALID_NAME
            | ERROR_INVALID_REPARSE_DATA
            | ERROR_NOT_A_REPARSE_POINT
            | ERROR_REPARSE_TAG_INVALID
            | ERROR_REPARSE_TAG_MISMATCH,
        ) => Problem::Unsafe,
        _ => Problem::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_paths::locate_root_with_default;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let root = PathBuf::from(crate::display_path::display_path(root));
        (temp, root)
    }

    fn junction(path: &Path, target: &Path) {
        let result = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(path)
            .arg(target)
            .output()
            .unwrap();
        assert!(result.status.success(), "failed to create fixture junction");
    }

    #[test]
    fn ordinary_root_preserves_existing_path_identity() {
        let (_temp, base) = fixture();
        let raw = base.join(super::super::DATA_DIRECTORY);
        assert_eq!(resolve_root(&raw, true, true), Ok((raw.clone(), false)));
        assert_eq!(resolve_root(&raw, true, false), Err(Problem::Unavailable));
        fs::create_dir(&raw).unwrap();
        assert_eq!(resolve_root(&raw, false, false), Ok((raw.clone(), false)));
        assert_eq!(resolve_root(&raw, true, false), Ok((raw, false)));
    }

    #[test]
    fn only_the_missing_unredirected_system_documents_leaf_can_initialize() {
        let (_temp, base) = fixture();
        let documents = base.join("Documents");
        let name = super::super::DATA_DIRECTORY;
        let raw = documents.join(name);
        let unredirected = || Some(documents.clone());
        assert_eq!(
            resolve_root_with(&raw, true, true, unredirected),
            Ok((raw.clone(), false))
        );
        assert!(!documents.exists());
        assert_eq!(
            resolve_root_with(&raw, true, false, unredirected),
            Err(Problem::Unavailable)
        );
        assert_eq!(
            resolve_root_with(&raw, false, true, unredirected),
            Err(Problem::Unavailable)
        );
        let moved = base.join("MovedDocuments");
        assert_eq!(
            resolve_root_with(&moved.join(name), true, true, unredirected),
            Err(Problem::Unavailable)
        );
        let missing_ancestor = base.join("Missing").join("Documents");
        assert_eq!(
            resolve_root_with(&missing_ancestor.join(name), true, true, || Some(
                missing_ancestor.clone()
            )),
            Err(Problem::Unavailable)
        );
        fs::write(base.join("Documents.lnk"), []).unwrap();
        assert_eq!(
            resolve_root_with(&raw, true, true, unredirected),
            Err(Problem::NotDirectory)
        );
    }

    #[test]
    fn cannot_initialize_missing_documents_below_a_directory_link() {
        let (_temp, base) = fixture();
        let target = base.join("RealProfile");
        fs::create_dir(&target).unwrap();
        let link = base.join("Profile");
        junction(&link, &target);
        let documents = link.join("Documents");
        let raw = documents.join(super::super::DATA_DIRECTORY);
        let resolved = resolve_root_with(&raw, true, true, || Some(documents.clone()));
        fs::remove_dir(link).unwrap();
        assert_eq!(resolved, Err(Problem::Unavailable));
    }

    #[test]
    fn follows_only_system_documents_parent_links() {
        let (_temp, base) = fixture();
        let target = base.join("MovedDocuments");
        fs::create_dir(&target).unwrap();
        let documents = base.join("Documents");
        junction(&documents, &target);
        let raw = documents.join(super::super::DATA_DIRECTORY);
        let actual = target.join(super::super::DATA_DIRECTORY);
        let missing_leaf = resolve_root(&raw, true, true);
        fs::create_dir(&actual).unwrap();
        let system = resolve_root(&raw, true, false);
        let explicit = resolve_root(&raw, false, false);
        fs::remove_dir(&documents).unwrap();
        assert_eq!(missing_leaf, Ok((actual.clone(), true)));
        assert_eq!(system, Ok((actual, true)));
        assert_eq!(explicit, Err(Problem::Unsafe));
    }

    #[test]
    fn redirected_documents_open_the_actual_recycle_bin_without_changing_settings() {
        let (_temp, base) = fixture();
        let physical = base
            .join("MovedDocuments")
            .join(super::super::DATA_DIRECTORY);
        let recycle = physical.join("recycle-bin");
        fs::create_dir_all(&recycle).unwrap();
        fs::write(physical.join("config.toml"), b"existing settings").unwrap();
        let documents = base.join("Documents");
        junction(&documents, physical.parent().unwrap());
        let old = documents.join(super::super::DATA_DIRECTORY);
        let pointer = base.join("bootstrap").join("data-location.json");
        let root = locate_root_with_default(&pointer, || Ok(old.clone()));
        fs::remove_dir(documents).unwrap();

        let root = root.unwrap();
        assert_eq!(root.path, physical);
        assert_eq!(root.documents_alias.as_deref(), Some(old.as_path()));
        assert!(!root.custom && !pointer.exists());
        assert!(
            crate::recycle_bin::list(&root.path.join("recycle-bin"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fs::read(root.path.join("config.toml")).unwrap(),
            b"existing settings"
        );
    }

    #[test]
    fn rejects_link_at_data_leaf_and_preserves_the_target() {
        let (_temp, base) = fixture();
        let target = base.join("OtherDirectory");
        fs::create_dir(&target).unwrap();
        let sentinel = target.join("keep.txt");
        fs::write(&sentinel, "keep").unwrap();
        let link = base.join(super::super::DATA_DIRECTORY);
        junction(&link, &target);
        let selected = resolve_root(&link, true, false);
        let allow_missing = resolve_root(&link, true, true);
        fs::remove_dir(link).unwrap();
        assert_eq!(selected, Err(Problem::Unsafe));
        assert_eq!(allow_missing, Err(Problem::Unsafe));
        assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
    }

    #[test]
    fn rejects_shortcuts_unc_and_bepinex_but_distinguishes_missing_parent() {
        let (_temp, base) = fixture();
        let name = super::super::DATA_DIRECTORY;
        let shortcut = base.join("Documents.LNK");
        fs::write(&shortcut, []).unwrap();
        assert_eq!(
            resolve_root(&shortcut.join(name), true, true),
            Err(Problem::NotDirectory)
        );
        assert_eq!(
            resolve_root(&base.join("Documents").join(name), true, true),
            Err(Problem::NotDirectory)
        );
        assert_eq!(
            resolve_root(&base.join("Missing").join(name), true, true),
            Err(Problem::Unavailable)
        );
        assert_eq!(
            resolve_root(&Path::new(r"\\example\share").join(name), true, true),
            Err(Problem::Unsafe)
        );
        let framework = base.join("BepInEx");
        fs::create_dir(&framework).unwrap();
        assert_eq!(
            resolve_root(&framework.join(name), true, true),
            Err(Problem::Unsafe)
        );
    }
}
