use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::errors::{INCOMPLETE, INVALID_RECORD, UNSAFE_PATH, io_error};
use super::file_io::matches_fingerprint;
use super::paths::{checked_path, is_link, record_directory, validate_absolute_path};
use super::records::{RecycleRecord, load_record};

#[derive(Clone, Debug)]
pub(crate) struct VerifiedPayloadFile {
    pub(crate) relative: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct VerifiedPayloadRecord {
    pub(crate) id: String,
    pub(crate) deleted_at: String,
    pub(crate) game_path: PathBuf,
    pub(crate) files: Vec<VerifiedPayloadFile>,
    pub(crate) diychef_resources_imported: bool,
}

pub(crate) fn verified_payload_records(
    root: &Path,
    game_root: &Path,
    include: impl Fn(&str) -> bool,
) -> Result<Vec<VerifiedPayloadRecord>, String> {
    validate_absolute_path(root)?;
    validate_absolute_path(game_root)?;
    let game_root = game_root.canonicalize().map_err(io_error)?;
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut verified = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                log::warn!("回收站 payload 记录目录读取失败：{}", error);
                continue;
            }
        };
        let id = entry.file_name().to_string_lossy().into_owned();
        let directory = match record_directory(root, &id) {
            Ok(directory) => directory,
            Err(error) => {
                log::warn!(
                    "回收站 payload 记录目录无效，跳过：ID={}，原因={}",
                    id,
                    error
                );
                continue;
            }
        };
        let record = match load_record(&directory, &id) {
            Ok(record) => record,
            Err(error) => {
                log::warn!("回收站 payload 记录无效，跳过：ID={}，原因={}", id, error);
                continue;
            }
        };
        if record.state != super::records::RecordState::Ready || record.diychef_resources_imported {
            continue;
        }
        // Metadata filtering only selects candidates. A matching record still
        // has its entire payload validated before any paths are exposed.
        if !record.files.iter().any(|file| include(&file.relative)) {
            continue;
        }
        let recorded_game = Path::new(&record.game_path);
        let record_game_path = match validate_absolute_path(recorded_game)
            .and_then(|()| recorded_game.canonicalize().map_err(io_error))
        {
            Ok(path) => path,
            Err(error) => {
                log::warn!(
                    "回收站 payload 游戏路径无法解析，跳过：ID={}，原因={}",
                    id,
                    error
                );
                continue;
            }
        };
        if record_game_path != game_root {
            continue;
        }

        match verify_record_payload(root, &id, &record, record_game_path) {
            Ok(record) => verified.push(record),
            Err(error) => log::warn!("回收站 payload 校验失败，跳过：ID={}，原因={}", id, error),
        }
    }

    verified.sort_by_cached_key(|record| {
        (
            std::cmp::Reverse(chrono::DateTime::parse_from_rfc3339(&record.deleted_at).ok()),
            record.id.clone(),
        )
    });
    Ok(verified)
}

fn verify_record_payload(
    root: &Path,
    id: &str,
    record: &RecycleRecord,
    game_path: PathBuf,
) -> Result<VerifiedPayloadRecord, String> {
    let directory = record_directory(root, id)?;
    let payload = checked_path(&directory, "files")?;
    validate_payload(&payload, record)?;

    let mut files = Vec::with_capacity(record.files.len());
    for file in &record.files {
        let path = checked_path(&payload, &file.relative)?;
        if !matches_fingerprint(&path, file)? {
            return Err(format!("文件指纹不匹配: {}", file.relative));
        }
        files.push(VerifiedPayloadFile {
            relative: file.relative.clone(),
            path,
        });
    }

    Ok(VerifiedPayloadRecord {
        id: id.to_owned(),
        deleted_at: record.deleted_at.clone(),
        game_path,
        files,
        diychef_resources_imported: record.diychef_resources_imported,
    })
}

pub(super) fn validate_payload(payload: &Path, record: &RecycleRecord) -> Result<(), String> {
    validate_absolute_path(payload)?;
    let expected = record
        .files
        .iter()
        .map(|file| file.relative.to_lowercase())
        .collect::<HashSet<_>>();
    let mut observed = HashSet::new();
    for entry in WalkDir::new(payload).follow_links(false) {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if is_link(&metadata) {
            return Err(UNSAFE_PATH.into());
        }
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(INVALID_RECORD.into());
        }
        if metadata.is_file() {
            let relative = entry
                .path()
                .strip_prefix(payload)
                .map_err(|_| UNSAFE_PATH.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let key = relative.to_lowercase();
            if !expected.contains(&key) || !observed.insert(key) {
                // Omitted files must block readers and restore alike, never be discarded.
                // Case-sensitive volumes must not hide an extra case-only alias.
                return Err(INCOMPLETE.into());
            }
        }
    }
    Ok(())
}
