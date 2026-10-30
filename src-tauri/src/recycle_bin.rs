//! Recoverable MOD removal. The active manifest and staged files are one transaction;
//! online reinstallation also stages the previous package before writing its replacement.
use crate::archive::{self, InstallManifest};
use crate::models::ModRecycleEntry;
use crate::mods;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const UNSAFE_PATH: &str = "recycle.unsafePath";
const INVALID_RECORD: &str = "recycle.invalidRecord";
const IO_ERROR: &str = "recycle.fileOperationFailed";
const CONFLICT: &str = "recycle.restoreTargetExists";
const INCOMPLETE: &str = "recycle.payloadModified";
const NO_FILES: &str = "recycle.noMovableFiles";
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
    /// 删除时仓库名的多语言快照。
    ///
    /// 为什么存快照而不是删除后去查仓库：MOD 已经从磁盘移走了，
    /// 回收站列表要能独立回答「这是哪个 MOD」，不该依赖当前配置里
    /// 是否还存在那个仓库（仓库可能已从源里下线）。
    ///
    /// 渲染时按界面语言从这里现选，所以切语言后**历史记录也会跟着变**。
    /// 旧记录没有这个字段，回退到单语 `name`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    name_i18n: BTreeMap<String, String>,
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

/// 清单里的某个文件是否属于给定仓库。
///
/// 用扫描时「本地 MOD 绑定在线仓库」的同一套字段：BepInPlugin GUID、
/// PE 产品名、原始文件名。任一非空且与仓库配置相等即算命中。
///
/// 依赖 DLL 读不出这些字段，返回 false —— 但只要同一份清单里有主 MOD
/// 命中，整包就已经被判定为该仓库的旧版本了。
fn dll_matches_repository(
    game_root: &Path,
    relative: &str,
    repo: &crate::models::RepositoryConfig,
) -> bool {
    let configured = [
        repo.plugin_guid.trim(),
        repo.local_product_name.trim(),
        repo.local_original_filename.trim(),
    ];
    if configured.iter().all(|value| value.is_empty()) {
        return false;
    }
    let normalized = match normalize_relative(relative) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let path = game_root.join(&normalized);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let plugin = mods::parse_bepin_plugin_metadata(&bytes);
    let metadata = crate::game::read_file_metadata(&path);
    let actual = [
        plugin
            .as_ref()
            .and_then(|item| item.guid.clone())
            .unwrap_or_default()
            .trim()
            .to_string(),
        metadata
            .product_name
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string(),
        metadata
            .original_filename
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string(),
    ];
    configured
        .iter()
        .zip(actual.iter())
        .any(|(expected, value)| !expected.is_empty() && value.eq_ignore_ascii_case(expected))
}

/// 两个游戏相对路径是否落在同一个**包目录**里。
///
/// 用于识别「同一个包的历史清单」：旧版本可能把内容解到子目录，
/// 文件路径与这次不同，但目录归属一致。
///
/// 必须排除 BepInEx 的系统容器目录：`BepInEx/plugins`、`BepInEx/patchers`
/// 下的散装 DLL 只是平铺放在一起，并不属于同一个包，认错会误删无关 MOD。
fn same_package_dir(left: &str, right: &str) -> bool {
    const SHARED_CONTAINERS: [&str; 3] = ["plugins", "patchers", "config"];
    let parent = |value: &str| -> Option<String> {
        let normalized = value.replace('\\', "/");
        let (dir, _) = normalized.rsplit_once('/')?;
        let dir = dir.trim_end_matches('/');
        if dir.is_empty() {
            return None;
        }
        // 目录本身是系统容器（或直接位于其中之一）时不算包目录
        let last = dir.rsplit_once('/').map_or(dir, |(_, tail)| tail);
        if SHARED_CONTAINERS.contains(&last.to_ascii_lowercase().as_str()) {
            return None;
        }
        Some(dir.to_lowercase())
    };
    match (parent(left), parent(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
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
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

/// 安装某个仓库的资产前，回收该仓库的已装包。
///
/// 「归属该仓库」按**身份**判定而不看渠道：
///   - `repository_id` 等于本次安装的包 ID（正常升级）；
///   - 或清单 binding 的 provider/owner/repository 与本次仓库一致
///     （历史安装用了别的包 ID，或仓库渠道、下载方式被改过）。
///
/// 之前用 `binding.matches_repository()` 判定，它额外要求 `channel` 相同。
/// 结果：仓库从 `all` 改成 `beta`（或反之）后，旧安装的 binding 不再匹配，
/// 旧包不被回收、文件留在磁盘上，扫描时就报「重复」。
/// 渠道只是「拉哪个 Release」的过滤条件，不改变「同一个仓库、同一批文件」的
/// 事实，所以这里必须按身份而非渠道判定。
///
/// 返回本次回收的全部记录；调用方在安装失败时据此整体回滚。
/// 任一包回收失败即中止（不继续安装），避免留下半清理状态。
pub fn recycle_repository_packages_before_install(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    repo: &crate::models::RepositoryConfig,
    replacement_files: &[String],
) -> Result<Vec<ModRecycleEntry>, String> {
    let manifests = all_active_manifests(manifests_dir)?;
    let mut targets = Vec::new();
    for manifest in &manifests {
        let same_id = manifest.repository_id == repo.id;
        let same_identity = manifest
            .repository_binding
            .as_ref()
            .is_some_and(|binding| binding.is_valid() && binding.matches_identity(repo));
        /*
         * 历史遗留或手动安装的清单：只要清单里**任一 DLL** 的身份能对上
         * 本次安装的仓库，整包都算这个仓库的旧版本。
         *
         * 判定用 GUID / 产品名 / 原始文件名 —— 与扫描时「本地 MOD 绑定在线
         * 仓库」用的是同一套字段。不用文件路径或目录来判断：
         *   · 包内依赖 DLL 读不出 GUID，靠主 MOD 命中即可整包归属；
         *   · 管理器解压 ZIP 时所有文件都写进同一份清单，所以命中一个就等于
         *     命中整包，与它们散落在哪个子目录无关；
         *   · 同目录判定会误伤 plugins 下平铺的无关插件。
         */
        let identity_match = manifest
            .files
            .iter()
            .any(|file| dll_matches_repository(game_root, file, repo));
        let overlaps_install = manifest.repository_id.starts_with("manual-")
            && manifest.files.iter().any(|file| {
                replacement_files.iter().any(|target| {
                    file_key(target) == file_key(file) || same_package_dir(target, file)
                })
            });
        if same_id || same_identity || identity_match || overlaps_install {
            targets.push(manifest.repository_id.clone());
        }
    }
    targets.sort();
    targets.dedup();

    let mut entries = Vec::new();
    // 同批要一起回收的包 ID 全量列表：它们彼此不能算「其他包」，
    // 否则 A 的清单会让 B 的文件被当成共享文件而跳过回收，装完仍留旧副本。
    let siblings = targets.clone();
    for package_id in targets {
        let entry = recycle_package_with_mods(
            root,
            manifests_dir,
            game_root,
            &package_id,
            None,
            None,
            None,
            replacement_files,
            &siblings,
        )?;
        if let Some(entry) = entry {
            log::info!(
                "安装前回收同仓库旧包：仓库={}，包={}，回收记录={}，文件数={}",
                repo.id,
                package_id,
                entry.id,
                entry.file_count
            );
            entries.push(entry);
            continue;
        }
        /*
         * 没有可回收文件：这批文件已被同批的另一个包顺带移走
         * （它们登记在同一批文件上）。此时必须把这个包的清单也移除，
         * 否则活动清单里留下一条指向已不存在文件的记录，扫描仍会多出一行。
         *
         * 只有在同批确实有其它包存在时才这样做 —— 单独回收一个已经空了的
         * 包时，保留清单是既有语义（用户删除时由调用方报错）。
         */
        if siblings.len() > 1 {
            set_active_manifest(manifests_dir, &package_id, None)?;
            log::info!(
                "旧包文件已由同批回收清空，移除其清单：仓库={}，包={}",
                repo.id,
                package_id
            );
        }
    }
    Ok(entries)
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
    sibling_packages: &[String],
) -> Result<Option<ModRecycleEntry>, String> {
    let (manifest, original) =
        read_active_manifest(manifests_dir, package_id)?.ok_or("mods.unknownFiles")?;
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
        return Err("mods.dllNotInPackage".into());
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
        return Err("mods.dllNotInPackage".into());
    }
    if let Some(dll) = explicit_dll {
        let relative = normalize_relative(&mods::canonical_dll_name(dll))?;
        if existing_variants(game_root, &relative)?.is_empty() {
            return Err("mods.dllNotFound".into());
        }
    }
    let peer_manifests = all_active_manifests(manifests_dir)?
        .into_iter()
        // 同批回收的包不算 peer：它们会和本包一起被移走，
        // 若把它们当「其他包」保护共享文件，两边都会跳过回收，旧副本留在磁盘上。
        .filter(|peer| {
            peer.repository_id != package_id && !sibling_packages.contains(&peer.repository_id)
        })
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
                    return Err("recycle.fileSharedByOtherPackage".into());
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
    // 清单只剩多语言快照，单语名取主语言值作为兜底显示名
    let name = {
        let from_i18n = archive::primary_translation(&manifest.package_name_i18n);
        if from_i18n.trim().is_empty() {
            package_id.to_owned()
        } else {
            from_i18n
        }
    };
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
        return Err("mods.notInPlugins".into());
    }
    let actual = existing_variants(game_root, &relative)?;
    if actual.is_empty() {
        return Err("mods.dllNotFound".into());
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
        return Err("recycle.insideModsDirectory".into());
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
        // 仓库名的多语言快照：回收站列表要能随界面语言渲染，而删除之后
        // 仓库可能已从源文档下线，无处可查，所以在这里固化下来。
        name_i18n: manifest
            .map(|manifest| manifest.package_name_i18n.clone())
            .unwrap_or_default(),
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
        return Err("recycle.partialFailure".into());
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
            || active.repository_binding != original.repository_binding
            || active.installed_at != original.installed_at
        {
            return Err("recycle.packageUpdated".into());
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
        return Err("recycle.recordMissing".into());
    }
    validate_absolute_path(game_root)?;
    let recorded_game = PathBuf::from(&record.game_path);
    validate_absolute_path(&recorded_game)?;
    if fs::canonicalize(game_root).map_err(io_error)?
        != fs::canonicalize(&recorded_game).map_err(io_error)?
    {
        return Err("recycle.wrongGameDirectory".into());
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
            return Err("recycle.partialFailure".into());
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

/// 清空回收站：彻底删除**全部**记录。
///
/// 逐条走 `delete` 而不是直接删整个 root 目录：
///   · 每条记录都要经过 `load_record` 校验（ID 与记录内容必须对得上），
///     直接 `remove_dir_all(root)` 会跳过这层，一个损坏的记录也能被静默抹掉；
///   · 逐条删除能准确统计「删了几条、几条失败」，失败时已删的保持删除、
///     未删的原样保留，用户看到的是真实剩余而不是「全部成功」的假象。
///
/// 返回成功删除的条数。任一记录失败立即中止并上抛错误 —— 清空是用户
/// 明确要求的破坏性操作，静默跳过失败项会留下「以为清空了其实还有」的状态。
pub fn delete_all(root: &Path) -> Result<usize, String> {
    let entries = list(root)?;
    let total = entries.len();
    if total == 0 {
        return Ok(0);
    }
    log::info!("清空回收站开始：待删除记录数={}", total);
    let mut removed = 0usize;
    for entry in entries {
        match delete(root, &entry.id) {
            Ok(()) => removed += 1,
            Err(error) => {
                log::error!(
                    "清空回收站中断：已删除={}，失败 ID={}，原因={}",
                    removed,
                    entry.id,
                    error
                );
                return Err(crate::locale_tag::with_params(
                    "recycle.emptyFailed",
                    &[
                        ("index", &(removed + 1).to_string()),
                        ("removed", &removed.to_string()),
                        ("reason", &error.to_string()),
                    ],
                ));
            }
        }
    }
    log::info!("清空回收站完成：删除记录数={}", removed);
    Ok(removed)
}

/*
 * ===== 直接删除（不经过回收站）=====
 *
 * 用户关闭「删除 MOD 时优先放入回收站」后，删除操作应直接移除文件。
 *
 * 实现方式是**先按原回收流程处理，再立即彻底删除刚生成的记录**，
 * 而不是另写一套删除逻辑。原因：
 *   · 回收流程已经处理了共享文件保护、`.disabled` 变体、清单快照、
 *     空目录清理、失败回滚 —— 这些都是踩过多轮坑才稳定的；
 *   · 复制一份「删除」实现迟早与回收逻辑分叉，出现「回收安全、直删
 *     丢共享文件」这类只在特定配置下复现的问题；
 *   · 暂存区是同一磁盘上的 rename，代价接近零，寿命只有几毫秒，
 *     用户完全感知不到。
 *
 * 唯一的语义差别是：失败时不再有回收记录可恢复，所以这里的错误直接上抛，
 * 由调用方保证「删不干净就报错」，绝不静默继续。
 */

/// 直接删除一个 MOD 包（或其中单个 DLL），不留回收记录。
pub fn purge_package(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: Option<&str>,
) -> Result<(), String> {
    let entry = recycle_package(root, manifests_dir, game_root, package_id, dll)?;
    purge_entry(root, &entry.id)
}

/// 直接删除本地选中的 DLL（或它的整包），不留回收记录。
pub fn purge_selected_mod(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: &str,
    whole_package: bool,
) -> Result<(), String> {
    let entry = recycle_selected_mod(
        root,
        manifests_dir,
        game_root,
        package_id,
        dll,
        whole_package,
    )?;
    purge_entry(root, &entry.id)
}

/// 直接删除同包中单个 DLL（保留包内其他成员），不留回收记录。
pub fn purge_mod_dll(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: &str,
    installed: &[crate::models::InstalledMod],
) -> Result<(), String> {
    let entry = recycle_mod_dll(root, manifests_dir, game_root, package_id, dll, installed)?;
    purge_entry(root, &entry.id)
}

/// 直接删除未由管理器安装的 DLL，不留回收记录。
pub fn purge_unmanaged(root: &Path, game_root: &Path, dll: &str) -> Result<(), String> {
    let entry = recycle_unmanaged(root, game_root, dll)?;
    purge_entry(root, &entry.id)
}

/*
 * 覆盖安装没有 `purge_repository_packages_before_install` 这样的包装：
 * 那条路径必须保留回滚能力，所以由调用方「先回收 → 安装成功后再 delete」，
 * 见 `install_downloaded_repository` 里 `keep_recycled` 的分支。
 */

/// 清掉回收站里的一条记录（回收流程刚生成的那条）。
fn purge_entry(root: &Path, id: &str) -> Result<(), String> {
    delete(root, id).map_err(|error| {
        log::error!(
            "直接删除时清理暂存记录失败：ID={}，原因={}。文件可能仍留在回收站，请手动检查。",
            id,
            error
        );
        error
    })?;
    log::info!("已直接删除 MOD（未保留回收记录）：ID={}", id);
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
                repository_binding: None,
                package_name_i18n: crate::archive::single_language("Fixture package"),
                note_i18n: crate::archive::single_language("original note"),
                group_i18n: Default::default(),
                prerelease: false,
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

    /// 构造一个只含 BepInPlugin 头的假 DLL，够 `parse_bepin_plugin_metadata` 解析。
    fn plugin_blob(guid: &str, name: &str, version: &str) -> Vec<u8> {
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in [guid, name, version] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes
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
    fn repository_binding_survives_package_and_child_restore_and_rejects_changed_owners() {
        for whole_package in [true, false] {
            for disabled in [false, true] {
                let workspace = Workspace::new();
                let actual = if disabled {
                    format!("{A}.disabled")
                } else {
                    A.into()
                };
                workspace.write(&actual, b"bound plugin");
                workspace.write(B, b"sibling plugin");
                let mut original = workspace.manifest("package", &[A, B]);
                original.repository_binding = Some(crate::models::RepositoryBinding {
                    provider: crate::models::Provider::Github,
                    owner: "Owner".into(),
                    repository: "Mod".into(),
                    tag: original.release_tag.clone(),
                    channel: crate::models::Channel::Stable,
                });
                archive::write_manifest(&workspace.manifests, &original).unwrap();
                let entry = recycle_package(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    "package",
                    (!whole_package).then_some(A),
                )
                .unwrap();
                let snapshot: InstallManifest = toml::from_str(
                    &fs::read_to_string(workspace.recycle.join(&entry.id).join("manifest.toml"))
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(snapshot.repository_binding, original.repository_binding);
                if !whole_package {
                    let mut changed = workspace.active("package").unwrap();
                    changed.repository_binding.as_mut().unwrap().owner = "OtherOwner".into();
                    archive::write_manifest(&workspace.manifests, &changed).unwrap();
                    assert!(
                        restore(
                            &workspace.recycle,
                            &workspace.manifests,
                            &workspace.game,
                            &entry.id,
                        )
                        .unwrap_err()
                        .contains("recycle.packageUpdated")
                    );
                    assert!(!workspace.game.join(&actual).exists());
                    assert!(workspace.recycle.join(&entry.id).exists());
                    changed.repository_binding = original.repository_binding.clone();
                    archive::write_manifest(&workspace.manifests, &changed).unwrap();
                }
                restore(
                    &workspace.recycle,
                    &workspace.manifests,
                    &workspace.game,
                    &entry.id,
                )
                .unwrap();
                assert_eq!(
                    workspace.active("package").unwrap().repository_binding,
                    original.repository_binding
                );
                assert_eq!(
                    fs::read(workspace.game.join(&actual)).unwrap(),
                    b"bound plugin"
                );
                assert_eq!(fs::read(workspace.game.join(B)).unwrap(), b"sibling plugin");
                assert!(list(&workspace.recycle).unwrap().is_empty());
            }
        }
    }

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
            assert_eq!(error, "recycle.noMovableFiles");
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

    /*
     * 直接删除（用户关闭「优先放入回收站」）：
     * 文件与清单都要消失，且回收站里不能留下任何记录。
     */
    #[test]
    fn purge_removes_files_and_leaves_no_recycle_record() {
        let workspace = Workspace::new();
        workspace.write(A, b"first DLL");
        workspace.write(DATA, b"settings");
        workspace.manifest("package", &[A, DATA]);

        purge_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            None,
        )
        .unwrap();

        assert!(!workspace.game.join(A).exists(), "DLL 应被删除");
        assert!(!workspace.game.join(DATA).exists(), "附属文件应被删除");
        assert!(workspace.active("package").is_none(), "安装清单应被移除");
        assert!(
            list(&workspace.recycle).unwrap().is_empty(),
            "不该留下回收记录"
        );
    }

    /// 直删同样要守住共享文件保护：同 GUID 的其他副本仍在使用时不能移走。
    #[test]
    fn purge_respects_shared_file_protection() {
        let workspace = Workspace::new();
        workspace.write(A, b"same DLL");
        workspace.manifest("package", &[A]);
        workspace.manifest("peer", &[A]);
        let installed = vec![];
        let error = purge_mod_dll(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "package",
            A,
            &installed,
        );
        // 与回收路径一致：共享文件必须有安全替代副本才允许移走
        assert!(error.is_err() || workspace.game.join(A).exists());
    }

    /*
     * 卸载 BepInEx 框架：它的 ZIP 不只解压出 `BepInEx/`，还有 `winhttp.dll`、
     * `doorstop_config.ini`、`run_bepinex.sh` 这些**直接落在游戏根目录**的文件。
     * 清单记的是这些游戏相对路径，所以按 package 回收必须把它们一起带走，
     * 否则用户以为卸干净了，实际 doorstop 还在接管启动。
     */
    #[test]
    fn framework_manifest_recycles_root_level_files_too() {
        let workspace = Workspace::new();
        const FRAMEWORK_FILES: &[&str] = &[
            "BepInEx/core/BepInEx.dll",
            "BepInEx/config/BepInEx.cfg",
            "winhttp.dll",
            "doorstop_config.ini",
            "run_bepinex.sh",
        ];
        for file in FRAMEWORK_FILES {
            workspace.write(file, b"framework");
        }
        workspace.manifest("bepinex-framework", FRAMEWORK_FILES);

        let entry = recycle_package(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            "bepinex-framework",
            None,
        )
        .unwrap();

        assert_eq!(entry.file_count, FRAMEWORK_FILES.len());
        for file in FRAMEWORK_FILES {
            assert!(!workspace.game.join(file).exists(), "{file} 应已移走");
        }
        // 回收记录里也要包含根目录文件，恢复时才能原样还原
        for file in ["winhttp.dll", "doorstop_config.ini", "run_bepinex.sh"] {
            assert!(
                entry.files.iter().any(|item| item == file),
                "{file} 应记录在案"
            );
        }

        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &entry.id,
        )
        .unwrap();
        for file in FRAMEWORK_FILES {
            assert!(workspace.game.join(file).exists(), "{file} 应已恢复");
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
        active.note_i18n = crate::archive::single_language("edited after removal");
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
        assert_eq!(
            crate::archive::primary_translation(&merged.note_i18n),
            "edited after removal"
        );
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
                    "mods.dllNotFound"
                } else {
                    "mods.dllNotInPackage"
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
                "recycle.fileSharedByOtherPackage",
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
            "recycle.fileSharedByOtherPackage"
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
            "recycle.packageUpdated"
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
            "recycle.wrongGameDirectory"
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
            "recycle.packageUpdated"
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
        assert_eq!(result.unwrap_err(), "recycle.partialFailure");
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
            "recycle.insideModsDirectory"
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

    #[test]
    fn installing_repository_recycles_every_package_bound_to_it() {
        // 同一仓库历史上装出过三个包：两个靠 binding 识别、一个靠 repository_id。
        // 安装前必须全部回收，否则装完仍会显示「重复」。
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "Repo", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "stable"
        }))
        .unwrap();

        let binding = || crate::models::RepositoryBinding {
            provider: crate::models::Provider::Github,
            owner: "Owner".into(),
            repository: "Mod".into(),
            tag: "v1.2.3".into(),
            channel: crate::models::Channel::Stable,
        };

        // 包 A：靠 binding 匹配（旧包 ID，与本次安装 ID 不同）
        workspace.write("BepInEx/plugins/Bundle/dup-a.dll", b"dup a");
        let mut a = workspace.manifest("old-package-a", &["BepInEx/plugins/Bundle/dup-a.dll"]);
        a.repository_binding = Some(binding());
        archive::write_manifest(&workspace.manifests, &a).unwrap();

        // 包 B：同样靠 binding 匹配
        workspace.write("BepInEx/plugins/Bundle/dup-b.dll", b"dup b");
        let mut b = workspace.manifest("old-package-b", &["BepInEx/plugins/Bundle/dup-b.dll"]);
        b.repository_binding = Some(binding());
        archive::write_manifest(&workspace.manifests, &b).unwrap();

        // 包 C：repository_id 就是本次 ID
        workspace.write("BepInEx/plugins/Bundle/same-id.dll", b"same id");
        workspace.manifest("repo-1", &["BepInEx/plugins/Bundle/same-id.dll"]);

        // 同 GUID 但属于别的仓库：不能动
        workspace.write("BepInEx/plugins/Other/keep.dll", b"keep");
        let mut keep = workspace.manifest("other-repo", &["BepInEx/plugins/Other/keep.dll"]);
        keep.repository_binding = Some(crate::models::RepositoryBinding {
            provider: crate::models::Provider::Github,
            owner: "SomeoneElse".into(),
            repository: "OtherMod".into(),
            tag: "v9.9.9".into(),
            channel: crate::models::Channel::Stable,
        });
        archive::write_manifest(&workspace.manifests, &keep).unwrap();

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            &["BepInEx/plugins/Bundle/new.dll".into()],
        )
        .unwrap();

        assert_eq!(entries.len(), 3, "三个同仓库包都应被回收");
        // 三个包的清单都已离开活动目录
        assert!(workspace.active("old-package-a").is_none());
        assert!(workspace.active("old-package-b").is_none());
        assert!(workspace.active("repo-1").is_none());
        // 其它仓库不受影响
        assert!(workspace.active("other-repo").is_some());
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Other/keep.dll")).unwrap(),
            b"keep"
        );
        // 被回收的文件已从游戏目录移走
        assert!(
            !workspace
                .game
                .join("BepInEx/plugins/Bundle/dup-a.dll")
                .exists()
        );
        assert!(
            !workspace
                .game
                .join("BepInEx/plugins/Bundle/dup-b.dll")
                .exists()
        );
    }

    #[test]
    fn repository_recycle_skips_unrelated_bindings() {
        // 仓库没装过任何东西时返回空，调用方据此跳过回滚准备
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-fresh", "name": "Fresh", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "stable"
        }))
        .unwrap();

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            &["BepInEx/plugins/new.dll".into()],
        )
        .unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn same_repository_packages_do_not_protect_each_others_files() {
        /*
         * 同一次安装前要回收同一仓库的多个旧包。若把它们互相当成
         * 「其他包」，每个包都会因为「文件仍由其他包使用」而跳过回收，
         * 结果旧副本全部留在磁盘上 —— 这就是「更新后仍显示重复」的根因。
         */
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "Repo", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "beta"
        }))
        .unwrap();

        // 两个旧包登记在同一批文件上（旧版本装出的两份副本）
        workspace.write(A, b"stale a");
        workspace.write(B, b"stale b");
        let binding = crate::models::RepositoryBinding {
            provider: crate::models::Provider::Github,
            owner: "Owner".into(),
            repository: "Mod".into(),
            tag: "v1.8.7.21".into(),
            // 渠道与当前仓库配置不同：旧安装记的是 all，仓库现在配的是 beta
            channel: crate::models::Channel::All,
        };
        let mut first = workspace.manifest("old-1", &[A, B]);
        first.repository_binding = Some(binding.clone());
        archive::write_manifest(&workspace.manifests, &first).unwrap();

        let mut second = workspace.manifest("old-2", &[A, B]);
        second.repository_binding = Some(binding);
        archive::write_manifest(&workspace.manifests, &second).unwrap();

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            &["BepInEx/plugins/new.dll".into()],
        )
        .unwrap();

        assert!(!entries.is_empty(), "至少有一个包产生回收记录");
        // 关键：文件必须真的从游戏目录移走，不能因为「同批包共享」而跳过
        assert!(!workspace.game.join(A).exists(), "旧包 A 的文件应已移走");
        assert!(!workspace.game.join(B).exists(), "旧包 B 的文件应已移走");
        // 两个旧包的清单都不能留存，否则扫描会多出指向空文件的条目
        assert!(workspace.active("old-1").is_none(), "旧包 1 的清单应已移除");
        assert!(workspace.active("old-2").is_none(), "旧包 2 的清单应已移除");
    }

    #[test]
    fn repository_recycle_matches_identity_even_when_channel_changed() {
        // 仓库渠道从 all 改成 beta（或反之）不应让旧安装失去归属：
        // 渠道只决定拉哪个 Release，不改变「同一个仓库」这个事实。
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "Repo", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "beta"
        }))
        .unwrap();

        workspace.write(A, b"old");
        let mut old = workspace.manifest("legacy-package", &[A]);
        old.repository_binding = Some(crate::models::RepositoryBinding {
            provider: crate::models::Provider::Github,
            owner: "Owner".into(),
            repository: "Mod".into(),
            tag: "v1.8.7.21".into(),
            channel: crate::models::Channel::Stable,
        });
        archive::write_manifest(&workspace.manifests, &old).unwrap();

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            &["BepInEx/plugins/new.dll".into()],
        )
        .unwrap();

        assert_eq!(entries.len(), 1, "渠道不同但身份相同的旧包也要回收");
        assert!(!workspace.game.join(A).exists());
        assert!(workspace.active("legacy-package").is_none());
    }

    #[test]
    fn repository_install_recycles_legacy_manual_manifest_of_same_package() {
        /*
         * 真实场景：早期在线安装误走了本地安装路径，留下 `manual-<uuid>`
         * 清单。之后从仓库正常安装时若不回收它，会多出一条「本地安装」
         * 分组下的孤儿条目，并与主 MOD 互判重复。
         */
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "DIYLevel", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "install_target": "BepInEx/plugins/DIYLevel", "enabled": true, "builtin": false,
            "channel": "all"
        }))
        .unwrap();

        // 旧的手动清单：同一个包目录下的依赖 DLL
        workspace.write("BepInEx/plugins/DIYLevel/OC2DIYLevel.dll", b"dep");
        workspace.manifest(
            "manual-legacy",
            &["BepInEx/plugins/DIYLevel/OC2DIYLevel.dll"],
        );

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            // 新包解到同一目录，但文件名不同（旧版依赖名带版本号）
            &["BepInEx/plugins/DIYLevel/DIYLevel.dll".into()],
        )
        .unwrap();

        assert_eq!(entries.len(), 1, "同包目录下的历史 manual 清单应被回收");
        assert!(workspace.active("manual-legacy").is_none());
        assert!(
            !workspace
                .game
                .join("BepInEx/plugins/DIYLevel/OC2DIYLevel.dll")
                .exists()
        );
    }

    #[test]
    fn legacy_manual_cleanup_never_touches_sibling_plugins() {
        /*
         * 反向保护：BepInEx/plugins 是系统容器，平铺其中的散装 DLL
         * 不属于同一个包，绝不能因为「同目录」被误回收。
         */
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "Repo", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "all"
        }))
        .unwrap();

        workspace.write("BepInEx/plugins/Unrelated.dll", b"keep me");
        workspace.manifest("manual-unrelated", &["BepInEx/plugins/Unrelated.dll"]);

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            &["BepInEx/plugins/Other.dll".into()],
        )
        .unwrap();

        assert!(entries.is_empty(), "plugins 根目录不该被当成包目录");
        assert!(workspace.active("manual-unrelated").is_some());
        assert!(
            workspace
                .game
                .join("BepInEx/plugins/Unrelated.dll")
                .exists()
        );
    }

    #[test]
    fn manual_package_is_recycled_by_dll_identity_across_directories() {
        /*
         * 用户实际场景：本地装了 DIYLevel 的 ZIP，管理器把主 MOD 和依赖
         * 一起写进了同一份 manual 清单，但两者可能落在不同子目录。
         * 该仓库后来发新版本时，必须按**包里任一 DLL 的身份**判定整包归属，
         * 与文件散落在哪无关，否则这条 manual 包永远不会被回收。
         */
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "DIYLevel", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "plugin_guid": "com.ch3ngyz.plugin.DIYLevel",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "all"
        }))
        .unwrap();

        // 主 MOD 在子目录，依赖平铺在 plugins 根；两者同属一份 manual 清单
        let main = "BepInEx/plugins/DIYLevel/DIYLevel.dll";
        let dep = "BepInEx/plugins/OC2DIYLevel-v0.10.dll";
        workspace.write(
            main,
            &plugin_blob("com.ch3ngyz.plugin.DIYLevel", "DIYLevel", "1.8.7"),
        );
        workspace.write(dep, b"dependency without plugin metadata");
        workspace.manifest("manual-legacy", &[main, dep]);

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            // 新版本解到完全不同的可见路径，目录判定帮不上忙
            &["BepInEx/plugins/DIYLevelNew/DIYLevel.dll".into()],
        )
        .unwrap();

        assert_eq!(entries.len(), 1, "整包应按主 MOD 的 GUID 命中并回收");
        assert!(workspace.active("manual-legacy").is_none());
        assert!(!workspace.game.join(main).exists(), "主 MOD 应已移走");
        assert!(!workspace.game.join(dep).exists(), "依赖应随整包一起移走");
    }

    #[test]
    fn unrelated_manual_package_survives_repository_install() {
        // 身份不匹配的 manual 包不能因为「同时存在」就被清掉
        let workspace = Workspace::new();
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "repo-1", "name": "DIYLevel", "provider": "github",
            "username": "Owner", "repository": "Mod", "download_mode": "release",
            "plugin_guid": "com.ch3ngyz.plugin.DIYLevel",
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "channel": "all"
        }))
        .unwrap();

        let other = "BepInEx/plugins/Other/Other.dll";
        workspace.write(other, &plugin_blob("com.example.other", "Other", "2.0.0"));
        workspace.manifest("manual-other", &[other]);

        let entries = recycle_repository_packages_before_install(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &repo,
            &["BepInEx/plugins/DIYLevel/DIYLevel.dll".into()],
        )
        .unwrap();

        assert!(entries.is_empty(), "身份不匹配的包不应被回收");
        assert!(workspace.active("manual-other").is_some());
        assert!(workspace.game.join(other).exists());
    }
}
