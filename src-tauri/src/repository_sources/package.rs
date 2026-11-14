//! One immutable official-source archive shared by every document in a refresh.

use super::transport::MAX_SOURCE_BYTES;
use crate::config::ConfigError;
use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Cursor, Read};
use std::sync::{Arc, OnceLock};

mod download;
pub(crate) use download::fetch;

pub(super) const MAX_ARCHIVE_BYTES: usize = 32 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;

tokio::task_local! { static ACTIVE: Arc<SourcePackage>; }

#[derive(Debug)]
pub(crate) struct SourcePackage {
    documents: BTreeMap<String, String>,
}

impl SourcePackage {
    pub(crate) fn bundled() -> Arc<Self> {
        static BUNDLED: OnceLock<Arc<SourcePackage>> = OnceLock::new();
        Arc::clone(BUNDLED.get_or_init(|| {
            Arc::new(Self {
                documents: super::embedded::bundled_files()
                    .iter()
                    .map(|(path, text)| (path.to_ascii_lowercase(), (*text).to_owned()))
                    .collect(),
            })
        }))
    }

    pub(crate) fn read(&self, path: &str) -> Result<&str, ConfigError> {
        let decoded = decode_path(path)?;
        let path = member_path(&decoded, false)?;
        if !path.ends_with(".toml") {
            return Err(ConfigError::Invalid("source.builtinNotToml".into()));
        }
        self.documents
            .get(&path)
            .map(String::as_str)
            .ok_or_else(|| {
                ConfigError::Read(crate::locale_tag::with_params(
                    "source.packageFileMissing",
                    &[("path", &path)],
                ))
            })
    }

    // Boxing happens before constructing the scoped future, so callers do not
    // inherit an arbitrarily large work future in Tauri's initial IPC state.
    pub(crate) fn in_snapshot<T>(
        package: Arc<Self>,
        work: impl Future<Output = T>,
    ) -> impl Future<Output = T> {
        ACTIVE.scope(package, Box::pin(work))
    }

    pub(crate) fn active() -> Option<Arc<Self>> {
        ACTIVE.try_with(Arc::clone).ok()
    }

    fn from_zip(bytes: &[u8]) -> Result<Self, ConfigError> {
        let count = check_envelope(bytes)?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| invalid("Source archive is not a readable ZIP"))?;
        // ZIP readers can collapse duplicate central-directory names. Compare
        // against the bounded original directory before trusting indexed reads.
        if archive.len() != count {
            return Err(invalid("Duplicate ZIP members"));
        }
        let mut members = BTreeMap::new();
        let mut documents = BTreeMap::new();
        let mut expanded = 0u64;
        for index in 0..count {
            let mut file = archive
                .by_index(index)
                .map_err(|_| invalid("Unsupported or encrypted ZIP member"))?;
            let directory = file.is_dir();
            let raw_name = std::str::from_utf8(file.name_raw())
                .map_err(|_| ConfigError::Invalid("source.fileNameNotUtf8".into()))?;
            let path = member_path(raw_name, directory)?;
            let kind = file.unix_mode().unwrap_or(0) & 0o170000;
            if !matches!(kind, 0 | 0o100000 | 0o040000) || (kind == 0o040000 && !directory) {
                return Err(invalid("Links and special ZIP members are not supported"));
            }
            if members.insert(path.clone(), directory).is_some() {
                return Err(invalid("Duplicate ZIP member paths"));
            }
            let document = !directory && path.ends_with(".toml");
            let limit = if directory {
                0
            } else if document {
                (MAX_SOURCE_BYTES as u64).min(MAX_EXPANDED_BYTES - expanded)
            } else {
                MAX_EXPANDED_BYTES - expanded
            };
            let (size, content) = read_member(&mut file, limit, document)?;
            if size != file.size() {
                return Err(invalid("ZIP member size does not match its directory"));
            }
            expanded += size;
            if document {
                let text = String::from_utf8(content)
                    .map_err(|_| invalid("Source document is not UTF-8"))?;
                documents.insert(path, text);
            }
        }
        // A file must never also act as another member's directory.
        for path in members.keys() {
            for (index, _) in path.match_indices('/') {
                if members.get(&path[..index]) == Some(&false) {
                    return Err(invalid("ZIP member conflicts with a parent file"));
                }
            }
        }
        let roots = documents
            .keys()
            .filter(|path| path.rsplit('/').next() == Some("mods.toml"))
            .collect::<Vec<_>>();
        if roots.len() != 1 {
            return Err(invalid(
                "Source archive must contain exactly one mods.toml root",
            ));
        }
        let prefix = roots[0].strip_suffix("mods.toml").unwrap().to_owned();
        for (path, directory) in &members {
            if *directory && prefix.starts_with(&format!("{path}/")) {
                continue;
            }
            if !path.starts_with(&prefix) {
                return Err(invalid("ZIP members are outside the source root"));
            }
        }
        Ok(Self {
            documents: documents
                .into_iter()
                .map(|(path, text)| (path[prefix.len()..].to_owned(), text))
                .collect(),
        })
    }
}

fn invalid(reason: &str) -> ConfigError {
    ConfigError::Invalid(crate::locale_tag::with_params(
        "source.tomlInvalid",
        &[("reason", reason)],
    ))
}

fn too_large() -> ConfigError {
    ConfigError::Invalid("source.tooLarge".into())
}

fn member_path(name: &str, directory: bool) -> Result<String, ConfigError> {
    let name = if directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if name.is_empty()
        || name.len() > 4096
        || name.starts_with('/')
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || name.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(ConfigError::Invalid("source.builtinPathUnsafe".into()));
    }
    Ok(name.to_ascii_lowercase())
}

fn decode_path(path: &str) -> Result<String, ConfigError> {
    let mut decoded = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().and_then(|byte| char::from(byte).to_digit(16));
            let low = bytes.next().and_then(|byte| char::from(byte).to_digit(16));
            let (Some(high), Some(low)) = (high, low) else {
                return Err(ConfigError::Invalid(
                    "source.fileNameEncodingInvalid".into(),
                ));
            };
            decoded.push((high * 16 + low) as u8);
        } else {
            // A plus in a URL path is a literal plus, not form-urlencoded space.
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).map_err(|_| ConfigError::Invalid("source.fileNameNotUtf8".into()))
}

fn read_member(
    reader: &mut impl Read,
    limit: u64,
    keep: bool,
) -> Result<(u64, Vec<u8>), ConfigError> {
    let mut size = 0u64;
    let mut content = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| invalid("ZIP member is truncated or has an invalid checksum"))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > limit {
            return Err(too_large());
        }
        if keep {
            content.extend_from_slice(&buffer[..read]);
        }
    }
    Ok((size, content))
}

// Bound the central-directory envelope before ZipArchive allocates its index.
// Payload decoding, local-header consistency and CRC validation stay in zip.
// ZIP64/multidisk are unnecessary for a package capped at 32 MiB/4096 members.
fn check_envelope(bytes: &[u8]) -> Result<usize, ConfigError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(too_large());
    }
    if !bytes.starts_with(&[80, 75, 3, 4]) || bytes.len() < 22 {
        return Err(invalid("Source archive is not a ZIP"));
    }
    let short =
        |offset: usize| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
    let long =
        |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let end = (bytes.len().saturating_sub(65557)..=bytes.len() - 22)
        .rev()
        .find(|&offset| {
            bytes[offset..].starts_with(&[80, 75, 5, 6])
                && offset + 22 + short(offset + 20) == bytes.len()
        })
        .ok_or_else(|| invalid("ZIP end directory is missing"))?;
    let count = short(end + 10);
    if count > MAX_ENTRIES {
        return Err(too_large());
    }
    if short(end + 4) != 0 || short(end + 6) != 0 || short(end + 8) != count {
        return Err(invalid("Multidisk ZIP is not supported"));
    }
    let mut position = long(end + 16);
    if position.checked_add(long(end + 12)) != Some(end) {
        return Err(invalid("ZIP directory bounds are invalid"));
    }
    let mut expanded = 0u64;
    for _ in 0..count {
        let header = bytes
            .get(position..position.saturating_add(46))
            .ok_or_else(|| invalid("ZIP directory is truncated"))?;
        if !header.starts_with(&[80, 75, 1, 2]) || header[8] & 1 != 0 {
            return Err(invalid("Invalid or encrypted ZIP member"));
        }
        let name_end = position + 46 + short(position + 28);
        let next = name_end + short(position + 30) + short(position + 32);
        if next > end {
            return Err(invalid("ZIP directory member is truncated"));
        }
        let name = &bytes[position + 46..name_end];
        let size = long(position + 24) as u64;
        expanded += size;
        if expanded > MAX_EXPANDED_BYTES
            || (name.len() >= 5
                && name[name.len() - 5..].eq_ignore_ascii_case(b".toml")
                && size > MAX_SOURCE_BYTES as u64)
        {
            return Err(too_large());
        }
        position = next;
    }
    if position != end {
        return Err(invalid("ZIP directory count does not match its members"));
    }
    Ok(count)
}

#[cfg(test)]
mod tests;
