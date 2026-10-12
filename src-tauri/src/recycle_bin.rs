//! Recoverable MOD removal. The active manifest and staged files are one transaction;
//! online reinstallation also stages the previous package before writing its replacement.
use crate::archive::{self, InstallManifest};
use crate::models::ModRecycleEntry;
use crate::mods;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const UNSAFE_PATH: &str = "回收站路径不安全";
const INVALID_RECORD: &str = "回收站记录无效";
const IO_ERROR: &str = "回收站文件操作失败";
const CONFLICT: &str = "恢复位置已有文件，拒绝覆盖";
const INCOMPLETE: &str = "回收站文件不完整或已被修改";
const NO_FILES: &str = "没有可移入回收站的文件，安装清单保持不变";
const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RecordState {
    Preparing,
    Ready,
    Restored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedFile {
    relative: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RelatedManifest {
    package_id: String,
    original: String,
    removed_files: Vec<String>,
}

impl RelatedManifest {
    fn manifest(&self) -> Result<InstallManifest, String> {
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

    fn remaining(&self) -> Result<Option<InstallManifest>, String> {
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
struct RecycleRecord {
    id: String,
    name: String,
    deleted_at: String,
    game_path: String,
    package_id: Option<String>,
    release_tag: Option<String>,
    // Only these manifest entries are merged back for partial removals.
    removed_manifest_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    related_manifests: Vec<RelatedManifest>,
    files: Vec<StagedFile>,
    state: RecordState,
}

impl RecycleRecord {
    fn display(&self) -> ModRecycleEntry {
        ModRecycleEntry {
            id: self.id.clone(),
            name: self.name.clone(),
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

fn io_error(error: impl std::fmt::Display) -> String {
    log::error!("回收站文件操作失败：{}", error);
    #[cfg(test)]
    eprintln!("回收站测试 I/O 诊断：{}", error);
    IO_ERROR.into()
}

fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions and other reparse points, not only symbolic links.
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

fn validate_absolute_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(UNSAFE_PATH.into());
    }
    let mut checked = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err(UNSAFE_PATH.into());
        }
        checked.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            // Windows verbatim drive prefixes (\\?\C:) only become a valid
            // filesystem path after the following RootDir separator.
            continue;
        }
        match fs::symlink_metadata(&checked) {
            Ok(metadata) if is_link(&metadata) => return Err(UNSAFE_PATH.into()),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn validate_relative(relative: &str) -> Result<(), String> {
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
        return Err(UNSAFE_PATH.into());
    }
    Ok(())
}

fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative(relative)?;
    let path = root.join(relative);
    validate_absolute_path(&path)?;
    Ok(path)
}

fn normalize_relative(relative: &str) -> Result<String, String> {
    let normalized = relative.replace('\\', "/");
    validate_relative(&normalized)?;
    Ok(normalized)
}

fn valid_package_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id != "."
        && id != ".."
        && !id.ends_with(['.', ' '])
        && id
            .chars()
            .all(|value| value.is_alphanumeric() || "-_ .".contains(value))
}

fn manifest_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_package_id(id) {
        return Err(INVALID_RECORD.into());
    }
    checked_path(directory, &format!("{id}.toml"))
}

fn record_directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    if Uuid::parse_str(id)
        .ok()
        .is_none_or(|parsed| parsed.to_string() != id)
    {
        return Err(INVALID_RECORD.into());
    }
    checked_path(root, id)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(INVALID_RECORD.into());
    }
    fs::read(path).map_err(io_error)
}

fn read_active_manifest(
    directory: &Path,
    id: &str,
) -> Result<Option<(InstallManifest, Vec<u8>)>, String> {
    let path = manifest_path(directory, id)?;
    if !path.try_exists().map_err(io_error)? {
        return Ok(None);
    }
    let bytes = read_bounded(&path)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| INVALID_RECORD.to_string())?;
    let manifest: InstallManifest = toml::from_str(text).map_err(|_| INVALID_RECORD.to_string())?;
    if manifest.repository_id != id {
        return Err(INVALID_RECORD.into());
    }
    for relative in &manifest.files {
        normalize_relative(relative)?;
    }
    Ok(Some((manifest, bytes)))
}

fn all_active_manifests(directory: &Path) -> Result<Vec<InstallManifest>, String> {
    validate_absolute_path(directory)?;
    if !directory.try_exists().map_err(io_error)? {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            let id = entry
                .path()
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or(INVALID_RECORD)?
                .to_owned();
            if let Some((manifest, _)) = read_active_manifest(directory, &id)? {
                manifests.push(manifest);
            }
        }
    }
    Ok(manifests)
}

fn file_key(relative: &str) -> String {
    mods::canonical_dll_name(&relative.replace('\\', "/")).to_lowercase()
}

fn fingerprint(path: &Path) -> Result<(u64, String), String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(UNSAFE_PATH.into());
    }
    let mut input = File::open(path).map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut total = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = input.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
        total += length as u64;
    }
    Ok((total, format!("{:x}", digest.finalize())))
}

fn matches_fingerprint(path: &Path, file: &StagedFile) -> Result<bool, String> {
    let (size, sha256) = fingerprint(path)?;
    Ok(size == file.size && sha256 == file.sha256)
}

// Cross-volume moves fall back to a verified copy. The source is removed only
// after all bytes were flushed and match the original fingerprint.
fn move_file(source: &Path, target: &Path, file: &StagedFile) -> Result<(), String> {
    validate_absolute_path(source)?;
    validate_absolute_path(target)?;
    if target.try_exists().map_err(io_error)? {
        return Err(CONFLICT.into());
    }
    if !matches_fingerprint(source, file)? {
        return Err(INCOMPLETE.into());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    if fs::rename(source, target).is_ok() {
        log::trace!("回收站文件移动完成：{}", file.relative);
        return Ok(());
    }
    copy_verified_and_remove(source, target, file)
}

fn copy_verified_and_remove(source: &Path, target: &Path, file: &StagedFile) -> Result<(), String> {
    log::debug!("回收站跨目录复制校验：{}", file.relative);
    let mut created_copy = false;
    let result = (|| {
        let mut input = File::open(source).map_err(io_error)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(io_error)?;
        created_copy = true;
        std::io::copy(&mut input, &mut output).map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        if !matches_fingerprint(target, file)? || !matches_fingerprint(source, file)? {
            return Err(INCOMPLETE.into());
        }
        archive::clear_readonly(source).map_err(io_error)?;
        fs::remove_file(source).map_err(io_error)
    })();
    if result.is_err() && created_copy && source.exists() && target.exists() {
        if let Err(error) = fs::remove_file(target) {
            log::error!("回收站复制失败的副本保留：{}", error);
        }
    }
    result
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    validate_absolute_path(path)?;
    let temp = path.with_file_name(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(io_error)?;
        output.write_all(bytes).map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        fs::rename(&temp, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn save_record(directory: &Path, record: &RecycleRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record).map_err(io_error)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(INVALID_RECORD.into());
    }
    write_atomic(&checked_path(directory, "record.json")?, &bytes)
}

fn load_record(directory: &Path, id: &str) -> Result<RecycleRecord, String> {
    let record_path = checked_path(directory, "record.json")?;
    if !record_path.try_exists().map_err(io_error)? {
        return Err("回收站记录不存在".into());
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

/// None selects the full package; Some selects only its canonical DLL.
pub fn recycle_package(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: Option<&str>,
) -> Result<ModRecycleEntry, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        dll,
        None,
        None,
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

pub fn recycle_mod_dll(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: &str,
    installed: &[crate::models::InstalledMod],
) -> Result<ModRecycleEntry, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        Some(dll),
        Some(installed),
        None,
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

/// A local delete explicitly selects this DLL, even when recycling its package.
pub fn recycle_selected_mod(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: &str,
    whole_package: bool,
) -> Result<ModRecycleEntry, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        (!whole_package).then_some(dll),
        None,
        Some(dll),
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

/// Shared files being overwritten belong to the replacement transaction too.
/// None means no old files need staging. Keep the old manifest until the
/// replacement installation atomically commits its new manifest.
pub fn recycle_before_reinstall(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    replacement_files: &[String],
) -> Result<Option<ModRecycleEntry>, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        None,
        None,
        None,
        replacement_files,
    )
}

fn has_safe_replacement(
    game_root: &Path,
    installed: &[crate::models::InstalledMod],
    dll: &str,
) -> Result<bool, String> {
    let targets = installed
        .iter()
        .filter(|item| file_key(&item.dll_file) == file_key(dll))
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(false);
    }
    // Both enabled and disabled variants can exist at the selected path.
    // A replacement must safely cover every variant that will be staged.
    for peer in installed.iter().filter(|peer| {
        targets
            .iter()
            .all(|target| mods::is_safe_replacement_mod(target, peer))
    }) {
        let path = checked_path(game_root, &mods::canonical_dll_name(&peer.dll_file))?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !is_link(&metadata) => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn recycle_package_with_mods(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: Option<&str>,
    installed: Option<&[crate::models::InstalledMod]>,
    explicit_dll: Option<&str>,
    replacement_files: &[String],
) -> Result<Option<ModRecycleEntry>, String> {
    let (manifest, original) =
        read_active_manifest(manifests_dir, package_id)?.ok_or("没有安装清单，拒绝删除未知文件")?;
    let selected = dll.map(file_key);
    let explicit = explicit_dll.map(file_key);
    let replacement_keys = replacement_files
        .iter()
        .map(|file| file_key(file))
        .collect::<HashSet<_>>();
    if explicit.as_ref().is_some_and(|selected| {
        !manifest
            .files
            .iter()
            .any(|file| file_key(file) == *selected)
    }) {
        return Err("当前 DLL 不属于这个 MOD 包".into());
    }
    let removed_files = manifest
        .files
        .iter()
        .filter(|file| {
            selected
                .as_ref()
                .is_none_or(|selected| file_key(file) == *selected)
        })
        .map(|file| normalize_relative(file))
        .collect::<Result<Vec<_>, _>>()?;
    if removed_files.is_empty() && dll.is_some() {
        return Err("当前 DLL 不属于这个 MOD 包".into());
    }
    if let Some(dll) = explicit_dll {
        let relative = normalize_relative(&mods::canonical_dll_name(dll))?;
        if existing_variants(game_root, &relative)?.is_empty() {
            return Err("找不到对应的 MOD DLL 文件".into());
        }
    }
    let peer_manifests = all_active_manifests(manifests_dir)?
        .into_iter()
        .filter(|peer| peer.repository_id != package_id)
        .collect::<Vec<_>>();
    let peer_files = peer_manifests
        .iter()
        .flat_map(|peer| peer.files.iter().map(|file| file_key(file)))
        .collect::<HashSet<_>>();
    let safe_duplicate = match (dll, installed) {
        (Some(dll), Some(installed)) => has_safe_replacement(game_root, installed, dll)?,
        _ => false,
    };
    let mut actual = Vec::new();
    for relative in &removed_files {
        if peer_files.contains(&file_key(relative)) {
            if explicit
                .as_ref()
                .is_some_and(|selected| file_key(relative) == *selected)
                || replacement_keys.contains(&file_key(relative))
            {
                actual.extend(existing_variants(game_root, relative)?);
                continue;
            }
            if dll.is_some() {
                if !safe_duplicate {
                    return Err("该文件仍由其他 MOD 包使用，不能移入回收站".into());
                }
                actual.extend(existing_variants(game_root, relative)?);
                continue;
            }
            log::debug!(
                "回收包保留其他包共享文件：包={}，文件={}",
                package_id,
                relative
            );
            continue;
        }
        actual.extend(existing_variants(game_root, relative)?);
    }
    if actual.is_empty() {
        log::debug!(
            "MOD 包没有可回收文件，保留安装清单：包={}，清单文件数={}",
            package_id,
            removed_files.len()
        );
        return Ok(None);
    }
    let name = manifest
        .package_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| package_id.to_owned());
    let name = dll
        .map(|relative| {
            Path::new(relative)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(&name)
                .to_owned()
        })
        .unwrap_or(name);
    let remaining = if dll.is_some() {
        let mut updated = manifest.clone();
        updated.files.retain(|file| {
            !removed_files
                .iter()
                .any(|removed| file_key(file) == file_key(removed))
        });
        (!updated.files.is_empty()).then_some(updated)
    } else {
        None
    };
    let mut related_manifests = Vec::new();
    if safe_duplicate || explicit.is_some() || !replacement_keys.is_empty() {
        for peer in peer_manifests {
            let removed = peer
                .files
                .iter()
                .filter(|file| {
                    removed_files
                        .iter()
                        .any(|removed| file_key(file) == file_key(removed))
                        && (safe_duplicate
                            || replacement_keys.contains(&file_key(file))
                            || explicit
                                .as_ref()
                                .is_some_and(|selected| file_key(file) == *selected))
                })
                .cloned()
                .collect::<Vec<_>>();
            if removed.is_empty() {
                continue;
            }
            let (_, bytes) =
                read_active_manifest(manifests_dir, &peer.repository_id)?.ok_or(INVALID_RECORD)?;
            related_manifests.push(RelatedManifest {
                package_id: peer.repository_id,
                original: String::from_utf8(bytes).map_err(|_| INVALID_RECORD.to_string())?,
                removed_files: removed,
            });
        }
    }
    stage_with_related_manifests(
        root,
        manifests_dir,
        game_root,
        name,
        Some(&manifest),
        Some(&original),
        removed_files,
        actual,
        remaining.as_ref(),
        related_manifests,
        &mut || Ok(()),
    )
    .map(Some)
}

pub fn recycle_unmanaged(
    root: &Path,
    game_root: &Path,
    dll: &str,
) -> Result<ModRecycleEntry, String> {
    let relative = normalize_relative(&mods::canonical_dll_name(dll))?;
    let parts = relative.split('/').collect::<Vec<_>>();
    if parts.len() < 3
        || !parts[0].eq_ignore_ascii_case("BepInEx")
        || !parts[1].eq_ignore_ascii_case("plugins")
        || !Path::new(&relative)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
    {
        return Err("只能删除 BepInEx/plugins 下的 DLL 文件".into());
    }
    let actual = existing_variants(game_root, &relative)?;
    if actual.is_empty() {
        return Err("找不到对应的 MOD DLL 文件".into());
    }
    let name = Path::new(&relative)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("MOD")
        .to_owned();
    stage(
        root,
        root,
        game_root,
        name,
        None,
        None,
        Vec::new(),
        actual,
        None,
        &mut || Ok(()),
    )
}

fn existing_variants(game_root: &Path, relative: &str) -> Result<Vec<String>, String> {
    let mut candidates = vec![relative.to_owned()];
    if Path::new(relative)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
    {
        candidates.push(format!("{relative}.disabled"));
    }
    let mut existing = Vec::new();
    for candidate in candidates {
        let path = checked_path(game_root, &candidate)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_link(&metadata) => existing.push(candidate),
            Ok(_) => return Err(UNSAFE_PATH.into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(existing)
}

#[allow(clippy::too_many_arguments)]
fn stage(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    name: String,
    manifest: Option<&InstallManifest>,
    original_manifest: Option<&[u8]>,
    removed_manifest_files: Vec<String>,
    actual_files: Vec<String>,
    remaining_manifest: Option<&InstallManifest>,
    after_move: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ModRecycleEntry, String> {
    stage_with_related_manifests(
        root,
        manifests_dir,
        game_root,
        name,
        manifest,
        original_manifest,
        removed_manifest_files,
        actual_files,
        remaining_manifest,
        Vec::new(),
        after_move,
    )
}

fn stage_with_related_manifests(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    name: String,
    manifest: Option<&InstallManifest>,
    original_manifest: Option<&[u8]>,
    removed_manifest_files: Vec<String>,
    actual_files: Vec<String>,
    remaining_manifest: Option<&InstallManifest>,
    related_manifests: Vec<RelatedManifest>,
    after_move: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ModRecycleEntry, String> {
    validate_absolute_path(root)?;
    validate_absolute_path(game_root)?;
    let canonical_game = fs::canonicalize(game_root).map_err(io_error)?;
    // A manager placed inside plugins must not have its staged DLLs scanned as
    // active MODs. Resolve the nearest existing ancestor before creating the bin.
    let existing_ancestor = root
        .ancestors()
        .find(|path| path.exists())
        .ok_or(UNSAFE_PATH)?;
    let resolved_ancestor = fs::canonicalize(existing_ancestor).map_err(io_error)?;
    let resolved_root = resolved_ancestor.join(
        root.strip_prefix(existing_ancestor)
            .map_err(|_| UNSAFE_PATH.to_string())?,
    );
    let inside_plugins = resolved_root
        .strip_prefix(&canonical_game)
        .ok()
        .is_some_and(|relative| {
            let mut parts = relative.components();
            parts
                .next()
                .is_some_and(|part| part.as_os_str().eq_ignore_ascii_case("BepInEx"))
                && parts
                    .next()
                    .is_some_and(|part| part.as_os_str().eq_ignore_ascii_case("plugins"))
        });
    if inside_plugins {
        return Err("回收站不能位于游戏的 MOD 目录内".into());
    }
    let id = Uuid::new_v4().to_string();
    let directory = record_directory(root, &id)?;
    let mut unique = HashSet::new();
    let mut files = Vec::new();
    for relative in actual_files {
        if unique.insert(relative.to_lowercase()) {
            let (size, sha256) = fingerprint(&checked_path(game_root, &relative)?)?;
            files.push(StagedFile {
                relative,
                size,
                sha256,
            });
        }
    }
    let mut record = RecycleRecord {
        id,
        name,
        deleted_at: Utc::now().to_rfc3339(),
        game_path: canonical_game.to_string_lossy().into_owned(),
        package_id: manifest.map(|manifest| manifest.repository_id.clone()),
        release_tag: manifest.map(|manifest| manifest.release_tag.clone()),
        removed_manifest_files,
        related_manifests,
        files,
        state: RecordState::Preparing,
    };
    fs::create_dir_all(&directory).map_err(io_error)?;
    let payload = checked_path(&directory, "files")?;
    fs::create_dir_all(&payload).map_err(io_error)?;
    log::info!(
        "MOD 移入回收站开始：ID={}，名称={}，文件数={}",
        record.id,
        record.name,
        record.files.len()
    );
    // Journal exists before any file is removed, including interrupted operations.
    let journal_result = (|| {
        save_record(&directory, &record)?;
        if let Some(bytes) = original_manifest {
            write_atomic(&checked_path(&directory, "manifest.toml")?, bytes)?;
        }
        Ok(())
    })();
    if let Err(error) = journal_result {
        let _ = remove_entry_directory(&directory);
        return Err(error);
    }
    let mut moved = Vec::new();
    let mut manifest_changed = false;
    let mut related_changed = 0usize;
    let result = (|| {
        for file in &record.files {
            move_file(
                &checked_path(game_root, &file.relative)?,
                &checked_path(&payload, &file.relative)?,
                file,
            )?;
            moved.push(file.clone());
            after_move()?;
        }
        if let Some(manifest) = manifest {
            set_active_manifest(manifests_dir, &manifest.repository_id, remaining_manifest)?;
            manifest_changed = true;
        }
        for related in &record.related_manifests {
            set_active_manifest(
                manifests_dir,
                &related.package_id,
                related.remaining()?.as_ref(),
            )?;
            related_changed += 1;
        }
        record.state = RecordState::Ready;
        save_record(&directory, &record)
    })();
    if let Err(error) = result {
        log::error!("MOD 回收失败，回滚：ID={}，原因={}", record.id, error);
        let files_rolled_back = rollback_moves(&payload, game_root, &moved);
        let mut manifest_rolled_back = !manifest_changed
            || manifest
                .zip(original_manifest)
                .is_some_and(|(manifest, bytes)| {
                    manifest_path(manifests_dir, &manifest.repository_id)
                        .and_then(|path| write_atomic(&path, bytes))
                        .is_ok()
                });
        for related in record.related_manifests[..related_changed].iter().rev() {
            manifest_rolled_back &= manifest_path(manifests_dir, &related.package_id)
                .and_then(|path| write_atomic(&path, related.original.as_bytes()))
                .is_ok();
        }
        if files_rolled_back && manifest_rolled_back {
            let _ = remove_entry_directory(&directory);
            return Err(error);
        }
        record.state = RecordState::Preparing;
        let _ = save_record(&directory, &record);
        log::error!("MOD 回收回滚未完成，保留全部暂存信息：ID={}", record.id);
        return Err("操作失败，部分文件保留在回收站，请查看日志".into());
    }
    for file in &record.files {
        if let Some(parent) = game_root.join(&file.relative).parent() {
            if let Err(error) = archive::prune_empty_parent_dirs(game_root, parent) {
                log::warn!("回收后空目录清理失败：{}", error);
            }
        }
    }
    log::info!("MOD 移入回收站完成：ID={}", record.id);
    Ok(record.display())
}

fn set_active_manifest(
    directory: &Path,
    id: &str,
    manifest: Option<&InstallManifest>,
) -> Result<(), String> {
    let path = manifest_path(directory, id)?;
    if let Some(manifest) = manifest {
        let text = toml::to_string_pretty(manifest).map_err(io_error)?;
        fs::create_dir_all(directory).map_err(io_error)?;
        write_atomic(&path, text.as_bytes())
    } else if path.try_exists().map_err(io_error)? {
        fs::remove_file(path).map_err(io_error)
    } else {
        Ok(())
    }
}

fn rollback_moves(from: &Path, to: &Path, moved: &[StagedFile]) -> bool {
    let mut complete = true;
    for file in moved.iter().rev() {
        let result = checked_path(from, &file.relative).and_then(|source| {
            checked_path(to, &file.relative).and_then(|target| move_file(&source, &target, file))
        });
        if let Err(error) = result {
            log::error!(
                "回收站文件回滚失败，保留副本：文件={}，原因={}",
                file.relative,
                error
            );
            complete = false;
        }
    }
    complete
}

fn original_manifest(
    directory: &Path,
    record: &RecycleRecord,
) -> Result<Option<InstallManifest>, String> {
    let Some(id) = record.package_id.as_deref() else {
        if !record.removed_manifest_files.is_empty() || record.release_tag.is_some() {
            return Err(INVALID_RECORD.into());
        }
        for file in &record.files {
            let relative = mods::canonical_dll_name(&file.relative);
            let parts = relative.split('/').collect::<Vec<_>>();
            if parts.len() < 3
                || !parts[0].eq_ignore_ascii_case("BepInEx")
                || !parts[1].eq_ignore_ascii_case("plugins")
                || !Path::new(&relative)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            {
                return Err(INVALID_RECORD.into());
            }
        }
        return Ok(None);
    };
    let bytes = read_bounded(&checked_path(directory, "manifest.toml")?)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| INVALID_RECORD.to_string())?;
    let manifest: InstallManifest = toml::from_str(text).map_err(|_| INVALID_RECORD.to_string())?;
    if manifest.repository_id != id
        || record.release_tag.as_deref() != Some(&manifest.release_tag)
        || record.removed_manifest_files.iter().any(|removed| {
            !manifest
                .files
                .iter()
                .any(|file| file_key(file) == file_key(removed))
        })
    {
        return Err(INVALID_RECORD.into());
    }
    for relative in &manifest.files {
        normalize_relative(relative)?;
    }
    if record.files.iter().any(|file| {
        !record
            .removed_manifest_files
            .iter()
            .any(|removed| file_key(removed) == file_key(&file.relative))
    }) {
        return Err(INVALID_RECORD.into());
    }
    Ok(Some(manifest))
}

pub fn original_game_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    let directory = record_directory(root, id)?;
    let record = load_record(&directory, id)?;
    Ok(PathBuf::from(record.game_path))
}

fn validate_payload(payload: &Path, record: &RecycleRecord) -> Result<(), String> {
    validate_absolute_path(payload)?;
    let expected = record
        .files
        .iter()
        .map(|file| file.relative.to_lowercase())
        .collect::<HashSet<_>>();
    for entry in walkdir::WalkDir::new(payload).follow_links(false) {
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
            if !expected.contains(&relative.to_lowercase()) {
                // Never silently discard a file omitted from edited/corrupt metadata.
                return Err(INCOMPLETE.into());
            }
        }
    }
    Ok(())
}

fn merge_restored_manifest(
    original: &InstallManifest,
    active: Option<&InstallManifest>,
    removed: &[String],
) -> Result<InstallManifest, String> {
    let mut merged = if let Some(active) = active {
        if active.release_tag != original.release_tag
            || active.installed_at != original.installed_at
        {
            return Err("当前 MOD 包已更新，无法合并原安装清单".into());
        }
        active.clone()
    } else {
        let mut merged = original.clone();
        merged.files.clear();
        merged
    };
    for relative in removed {
        if !merged
            .files
            .iter()
            .any(|file| file_key(file) == file_key(relative))
        {
            let original_position = original
                .files
                .iter()
                .position(|file| file_key(file) == file_key(relative))
                .ok_or(INVALID_RECORD)?;
            let insertion = original.files[original_position + 1..]
                .iter()
                .find_map(|next| {
                    merged
                        .files
                        .iter()
                        .position(|file| file_key(file) == file_key(next))
                })
                .unwrap_or(merged.files.len());
            merged.files.insert(insertion, relative.clone());
        }
    }
    Ok(merged)
}

pub fn restore(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    id: &str,
) -> Result<(), String> {
    let directory = record_directory(root, id)?;
    let mut record = load_record(&directory, id)?;
    if record.state == RecordState::Restored {
        return Err("回收站记录不存在".into());
    }
    validate_absolute_path(game_root)?;
    let recorded_game = PathBuf::from(&record.game_path);
    validate_absolute_path(&recorded_game)?;
    if fs::canonicalize(game_root).map_err(io_error)?
        != fs::canonicalize(&recorded_game).map_err(io_error)?
    {
        return Err("请切回原游戏目录后再恢复".into());
    }
    let payload = checked_path(&directory, "files")?;
    validate_payload(&payload, &record)?;
    let original = original_manifest(&directory, &record)?;
    let mut originals = Vec::new();
    if let Some(original) = original {
        originals.push((original, record.removed_manifest_files.clone()));
    }
    for related in &record.related_manifests {
        originals.push((related.manifest()?, related.removed_files.clone()));
    }
    let mut restore_plans = Vec::new();
    for (original, removed) in originals {
        let previous = read_active_manifest(manifests_dir, &original.repository_id)?;
        let restored = merge_restored_manifest(
            &original,
            previous.as_ref().map(|(active, _)| active),
            &removed,
        )?;
        restore_plans.push((restored, previous.map(|(_, bytes)| bytes)));
    }
    let mut pending = Vec::new();
    for file in &record.files {
        let staged = checked_path(&payload, &file.relative)?;
        let target = checked_path(game_root, &file.relative)?;
        if !staged.try_exists().map_err(io_error)? {
            if record.state == RecordState::Preparing
                && target.try_exists().map_err(io_error)?
                && matches_fingerprint(&target, file)?
            {
                continue;
            }
            return Err(INCOMPLETE.into());
        }
        if !matches_fingerprint(&staged, file)? {
            return Err(INCOMPLETE.into());
        }
        let canonical = mods::canonical_dll_name(&file.relative);
        if !existing_variants(game_root, &canonical)?.is_empty() {
            return Err(CONFLICT.into());
        }
        pending.push(file.clone());
    }
    log::info!("恢复 MOD 开始：ID={}，文件数={}", id, pending.len());
    let mut moved = Vec::new();
    let mut manifests_changed = 0usize;
    let result = (|| {
        for file in &pending {
            move_file(
                &checked_path(&payload, &file.relative)?,
                &checked_path(game_root, &file.relative)?,
                file,
            )?;
            moved.push(file.clone());
        }
        for (manifest, _) in &restore_plans {
            set_active_manifest(manifests_dir, &manifest.repository_id, Some(manifest))?;
            manifests_changed += 1;
        }
        record.state = RecordState::Restored;
        save_record(&directory, &record)
    })();
    if let Err(error) = result {
        let files_rolled_back = rollback_moves(game_root, &payload, &moved);
        let mut manifest_rolled_back = true;
        for (manifest, previous) in restore_plans[..manifests_changed].iter().rev() {
            manifest_rolled_back &= match previous {
                Some(bytes) => manifest_path(manifests_dir, &manifest.repository_id)
                    .and_then(|path| write_atomic(&path, bytes))
                    .is_ok(),
                None => set_active_manifest(manifests_dir, &manifest.repository_id, None).is_ok(),
            };
        }
        if !files_rolled_back || !manifest_rolled_back {
            record.state = RecordState::Preparing;
            let _ = save_record(&directory, &record);
            log::error!("MOD 恢复回滚不完整，保留回收记录：ID={}", id);
            return Err("操作失败，部分文件保留在回收站，请查看日志".into());
        }
        return Err(error);
    }
    // The restored journal is hidden if cleanup fails; recovered files are never deleted.
    if let Err(error) = remove_entry_directory(&directory) {
        log::warn!("已恢复 MOD 的回收记录清理失败：ID={}，原因={}", id, error);
    }
    log::info!("MOD 恢复完成：ID={}", id);
    Ok(())
}

// Walk without following links, and validate the entire tree before deleting any file.
// Metadata paths are never used for permanent deletion: only the UUID-owned directory.
fn remove_entry_directory(directory: &Path) -> Result<(), String> {
    validate_absolute_path(directory)?;
    let mut children = Vec::new();
    for entry in walkdir::WalkDir::new(directory).follow_links(false) {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if is_link(&metadata) {
            return Err(UNSAFE_PATH.into());
        }
        children.push((entry.path().to_path_buf(), metadata.is_dir()));
    }
    children.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    // Journal and snapshot stay until all staged payload files were removed successfully.
    children.sort_by_key(|(path, _)| {
        path == &directory.join("record.json")
            || path == &directory.join("manifest.toml")
            || path == directory
    });
    for (path, is_directory) in children {
        if is_directory {
            fs::remove_dir(path).map_err(io_error)?;
        } else {
            archive::clear_readonly(&path).map_err(io_error)?;
            fs::remove_file(path).map_err(io_error)?;
        }
    }
    Ok(())
}

pub fn delete(root: &Path, id: &str) -> Result<(), String> {
    let directory = record_directory(root, id)?;
    // Validate the requested ID/record, never trust recorded original game paths.
    let record = load_record(&directory, id)?;
    log::info!(
        "彻底删除回收站 MOD 开始：ID={}，文件数={}",
        id,
        record.files.len()
    );
    remove_entry_directory(&directory)?;
    log::info!("彻底删除回收站 MOD 完成：ID={}", id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Workspace {
        directory: PathBuf,
        game: PathBuf,
        manifests: PathBuf,
        recycle: PathBuf,
    }

    impl Workspace {
        fn new() -> Self {
            let directory =
                std::env::temp_dir().join(format!("oc2-recycle-test-{}", Uuid::new_v4()));
            let game = directory.join("game");
            let manifests = directory.join("manager/manifests");
            let recycle = directory.join("manager/recycle-bin");
            fs::create_dir_all(&game).unwrap();
            fs::create_dir_all(&manifests).unwrap();
            fs::write(game.join("Overcooked2.exe"), b"test fixture only").unwrap();
            Self {
                directory,
                game,
                manifests,
                recycle,
            }
        }

        fn write(&self, relative: &str, content: &[u8]) {
            let path = self.game.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        fn manifest(&self, id: &str, files: &[&str]) -> InstallManifest {
            let manifest = InstallManifest {
                repository_id: id.into(),
                release_tag: "v1.2.3".into(),
                package_name: Some("Fixture package".into()),
                note: Some("original note".into()),
                installed_at: "2026-10-07T00:00:00Z".into(),
                files: files.iter().map(|file| (*file).into()).collect(),
            };
            archive::write_manifest(&self.manifests, &manifest).unwrap();
            manifest
        }

        fn active(&self, id: &str) -> Option<InstallManifest> {
            read_active_manifest(&self.manifests, id)
                .unwrap()
                .map(|(manifest, _)| manifest)
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            // Only the fresh UUID-owned test directory can be removed.
            let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
            let target = fs::canonicalize(&self.directory).unwrap();
            assert!(target.starts_with(&temp) && target != temp);
            assert!(
                self.directory
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("oc2-recycle-test-")
            );
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    const A: &str = "BepInEx/plugins/Bundle/A.dll";
    const B: &str = "BepInEx/plugins/Bundle/B.dll";
    const DATA: &str = "BepInEx/plugins/Bundle/data/settings.json";

    #[test]
    fn hiding_unidentified_plugins_keeps_all_package_files_recoverable() {
        for hide_unidentified in [true, false] {
            let workspace = Workspace::new();
            let mut plugin = b"BepInPlugin".to_vec();
            plugin.extend_from_slice(&[1, 0]);
            for value in ["com.example.recognized", "Recognized MOD", "1.0.0"] {
                plugin.push(value.len() as u8);
                plugin.extend_from_slice(value.as_bytes());
            }
            workspace.write(A, &plugin);
            workspace.write(
                &format!("{B}.disabled"),
                b"dependency without a BepInEx GUID",
            );
            workspace.write(DATA, b"settings");
            workspace.manifest("package", &[A, B, DATA]);
            let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
            let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "ui_zoom": 1.5,
                "onboarding_seen_steps": [],
                "hide_unidentified_mods": hide_unidentified,
                "game": {"path": "", "auto_detect": false},
                "network": {
                    "timeout_seconds": 5,
                    "download_threads": 1,
                    "max_retry_count": 3,
                    "check_updates_on_start": false
                },
                "launch": {
                    "ask_user_to_start_from_steam": false,
                    "auto_restore_winhttp": false
                },
                "auth": {
                    "github_token": "",
                    "gitee_token": "",
                    "official_api_only": true
                },
                "repositories": []
            }))
            .unwrap();
            let installed = mods::scan_mods_with_manifests(
                &workspace.game,
                &config,
                Some(&workspace.manifests),
            );
            assert_eq!(installed.len(), 2);
            assert_eq!(
                installed.iter().filter(|item| item.guid.is_some()).count(),
                1
            );
            assert!(
                installed
                    .iter()
                    .all(|item| item.package_id.as_deref() == Some("package")
                        && item.package_mod_count == 2)
            );

            let entry = recycle_package(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                None,
            )
            .unwrap();
            assert_eq!(entry.file_count, 3);
            assert_eq!(
                entry.files,
                vec![A.to_owned(), format!("{B}.disabled"), DATA.to_owned()]
            );
            assert!(workspace.active("package").is_none());
            for file in &entry.files {
                assert!(!workspace.game.join(file).exists());
            }
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id,
            )
            .unwrap();
            assert_eq!(fs::read(workspace.game.join(A)).unwrap(), plugin);
            assert_eq!(
                fs::read(workspace.game.join(format!("{B}.disabled"))).unwrap(),
                b"dependency without a BepInEx GUID"
            );
            assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"settings");
            assert_eq!(
                fs::read(workspace.manifests.join("package.toml")).unwrap(),
                original
            );
        }
    }

    #[test]
    fn empty_missing_or_shared_only_packages_do_not_create_empty_recycle_records() {
        for condition in ["empty", "missing", "shared"] {
            let workspace = Workspace::new();
            workspace.manifest("package", if condition == "empty" { &[] } else { &[A] });
            if condition == "shared" {
                workspace.write(A, b"shared DLL");
                workspace.manifest("peer", &[A]);
            }
            let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
            let error = recycle_package(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                None,
            )
            .unwrap_err();
            assert_eq!(error, "没有可移入回收站的文件，安装清单保持不变");
            assert!(list(&workspace.recycle).unwrap().is_empty());
            assert_eq!(
                fs::read(workspace.manifests.join("package.toml")).unwrap(),
                original
            );
            if condition == "shared" {
                assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"shared DLL");
                assert!(workspace.active("peer").is_some());
            }
        }
    }

    #[test]
    fn whole_package_preserves_disabled_paths_nondefault_target_and_raw_manifest() {
        let workspace = Workspace::new();
        workspace.write(A, b"first DLL");
        workspace.write(&format!("{B}.disabled"), b"disabled DLL");
        workspace.write(DATA, b"settings");
        workspace.write("CustomInstallTarget/data.txt", b"non-default target");
        let original = workspace.manifest("package", &[A, B, DATA, "CustomInstallTarget/data.txt"]);
        let bytes = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let entry = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap();
        assert_eq!(entry.file_count, 4);
        assert_eq!(entry.name, "Fixture package");
        assert!(entry.files.contains(&format!("{B}.disabled")));
        assert!(!workspace.game.join(A).exists());
        assert!(workspace.active("package").is_none());
        assert_eq!(
            fs::read(workspace.recycle.join(&entry.id).join("manifest.toml")).unwrap(),
            bytes
        );
        assert_eq!(list(&workspace.recycle).unwrap().len(), 1);
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"first DLL");
        assert!(!workspace.game.join(B).exists());
        assert_eq!(
            fs::read(workspace.game.join(format!("{B}.disabled"))).unwrap(),
            b"disabled DLL"
        );
        assert_eq!(
            fs::read(workspace.game.join("CustomInstallTarget/data.txt")).unwrap(),
            b"non-default target"
        );
        assert_eq!(workspace.active("package").unwrap().files, original.files);
        assert!(list(&workspace.recycle).unwrap().is_empty());
    }

    #[test]
    fn partial_restore_merges_only_its_own_files_and_retains_current_metadata() {
        let workspace = Workspace::new();
        workspace.write(A, b"first");
        workspace.write(B, b"second");
        workspace.write(DATA, b"settings");
        workspace.manifest("package", &[A, B, DATA]);
        let first = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            Some(A),
        )
        .unwrap();
        assert_eq!(workspace.active("package").unwrap().files, vec![B, DATA]);
        let second = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            Some(B),
        )
        .unwrap();
        let mut active = workspace.active("package").unwrap();
        active.note = Some("edited after removal".into());
        archive::write_manifest(&workspace.manifests, &active).unwrap();
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &first.id,
        )
        .unwrap();
        let merged = workspace.active("package").unwrap();
        assert_eq!(merged.files, vec![A, DATA]);
        assert_eq!(merged.note.as_deref(), Some("edited after removal"));
        assert!(!workspace.game.join(B).exists());
        assert!(workspace.recycle.join(&second.id).exists());
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &second.id,
        )
        .unwrap();
        assert_eq!(workspace.active("package").unwrap().files, vec![A, B, DATA]);
    }

    #[test]
    fn selected_mod_recycles_duplicate_manifest_references_and_restores_them() {
        for disabled in [false, true] {
            for whole_package in [false, true] {
                let workspace = Workspace::new();
                let actual = if disabled {
                    format!("{A}.disabled")
                } else {
                    A.into()
                };
                workspace.write(&actual, b"selected plugin");
                workspace.write(DATA, b"shared data");
                workspace.write(B, b"unrelated plugin");
                workspace.manifest("package", &[A, DATA]);
                workspace.manifest("manual", &[A]);
                workspace.manifest("peer", &[A, B, DATA]);
                let originals = ["package", "manual", "peer"].map(|id| {
                    (
                        id,
                        fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                    )
                });
                let entry = recycle_selected_mod(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    "package",
                    A,
                    whole_package,
                )
                .unwrap();
                assert_eq!(entry.files, vec![actual.clone()]);
                assert!(!workspace.game.join(&actual).exists());
                assert!(workspace.active("manual").is_none());
                assert_eq!(workspace.active("peer").unwrap().files, vec![B, DATA]);
                if whole_package {
                    assert!(workspace.active("package").is_none());
                } else {
                    assert_eq!(workspace.active("package").unwrap().files, vec![DATA]);
                }
                assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"shared data");
                assert_eq!(
                    fs::read(workspace.game.join(B)).unwrap(),
                    b"unrelated plugin"
                );
                restore(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    &entry.id,
                )
                .unwrap();
                assert_eq!(
                    fs::read(workspace.game.join(&actual)).unwrap(),
                    b"selected plugin"
                );
                for (id, original) in originals {
                    assert_eq!(
                        fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                        original
                    );
                }
            }
        }
    }

    #[test]
    fn selected_mod_rejects_missing_or_unowned_dll_without_changing_manifests() {
        for selected in [A, B] {
            let workspace = Workspace::new();
            workspace.write(DATA, b"keep data");
            workspace.write(B, b"unowned DLL");
            workspace.manifest("package", &[A, DATA]);
            workspace.manifest("peer", &[A]);
            let result = recycle_selected_mod(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                selected,
                true,
            );
            assert_eq!(
                result.unwrap_err(),
                if selected == A {
                    "找不到对应的 MOD DLL 文件"
                } else {
                    "当前 DLL 不属于这个 MOD 包"
                }
            );
            assert_eq!(workspace.active("package").unwrap().files, vec![A, DATA]);
            assert_eq!(workspace.active("peer").unwrap().files, vec![A]);
            assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"keep data");
            assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"unowned DLL");
            assert!(list(&workspace.recycle).unwrap().is_empty());
        }
    }

    #[test]
    fn unmanaged_disabled_dll_can_restore_or_be_permanently_deleted() {
        let workspace = Workspace::new();
        workspace.write(&format!("{A}.disabled"), b"unmanaged");
        let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
        assert_eq!(entry.files, vec![format!("{A}.disabled")]);
        assert!(entry.package_id.is_none());
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        assert_eq!(
            fs::read(workspace.game.join(format!("{A}.disabled"))).unwrap(),
            b"unmanaged"
        );
        let removed = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
        delete(&workspace.recycle, &removed.id).unwrap();
        assert!(!workspace.recycle.join(removed.id).exists());
        assert!(!workspace.game.join(format!("{A}.disabled")).exists());
        assert!(list(&workspace.recycle).unwrap().is_empty());
    }

    fn duplicate_plugin(file: &str, version: &str) -> crate::models::InstalledMod {
        serde_json::from_value(serde_json::json!({
            "name": "ConfigurationManager fixture", "dll_file": file,
            "guid": "fixture.configurationmanager", "enabled": true,
            "version": version, "version_source": "BepInPlugin",
            "update_available": false, "duplicate_guid": true,
            "duplicate_version": true, "status": "installed"
        }))
        .unwrap()
    }

    #[test]
    fn duplicate_recycle_updates_all_owners_and_restores_their_manifests() {
        let workspace = Workspace::new();
        workspace.write(A, b"redundant plugin");
        workspace.write(B, b"effective plugin");
        workspace.write(DATA, b"keep unrelated data");
        let primary = workspace.manifest("package", &[A, DATA]);
        let peer = workspace.manifest("peer", &[A, B]);
        let historical = workspace.manifest("historical", &[A]);
        let installed = [duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "2.0.0")];
        let entry = recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &installed,
        )
        .unwrap();
        assert_eq!(entry.files, vec![A]);
        assert!(!workspace.game.join(A).exists());
        assert_eq!(
            fs::read(workspace.game.join(B)).unwrap(),
            b"effective plugin"
        );
        assert_eq!(
            fs::read(workspace.game.join(DATA)).unwrap(),
            b"keep unrelated data"
        );
        assert_eq!(workspace.active("package").unwrap().files, vec![DATA]);
        assert_eq!(workspace.active("peer").unwrap().files, vec![B]);
        assert!(workspace.active("historical").is_none());
        let record = load_record(&workspace.recycle.join(&entry.id), &entry.id).unwrap();
        assert_eq!(record.related_manifests.len(), 2);
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        assert_eq!(
            fs::read(workspace.game.join(A)).unwrap(),
            b"redundant plugin"
        );
        for original in [primary, peer, historical] {
            let restored = workspace.active(&original.repository_id).unwrap();
            assert_eq!(
                toml::to_string(&restored).unwrap(),
                toml::to_string(&original).unwrap()
            );
        }
    }

    #[test]
    fn shared_dll_requires_existing_enabled_equal_or_newer_guid_copy() {
        for condition in [
            "different_guid",
            "empty_guid",
            "disabled",
            "older",
            "unknown_version",
            "warning",
            "same_path",
            "missing_file",
            "missing_target",
        ] {
            let workspace = Workspace::new();
            workspace.write(A, b"shared plugin");
            if condition != "missing_file" {
                workspace.write(B, b"other plugin");
            }
            workspace.manifest("package", &[A]);
            workspace.manifest("peer", &[A, DATA]);
            let mut target = duplicate_plugin(A, "2.0.0");
            let mut peer = duplicate_plugin(B, "2.0.0");
            match condition {
                "different_guid" => peer.guid = Some("another.plugin".into()),
                "empty_guid" => target.guid = None,
                "disabled" => peer.enabled = false,
                "older" => peer.version = Some("1.0.0".into()),
                "unknown_version" => peer.version = None,
                "warning" => peer.load_warning = Some("load warning".into()),
                "same_path" => peer.dll_file = format!("{A}.disabled"),
                _ => {}
            }
            let installed = if condition == "missing_target" {
                vec![peer]
            } else {
                vec![target, peer]
            };
            let result = recycle_mod_dll(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                A,
                &installed,
            );
            assert_eq!(
                result.unwrap_err(),
                "该文件仍由其他 MOD 包使用，不能移入回收站",
                "{condition}"
            );
            assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"shared plugin");
            assert_eq!(workspace.active("package").unwrap().files, vec![A]);
            assert_eq!(workspace.active("peer").unwrap().files, vec![A, DATA]);
            assert!(list(&workspace.recycle).unwrap().is_empty());
        }
    }

    #[test]
    fn duplicate_replacement_must_cover_all_selected_path_variants() {
        let workspace = Workspace::new();
        workspace.write(A, b"enabled version 3");
        workspace.write(&format!("{A}.disabled"), b"disabled version 1");
        workspace.write(B, b"other version 2");
        workspace.manifest("package", &[A]);
        workspace.manifest("peer", &[A, B]);
        let mut older = duplicate_plugin(A, "1.0.0");
        older.enabled = false;
        let installed = [
            older,
            duplicate_plugin(A, "3.0.0"),
            duplicate_plugin(B, "2.0.0"),
        ];
        assert_eq!(
            recycle_mod_dll(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "package",
                A,
                &installed
            )
            .unwrap_err(),
            "该文件仍由其他 MOD 包使用，不能移入回收站"
        );
        assert_eq!(
            fs::read(workspace.game.join(A)).unwrap(),
            b"enabled version 3"
        );
        assert_eq!(
            fs::read(workspace.game.join(format!("{A}.disabled"))).unwrap(),
            b"disabled version 1"
        );
    }

    #[test]
    fn disabled_duplicate_preserves_variant_and_canonical_peer_reference() {
        let workspace = Workspace::new();
        let disabled = format!("{A}.disabled");
        workspace.write(&disabled, b"disabled duplicate");
        workspace.write(B, b"effective plugin");
        workspace.manifest("package", &[A]);
        let peer_path = "BepInEx\\plugins\\Bundle\\A.DLL.disabled";
        workspace.manifest("peer", &[peer_path, B]);
        let mut target = duplicate_plugin(A, "1.0.0");
        target.enabled = false;
        let entry = recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &[target, duplicate_plugin(B, "1.0.0")],
        )
        .unwrap();
        assert_eq!(entry.files, vec![disabled.clone()]);
        assert_eq!(workspace.active("peer").unwrap().files, vec![B]);
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        assert!(!workspace.game.join(A).exists());
        assert_eq!(
            fs::read(workspace.game.join(disabled)).unwrap(),
            b"disabled duplicate"
        );
        assert_eq!(workspace.active("peer").unwrap().files, vec![peer_path, B]);
    }

    #[test]
    fn duplicate_restore_refuses_updated_peer_before_moving_payload() {
        let workspace = Workspace::new();
        workspace.write(A, b"duplicate");
        workspace.write(B, b"effective");
        workspace.manifest("package", &[A]);
        workspace.manifest("peer", &[A, B]);
        let entry = recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &[duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "1.0.0")],
        )
        .unwrap();
        let mut updated = workspace.active("peer").unwrap();
        updated.release_tag = "new release".into();
        archive::write_manifest(&workspace.manifests, &updated).unwrap();
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            "当前 MOD 包已更新，无法合并原安装清单"
        );
        assert!(!workspace.game.join(A).exists());
        assert!(workspace.active("package").is_none());
        assert_eq!(workspace.active("peer").unwrap().release_tag, "new release");
        assert_eq!(
            fs::read(workspace.recycle.join(&entry.id).join("files").join(A)).unwrap(),
            b"duplicate"
        );
    }

    #[test]
    fn duplicate_restore_preserves_later_independent_peer_removal() {
        let workspace = Workspace::new();
        workspace.write(A, b"duplicate");
        workspace.write(B, b"effective");
        workspace.write(DATA, b"data");
        workspace.manifest("package", &[A]);
        workspace.manifest("peer", &[A, B, DATA]);
        let first = recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &[duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "1.0.0")],
        )
        .unwrap();
        let second = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "peer",
            Some(B),
        )
        .unwrap();
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &first.id,
        )
        .unwrap();
        assert_eq!(workspace.active("peer").unwrap().files, vec![A, DATA]);
        assert!(!workspace.game.join(B).exists());
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &second.id,
        )
        .unwrap();
        assert_eq!(workspace.active("peer").unwrap().files, vec![A, B, DATA]);
    }

    #[test]
    fn duplicate_journal_commit_failure_rolls_back_every_manifest() {
        let workspace = Workspace::new();
        workspace.write(A, b"duplicate");
        let primary = workspace.manifest("package", &[A, DATA]);
        workspace.manifest("peer", &[A, B]);
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let peer_original = fs::read(workspace.manifests.join("peer.toml")).unwrap();
        let related = RelatedManifest {
            package_id: "peer".into(),
            original: String::from_utf8(peer_original.clone()).unwrap(),
            removed_files: vec![A.into()],
        };
        let mut remaining = primary.clone();
        remaining.files.retain(|file| file != A);
        let result = stage_with_related_manifests(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package".into(),
            Some(&primary),
            Some(&original),
            vec![A.into()],
            vec![A.into()],
            Some(&remaining),
            vec![related],
            &mut || {
                let entry = fs::read_dir(&workspace.recycle)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::remove_file(entry.join("record.json")).unwrap();
                fs::create_dir(entry.join("record.json")).unwrap();
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err(), IO_ERROR);
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"duplicate");
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        assert_eq!(
            fs::read(workspace.manifests.join("peer.toml")).unwrap(),
            peer_original
        );
    }

    #[test]
    fn duplicate_restore_rejects_unrelated_manifest_snapshot_entries() {
        let workspace = Workspace::new();
        workspace.write(A, b"duplicate");
        workspace.write(B, b"effective");
        workspace.manifest("package", &[A]);
        workspace.manifest("peer", &[A, B]);
        let entry = recycle_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &[duplicate_plugin(A, "1.0.0"), duplicate_plugin(B, "1.0.0")],
        )
        .unwrap();
        let directory = workspace.recycle.join(&entry.id);
        let mut record = load_record(&directory, &entry.id).unwrap();
        record.related_manifests[0].removed_files.push(B.into());
        save_record(&directory, &record).unwrap();
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            INVALID_RECORD
        );
        assert!(!workspace.game.join(A).exists());
        assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"effective");
    }

    #[test]
    fn shared_files_stay_with_other_active_package() {
        let workspace = Workspace::new();
        workspace.write(A, b"owned");
        workspace.write(DATA, b"shared");
        workspace.manifest("package", &[A, DATA]);
        workspace.manifest("peer", &[DATA]);
        let entry = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap();
        assert_eq!(entry.files, vec![A]);
        assert_eq!(fs::read(workspace.game.join(DATA)).unwrap(), b"shared");
        assert!(workspace.active("peer").is_some());
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        assert_eq!(workspace.active("package").unwrap().files, vec![A, DATA]);
    }

    #[test]
    fn restoring_disabled_dll_refuses_enabled_variant_and_keeps_both_copies() {
        let workspace = Workspace::new();
        workspace.write(&format!("{A}.disabled"), b"original");
        let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
        workspace.write(A, b"new install");
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            CONFLICT
        );
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"new install");
        assert_eq!(
            fs::read(
                workspace
                    .recycle
                    .join(&entry.id)
                    .join("files")
                    .join(format!("{A}.disabled"))
            )
            .unwrap(),
            b"original"
        );
        assert_eq!(list(&workspace.recycle).unwrap().len(), 1);
    }

    #[test]
    fn restore_refuses_changed_game_directory_and_reinstalled_package() {
        let workspace = Workspace::new();
        workspace.write(A, b"original");
        workspace.manifest("package", &[A]);
        let entry = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap();
        let other_game = workspace.directory.join("other-game");
        fs::create_dir_all(&other_game).unwrap();
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &other_game,
                &entry.id
            )
            .unwrap_err(),
            "请切回原游戏目录后再恢复"
        );
        let mut updated = workspace.manifest("package", &[B]);
        updated.release_tag = "v2".into();
        archive::write_manifest(&workspace.manifests, &updated).unwrap();
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            "当前 MOD 包已更新，无法合并原安装清单"
        );
        assert_eq!(workspace.active("package").unwrap().release_tag, "v2");
        assert!(workspace.recycle.join(entry.id).exists());
    }

    #[test]
    fn mid_move_failure_rolls_back_files_and_leaves_original_manifest_untouched() {
        let workspace = Workspace::new();
        workspace.write(A, b"first");
        workspace.write(B, b"second");
        let manifest = workspace.manifest("package", &[A, B]);
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let error = stage(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package".into(),
            Some(&manifest),
            Some(&original),
            manifest.files.clone(),
            manifest.files.clone(),
            None,
            &mut || Err("injected failure".into()),
        )
        .unwrap_err();
        assert_eq!(error, "injected failure");
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"first");
        assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"second");
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        assert!(list(&workspace.recycle).unwrap().is_empty());
    }

    #[test]
    fn journal_commit_failure_restores_files_and_the_removed_active_manifest() {
        let workspace = Workspace::new();
        workspace.write(A, b"first");
        workspace.write(B, b"second");
        let manifest = workspace.manifest("package", &[A, B]);
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let mut count = 0;
        let result = stage(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package".into(),
            Some(&manifest),
            Some(&original),
            manifest.files.clone(),
            manifest.files.clone(),
            None,
            &mut || {
                count += 1;
                if count == 2 {
                    let entry = fs::read_dir(&workspace.recycle)
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    fs::remove_file(entry.join("record.json")).unwrap();
                    fs::create_dir(entry.join("record.json")).unwrap();
                }
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err(), IO_ERROR);
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"first");
        assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"second");
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
    }

    #[test]
    fn rollback_conflict_preserves_staged_copy_and_recoverable_record() {
        let workspace = Workspace::new();
        workspace.write(A, b"original");
        let manifest = workspace.manifest("package", &[A]);
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let result = stage(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package".into(),
            Some(&manifest),
            Some(&original),
            manifest.files.clone(),
            manifest.files.clone(),
            None,
            &mut || {
                workspace.write(A, b"conflicting new file");
                Err("injected failure".into())
            },
        );
        assert_eq!(
            result.unwrap_err(),
            "操作失败，部分文件保留在回收站，请查看日志"
        );
        let entries = list(&workspace.recycle).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            fs::read(workspace.recycle.join(&entries[0].id).join("files").join(A)).unwrap(),
            b"original"
        );
        assert_eq!(
            fs::read(workspace.game.join(A)).unwrap(),
            b"conflicting new file"
        );
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        fs::remove_file(workspace.game.join(A)).unwrap();
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entries[0].id,
        )
        .unwrap();
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"original");
    }

    #[test]
    fn verified_copy_fallback_removes_source_only_after_integrity_check() {
        let workspace = Workspace::new();
        let source = workspace.directory.join("source.dll");
        let target = workspace.directory.join("target.dll");
        fs::write(&source, b"cross-volume-copy fixture").unwrap();
        let (size, sha256) = fingerprint(&source).unwrap();
        let file = StagedFile {
            relative: "fixture.dll".into(),
            size,
            sha256,
        };
        copy_verified_and_remove(&source, &target, &file).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"cross-volume-copy fixture");
        fs::write(&source, b"changed fixture").unwrap();
        let another = workspace.directory.join("failed-copy.dll");
        assert_eq!(
            copy_verified_and_remove(&source, &another, &file).unwrap_err(),
            INCOMPLETE
        );
        assert_eq!(fs::read(&source).unwrap(), b"changed fixture");
        assert!(!another.exists());
        assert!(copy_verified_and_remove(&source, &target, &file).is_err());
        assert_eq!(fs::read(target).unwrap(), b"cross-volume-copy fixture");
    }

    #[test]
    fn forged_ids_paths_and_corrupted_payload_cannot_escape_or_destroy_data() {
        let workspace = Workspace::new();
        workspace.write(A, b"original");
        assert!(recycle_unmanaged(&workspace.recycle, &workspace.game, "../outside.dll").is_err());
        assert!(
            recycle_unmanaged(
                &workspace.recycle,
                &workspace.game,
                "BepInEx/plugins/a.dll:stream"
            )
            .is_err()
        );
        assert!(delete(&workspace.recycle, "../game").is_err());
        assert!(
            recycle_package(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                "../../outside",
                None
            )
            .is_err()
        );
        let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
        let directory = workspace.recycle.join(&entry.id);
        let original_record = load_record(&directory, &entry.id).unwrap();
        let mut forged = original_record.clone();
        forged.files[0].relative = "../../outside.dll".into();
        save_record(&directory, &forged).unwrap();
        assert!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .is_err()
        );
        assert!(delete(&workspace.recycle, &entry.id).is_err());
        assert_eq!(
            fs::read(directory.join("files").join(A)).unwrap(),
            b"original"
        );
        save_record(&directory, &original_record).unwrap();
        fs::write(directory.join("files").join(A), b"changed staged file").unwrap();
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            INCOMPLETE
        );
        assert!(directory.exists());
        assert!(!workspace.game.join(A).exists());
    }

    #[test]
    fn undeclared_staged_files_are_preserved_when_restore_metadata_is_incomplete() {
        let workspace = Workspace::new();
        workspace.write(A, b"original");
        let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
        let directory = workspace.recycle.join(&entry.id);
        let unknown = directory.join("files/omitted-from-record.dll");
        fs::write(&unknown, b"must not disappear during restore").unwrap();
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            INCOMPLETE
        );
        assert_eq!(
            fs::read(&unknown).unwrap(),
            b"must not disappear during restore"
        );
        assert_eq!(
            fs::read(directory.join("files").join(A)).unwrap(),
            b"original"
        );
        assert!(!workspace.game.join(A).exists());
        // Explicit permanent deletion still owns the entire UUID directory.
        delete(&workspace.recycle, &entry.id).unwrap();
        assert!(!directory.exists());
    }

    #[test]
    fn manager_inside_plugins_is_rejected_before_any_file_moves() {
        let workspace = Workspace::new();
        workspace.write(A, b"original");
        let recycle = workspace.game.join("BepInEx/plugins/manager/recycle-bin");
        assert_eq!(
            recycle_unmanaged(&recycle, &workspace.game, A).unwrap_err(),
            "回收站不能位于游戏的 MOD 目录内"
        );
        assert_eq!(fs::read(workspace.game.join(A)).unwrap(), b"original");
        assert!(!recycle.exists());
    }

    #[cfg(any(windows, unix))]
    fn directory_link(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            // Directory junctions do not require symlink privileges on Windows.
            let output = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(target.to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap();
            if !output.status.success() {
                eprintln!(
                    "junction creation failed: {} {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return output.status.success();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(any(windows, unix))]
    fn remove_directory_link(link: &Path) {
        assert!(is_link(&fs::symlink_metadata(link).unwrap()));
        #[cfg(windows)]
        fs::remove_dir(link).unwrap();
        #[cfg(unix)]
        fs::remove_file(link).unwrap();
    }

    #[test]
    #[cfg(any(windows, unix))]
    fn junctions_in_source_destination_or_recycle_payload_never_follow_outside_paths() {
        let workspace = Workspace::new();
        let outside = workspace.directory.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("A.dll"), b"outside original").unwrap();
        let linked_source = workspace.game.join("BepInEx/plugins/Linked");
        fs::create_dir_all(linked_source.parent().unwrap()).unwrap();
        assert!(
            directory_link(&linked_source, &outside),
            "test link creation failed"
        );
        assert_eq!(
            recycle_unmanaged(
                &workspace.recycle,
                &workspace.game,
                "BepInEx/plugins/Linked/A.dll"
            )
            .unwrap_err(),
            UNSAFE_PATH
        );
        remove_directory_link(&linked_source);
        workspace.write(A, b"staged original");
        let entry = recycle_unmanaged(&workspace.recycle, &workspace.game, A).unwrap();
        let directory = workspace.recycle.join(&entry.id);
        let bundle = directory.join("files/BepInEx/plugins/Bundle");
        let preserved = workspace.directory.join("preserved-staged.dll");
        fs::rename(bundle.join("A.dll"), &preserved).unwrap();
        fs::remove_dir(&bundle).unwrap();
        assert!(directory_link(&bundle, &outside));
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            UNSAFE_PATH
        );
        assert_eq!(
            delete(&workspace.recycle, &entry.id).unwrap_err(),
            UNSAFE_PATH
        );
        assert_eq!(
            fs::read(outside.join("A.dll")).unwrap(),
            b"outside original"
        );
        remove_directory_link(&bundle);
        fs::create_dir(&bundle).unwrap();
        fs::rename(&preserved, bundle.join("A.dll")).unwrap();
        let target_bundle = workspace.game.join("BepInEx/plugins/Bundle");
        assert!(directory_link(&target_bundle, &outside));
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &entry.id
            )
            .unwrap_err(),
            UNSAFE_PATH
        );
        assert_eq!(fs::read(bundle.join("A.dll")).unwrap(), b"staged original");
        assert_eq!(
            fs::read(outside.join("A.dll")).unwrap(),
            b"outside original"
        );
        remove_directory_link(&target_bundle);
        assert_eq!(
            fs::read(outside.join("A.dll")).unwrap(),
            b"outside original"
        );
    }
}
