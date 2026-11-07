use super::errors::INVALID_RECORD;
use super::errors::io_error;
use super::file_io::MAX_RECORD_BYTES;
use super::file_io::read_bounded;
use super::file_io::write_atomic;
use super::paths::checked_path;
use super::paths::file_key;
use super::paths::normalize_relative;
use super::paths::record_directory;
use super::paths::valid_package_id;
use super::paths::validate_absolute_path;
use super::paths::validate_relative;
use crate::archive::InstallManifest;
use crate::models::ModRecycleEntry;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecordState {
    Preparing,
    Ready,
    Restored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StagedFile {
    pub(super) relative: String,
    pub(super) size: u64,
    pub(super) sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RelatedManifest {
    pub(super) package_id: String,
    pub(super) original: String,
    pub(super) removed_files: Vec<String>,
}

impl RelatedManifest {
    pub(super) fn manifest(&self) -> Result<InstallManifest, String> {
        let manifest: InstallManifest =
            toml::from_str(&self.original).map_err(|_| INVALID_RECORD.to_string())?;
        if manifest.repository_id != self.package_id || self.removed_files.is_empty() {
            return Err(INVALID_RECORD.into());
        }
        for file in &manifest.files {
            normalize_relative(file)?;
        }
        for removed in &self.removed_files {
            normalize_relative(removed)?;
            if !manifest
                .files
                .iter()
                .any(|file| file_key(file) == file_key(removed))
            {
                return Err(INVALID_RECORD.into());
            }
        }
        Ok(manifest)
    }

    pub(super) fn remaining(&self) -> Result<Option<InstallManifest>, String> {
        let mut manifest = self.manifest()?;
        manifest.files.retain(|file| {
            !self
                .removed_files
                .iter()
                .any(|removed| file_key(file) == file_key(removed))
        });
        Ok((!manifest.files.is_empty()).then_some(manifest))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecycleRecord {
    pub(super) id: String,
    pub(super) name: String,
    /// 删除时仓库名的多语言快照。
    ///
    /// 为什么存快照而不是删除后去查仓库：MOD 已经从磁盘移走了，
    /// 回收站列表要能独立回答「这是哪个 MOD」，不该依赖当前配置里
    /// 是否还存在那个仓库（仓库可能已从源里下线）。
    ///
    /// 渲染时按界面语言从这里现选，所以切语言后**历史记录也会跟着变**。
    /// 旧记录没有这个字段，回退到单语 `name`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) name_i18n: BTreeMap<String, String>,
    pub(super) deleted_at: String,
    pub(super) game_path: String,
    pub(super) package_id: Option<String>,
    pub(super) release_tag: Option<String>,
    // Only these manifest entries are merged back for partial removals.
    pub(super) removed_manifest_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) related_manifests: Vec<RelatedManifest>,
    pub(super) files: Vec<StagedFile>,
    pub(super) state: RecordState,
}

impl RecycleRecord {
    pub(super) fn display(&self) -> ModRecycleEntry {
        ModRecycleEntry {
            id: self.id.clone(),
            name: self.name.clone(),
            name_i18n: self.name_i18n.clone(),
            deleted_at: self.deleted_at.clone(),
            file_count: self.files.len(),
            files: self
                .files
                .iter()
                .map(|file| file.relative.clone())
                .collect(),
            game_path: self.game_path.clone(),
            package_id: self.package_id.clone(),
            release_tag: self.release_tag.clone(),
        }
    }
}

pub(super) fn save_record(directory: &Path, record: &RecycleRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record).map_err(io_error)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(INVALID_RECORD.into());
    }
    write_atomic(&checked_path(directory, "record.json")?, &bytes)
}

pub(super) fn load_record(directory: &Path, id: &str) -> Result<RecycleRecord, String> {
    let record_path = checked_path(directory, "record.json")?;
    if !record_path.try_exists().map_err(io_error)? {
        return Err("recycle.recordMissing".into());
    }
    let record: RecycleRecord = serde_json::from_slice(&read_bounded(&record_path)?)
        .map_err(|_| INVALID_RECORD.to_string())?;
    if record.id != id
        || record.files.len() > 100_000
        || record
            .package_id
            .as_deref()
            .is_some_and(|id| !valid_package_id(id))
        || !Path::new(&record.game_path).is_absolute()
        || chrono::DateTime::parse_from_rfc3339(&record.deleted_at).is_err()
    {
        return Err(INVALID_RECORD.into());
    }
    let mut unique = HashSet::new();
    for file in &record.files {
        validate_relative(&file.relative)?;
        if !unique.insert(file.relative.to_lowercase())
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|value| value.is_ascii_hexdigit())
        {
            return Err(INVALID_RECORD.into());
        }
    }
    for relative in &record.removed_manifest_files {
        validate_relative(relative)?;
    }
    let mut packages = HashSet::new();
    if let Some(id) = &record.package_id {
        packages.insert(id.to_lowercase());
    }
    for related in &record.related_manifests {
        if record.package_id.is_none()
            || !valid_package_id(&related.package_id)
            || !packages.insert(related.package_id.to_lowercase())
            || related.removed_files.iter().any(|file| {
                !record
                    .removed_manifest_files
                    .iter()
                    .any(|removed| file_key(file) == file_key(removed))
            })
        {
            return Err(INVALID_RECORD.into());
        }
        related.manifest()?;
    }
    Ok(record)
}

pub fn list(root: &Path) -> Result<Vec<ModRecycleEntry>, String> {
    log::debug!("读取 MOD 回收站");
    validate_absolute_path(root)?;
    if !root.try_exists().map_err(io_error)? {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let id = entry.file_name().to_string_lossy().into_owned();
        let result = record_directory(root, &id).and_then(|directory| load_record(&directory, &id));
        match result {
            Ok(record) if record.state != RecordState::Restored => entries.push(record.display()),
            Ok(_) => (),
            Err(error) => log::warn!("忽略无效 MOD 回收记录：ID={}，原因={}", id, error),
        }
    }
    entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(entries)
}

pub fn original_game_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    let directory = record_directory(root, id)?;
    let record = load_record(&directory, id)?;
    Ok(PathBuf::from(record.game_path))
}
