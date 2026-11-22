use flate2::read::GzDecoder;
use plist::Value;
use semver::Version;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_PATH_BYTES: usize = 1024;
const MAX_PLIST_BYTES: u64 = 64 * 1024;
const PLIST_PATH: &str = "Contents/Info.plist";
const EXECUTABLE_DIR: &str = "Contents/MacOS/";

type ValidationResult<T> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn installed_bundle_identifier(bundle: &Path) -> ValidationResult<String> {
    let value = read_plist(File::open(bundle.join(PLIST_PATH))?)?;
    Ok(plist_string(&value, "CFBundleIdentifier")?.to_owned())
}

pub(super) fn verify_archive(
    bytes: &[u8],
    version: &str,
    identifier: &str,
) -> ValidationResult<()> {
    let mut decoder = GzDecoder::new(bytes).take(MAX_ARCHIVE_BYTES + 1);
    let mut app_root = None;
    let mut plist = None;
    let mut executables = HashSet::new();
    let mut paths = HashMap::new();
    for (index, entry) in tar::Archive::new(&mut decoder)
        .entries()?
        .raw(true)
        .enumerate()
    {
        if index >= MAX_ARCHIVE_ENTRIES {
            return Err("too many entries in update archive".into());
        }
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if !kind.is_dir() && !kind.is_file() {
            return Err("unsupported entry type in update archive".into());
        }
        let path = entry.path_bytes();
        let (root, relative) = archive_path(&path)?;
        if app_root.get_or_insert_with(|| root.to_owned()).as_str() != root {
            return Err("multiple app roots in update archive".into());
        }
        if relative.is_empty() && !kind.is_dir() {
            return Err("update archive app root is not a directory".into());
        }
        if paths
            .insert(relative.to_lowercase(), kind.is_dir())
            .is_some()
        {
            return Err("duplicate entry in update archive".into());
        }
        if relative == PLIST_PATH {
            if !kind.is_file() {
                return Err("update archive Info.plist is not a regular file".into());
            }
            plist = Some(read_plist(&mut entry)?);
        } else if let Some(name) = relative.strip_prefix(EXECUTABLE_DIR)
            && valid_component(name)
            && kind.is_file()
            && entry.size() > 0
            && entry.header().mode()? & 0o111 != 0
        {
            executables.insert(name.to_owned());
        }
    }
    io::copy(&mut decoder, &mut io::sink())?;
    if decoder.limit() == 0 {
        return Err("expanded update archive exceeds size limit".into());
    }
    for path in paths.keys() {
        let mut prefix = path.as_str();
        while let Some((parent, _)) = prefix.rsplit_once('/') {
            if paths.get(parent) == Some(&false) {
                return Err("update archive file is also a parent directory".into());
            }
            prefix = parent;
        }
    }
    let plist = plist.ok_or("missing Info.plist in update archive")?;
    let name = bundle_executable(&plist, version, identifier)?;
    if !executables.contains(name) {
        return Err("update archive is missing the declared executable".into());
    }
    Ok(())
}

fn archive_path(bytes: &[u8]) -> ValidationResult<(&str, &str)> {
    if bytes.len() > MAX_PATH_BYTES {
        return Err("update archive path exceeds size limit".into());
    }
    let path = std::str::from_utf8(bytes)?;
    let path = path.strip_suffix('/').unwrap_or(path);
    let (root, relative) = path.split_once('/').unwrap_or((path, ""));
    if !path.split('/').all(valid_component) || !root.ends_with(".app") || root == ".app" {
        return Err("invalid app path in update archive".into());
    }
    Ok((root, relative))
}

fn valid_component(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['/', '\\', '\0'])
}

fn read_plist(reader: impl Read) -> ValidationResult<Value> {
    let mut bytes = Vec::new();
    reader.take(MAX_PLIST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PLIST_BYTES {
        return Err("app Info.plist exceeds size limit".into());
    }
    Ok(Value::from_reader(Cursor::new(bytes))?)
}

fn plist_string<'a>(plist: &'a Value, key: &str) -> ValidationResult<&'a str> {
    plist
        .as_dictionary()
        .and_then(|properties| properties.get(key))
        .and_then(Value::as_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("app Info.plist is missing {key}").into())
}

fn bundle_executable<'a>(
    plist: &'a Value,
    version: &str,
    identifier: &str,
) -> ValidationResult<&'a str> {
    if plist_string(plist, "CFBundleIdentifier")? != identifier {
        return Err("app bundle identifier does not match the installed app".into());
    }
    let actual = Version::parse(plist_string(plist, "CFBundleShortVersionString")?)?;
    if actual != Version::parse(version)? {
        return Err("app bundle version does not match the signed update".into());
    }
    let name = plist_string(plist, "CFBundleExecutable")?;
    if !valid_component(name) {
        return Err("invalid app bundle executable name".into());
    }
    Ok(name)
}

pub(super) fn verify_installed_app(
    bundle: &Path,
    version: &str,
    identifier: &str,
) -> ValidationResult<()> {
    let value = read_plist(File::open(bundle.join(PLIST_PATH))?)?;
    let name = bundle_executable(&value, version, identifier)?;
    let metadata = fs::symlink_metadata(bundle.join(EXECUTABLE_DIR).join(name))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("installed bundle is missing the declared executable".into());
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err("installed bundle's declared executable is not executable".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, EntryType, Header};

    const IDENTIFIER: &str = "com.example.modmanager";
    const VERSION: &str = "2.4.1";
    const APP_PLIST: &str = "OC2ModManager.app/Contents/Info.plist";
    const APP_EXECUTABLE: &str = "OC2ModManager.app/Contents/MacOS/RenamedExecutable";

    fn plist_bytes(executable: &str) -> Vec<u8> {
        let mut properties = plist::Dictionary::new();
        properties.insert(
            "CFBundleIdentifier".into(),
            Value::String(IDENTIFIER.into()),
        );
        properties.insert(
            "CFBundleShortVersionString".into(),
            Value::String(VERSION.into()),
        );
        properties.insert(
            "CFBundleExecutable".into(),
            Value::String(executable.into()),
        );
        let mut bytes = Vec::new();
        Value::Dictionary(properties)
            .to_writer_xml(&mut bytes)
            .unwrap();
        bytes
    }

    fn package(entries: &[(&str, EntryType, u32, &[u8])]) -> Vec<u8> {
        let mut archive = Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for &(path, kind, mode, contents) in entries {
            let mut header = Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(contents.len() as u64);
            header.set_mode(mode);
            header.as_gnu_mut().unwrap().name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_cksum();
            archive.append(&header, contents).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn accepts_bundle_with_renamed_executable_before_plist() {
        let bytes = package(&[
            (APP_EXECUTABLE, EntryType::Regular, 0o755, b"executable"),
            (
                APP_PLIST,
                EntryType::Regular,
                0o644,
                &plist_bytes("RenamedExecutable"),
            ),
        ]);
        assert!(verify_archive(&bytes, VERSION, IDENTIFIER).is_ok());
        assert!(verify_archive(&bytes, "2.4.1+other", IDENTIFIER).is_err());
        assert!(verify_archive(&bytes, VERSION, "com.example.other").is_err());
    }

    #[test]
    fn rejects_bundle_with_unsafe_layout() {
        let plist = plist_bytes("RenamedExecutable");
        let mandatory = [
            (APP_PLIST, EntryType::Regular, 0o644, &plist[..]),
            (APP_EXECUTABLE, EntryType::Regular, 0o755, &b"exe"[..]),
        ];
        for path in [
            APP_PLIST,
            "OC2ModManager.app/contents/info.plist",
            "OC2ModManager.app/Contents/MacOS",
            "OC2ModManager.app/Contents/../Outside",
            "OC2ModManager.app/Contents//Info.plist",
            "/OC2ModManager.app/Contents/Outside",
            "Other.app/Contents/Outside",
        ] {
            let mut entries = mandatory.to_vec();
            entries.push((path, EntryType::Regular, 0o644, b"extra"));
            assert!(
                verify_archive(&package(&entries), VERSION, IDENTIFIER).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_missing_or_unusable_bundle_files() {
        let plist = plist_bytes("RenamedExecutable");
        let plist_entry = (APP_PLIST, EntryType::Regular, 0o644, &plist[..]);
        let executable = (APP_EXECUTABLE, EntryType::Regular, 0o755, &b"exe"[..]);
        let large_plist = vec![b'x'; MAX_PLIST_BYTES as usize + 1];
        let invalid_name = plist_bytes("../RenamedExecutable");
        let scenarios: &[&[(&str, EntryType, u32, &[u8])]] = &[
            &[executable],
            &[plist_entry],
            &[
                plist_entry,
                (APP_EXECUTABLE, EntryType::Symlink, 0o755, b""),
            ],
            &[
                plist_entry,
                (APP_EXECUTABLE, EntryType::Regular, 0o644, b"exe"),
            ],
            &[
                plist_entry,
                (APP_EXECUTABLE, EntryType::Regular, 0o755, b""),
            ],
            &[
                executable,
                (APP_PLIST, EntryType::Regular, 0o644, &large_plist),
            ],
            &[
                executable,
                (APP_PLIST, EntryType::Regular, 0o644, &invalid_name),
            ],
        ];
        for entries in scenarios {
            assert!(verify_archive(&package(entries), VERSION, IDENTIFIER).is_err());
        }
    }
}
