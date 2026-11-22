//! 业务配置之外的固定启动定位文件；损坏或不可达时禁止悄悄回退到空配置。

use super::{DATA_DIRECTORY, ROOT_UNAVAILABLE};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, Metadata};
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_LOCATION_BYTES: u64 = 16 * 1024;
const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DataLocation {
    schema_version: u32,
    root: PathBuf,
}

fn unavailable(_: impl std::fmt::Display) -> String {
    ROOT_UNAVAILABLE.into()
}

/// 位置与当前数据根完全独立，卸载、迁移与清除数据都不删除它。
pub(crate) fn bootstrap_dir() -> Result<PathBuf, String> {
    let parent = dirs::config_local_dir()
        .ok_or(ROOT_UNAVAILABLE)?
        .join("OC2ModManager-bootstrap");
    validate_directory_chain(&parent)?;
    Ok(parent)
}

pub(crate) fn location_file() -> Result<PathBuf, String> {
    let file = bootstrap_dir()?.join("data-location.json");
    validate_pointer_path(&file)?;
    Ok(file)
}

pub(super) fn validate_root_shape(root: &Path) -> Result<(), String> {
    if !root.is_absolute()
        || root.file_name().is_none_or(|name| name != DATA_DIRECTORY)
        || root
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || std::env::current_exe().is_ok_and(|executable| root == executable)
    {
        return Err(ROOT_UNAVAILABLE.into());
    }
    Ok(())
}

fn reject_link(metadata: &Metadata) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(ROOT_UNAVAILABLE.into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err(ROOT_UNAVAILABLE.into());
    }
    Ok(())
}

/// 检查全部已存在的目录分量，不能让父级链接把固定定位文件重定向到别处。
fn validate_directory_chain(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                reject_link(&metadata)?;
                if !metadata.is_dir() {
                    return Err(ROOT_UNAVAILABLE.into());
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(unavailable(error)),
        }
    }
    // 共用游戏域既有的缺失后缀解析，文件系统错误也必须阻止使用该路径。
    crate::game::resolve_existing_ancestor(path).map_err(unavailable)?;
    Ok(())
}

fn validate_pointer_path(file: &Path) -> Result<(), String> {
    if !file.is_absolute()
        || file.file_name().is_none()
        || file
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ROOT_UNAVAILABLE.into());
    }
    validate_directory_chain(file.parent().ok_or(ROOT_UNAVAILABLE)?)?;
    match fs::symlink_metadata(file) {
        Ok(metadata) => {
            reject_link(&metadata)?;
            if !metadata.is_file() {
                return Err(ROOT_UNAVAILABLE.into());
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(unavailable(error)),
    }
}

fn validate_root(root: &Path, require_exists: bool) -> Result<(), String> {
    validate_root_shape(root)?;
    crate::install_guard::validate_install_dir(root).map_err(unavailable)?;
    let parent = root.parent().ok_or(ROOT_UNAVAILABLE)?;
    validate_directory_chain(parent)?;
    if !parent.try_exists().map_err(unavailable)? {
        // 数据清除后仅允许最后一层缺失；父目录缺失代表磁盘/挂载点不可用。
        return Err(ROOT_UNAVAILABLE.into());
    }
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            reject_link(&metadata)?;
            if !metadata.is_dir() {
                return Err(ROOT_UNAVAILABLE.into());
            }
            Ok(())
        }
        Err(error) if !require_exists && error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(unavailable(error)),
    }
}

pub(super) fn read_location(file: &Path) -> Result<Option<PathBuf>, String> {
    validate_pointer_path(file)?;
    let input = match File::open(file) {
        Ok(input) => input,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unavailable(error)),
    };
    if input.metadata().map_err(unavailable)?.len() > MAX_LOCATION_BYTES {
        return Err(ROOT_UNAVAILABLE.into());
    }
    let mut bytes = Vec::new();
    input
        .take(MAX_LOCATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unavailable)?;
    if bytes.len() as u64 > MAX_LOCATION_BYTES {
        return Err(ROOT_UNAVAILABLE.into());
    }
    let location: DataLocation = serde_json::from_slice(&bytes).map_err(unavailable)?;
    if location.schema_version != SCHEMA_VERSION {
        return Err(ROOT_UNAVAILABLE.into());
    }
    validate_root(&location.root, false)?;
    Ok(Some(location.root))
}

/// 只在固定定位文件完全不存在时用默认路径；自定义根缺失但父盘可达可重建。
pub(crate) fn read_location_at(file: &Path, default: &Path) -> Result<PathBuf, String> {
    Ok(read_location(file)?.unwrap_or_else(|| default.to_path_buf()))
}

/// 迁移已验证完成后原子发布；失败由 NamedTempFile 清理本次临时文件。
pub(crate) fn write_location_at(file: &Path, root: &Path) -> Result<(), String> {
    validate_root(root, true)?;
    validate_pointer_path(file)?;
    let parent = file.parent().ok_or(ROOT_UNAVAILABLE)?;
    fs::create_dir_all(parent).map_err(unavailable)?;
    // 创建完成后重新检查，拒绝预先存在或创建期间出现的 junction 重定向。
    validate_pointer_path(file)?;
    let location = DataLocation {
        schema_version: SCHEMA_VERSION,
        root: root.to_path_buf(),
    };
    let bytes = serde_json::to_vec(&location).map_err(unavailable)?;
    if bytes.len() as u64 > MAX_LOCATION_BYTES {
        return Err(ROOT_UNAVAILABLE.into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(unavailable)?;
    temporary.write_all(&bytes).map_err(unavailable)?;
    temporary.as_file().sync_all().map_err(unavailable)?;
    validate_root(root, true)?;
    validate_pointer_path(file)?;
    let temporary = temporary.into_temp_path();
    crate::mod_changes::transaction::durable_rename(&temporary, file, true).map_err(unavailable)?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|parent| parent.sync_all())
        .map_err(unavailable)?;
    Ok(())
}

#[cfg(test)]
#[path = "location_tests.rs"]
mod tests;
