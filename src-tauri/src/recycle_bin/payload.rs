use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::errors::{UNSAFE_PATH, VERIFY_FILES, io_error};
use super::file_io::matches_fingerprint;
use super::paths::{
    checked_path_for, is_link, record_directory, validate_absolute_path, validate_absolute_path_for,
};
use super::records::{RecycleRecord, load_record};
use crate::file_check;

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
    let payload = checked_path_for(&directory, "files", Some(VERIFY_FILES))?;
    validate_payload(&payload, record)?;

    let mut files = Vec::with_capacity(record.files.len());
    for file in &record.files {
        let path = checked_path_for(&payload, &file.relative, Some(VERIFY_FILES))?;
        if !matches_fingerprint(&path, file, Some(VERIFY_FILES))? {
            return Err(file_check::report(VERIFY_FILES, &path, file_check::CONTENT));
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
    validate_absolute_path_for(payload, Some(VERIFY_FILES))?;
    let expected = record
        .files
        .iter()
        .map(|file| file.relative.to_lowercase())
        .collect::<HashSet<_>>();
    let mut observed = HashSet::new();
    for entry in WalkDir::new(payload).follow_links(false) {
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(payload).to_path_buf();
            match error.into_io_error() {
                Some(cause) => file_check::report_io(VERIFY_FILES, &path, cause, file_check::READ),
                None => file_check::report(VERIFY_FILES, &path, file_check::READ),
            }
        })?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            file_check::report_io(VERIFY_FILES, entry.path(), error, file_check::READ)
        })?;
        if is_link(&metadata) {
            return Err(file_check::report(
                VERIFY_FILES,
                entry.path(),
                file_check::UNSAFE,
            ));
        }
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(file_check::report(
                VERIFY_FILES,
                entry.path(),
                file_check::TYPE,
            ));
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
                return Err(file_check::report(
                    VERIFY_FILES,
                    entry.path(),
                    "recycle.fileNotRecorded",
                ));
            }
        }
    }
    Ok(())
}
