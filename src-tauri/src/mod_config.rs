use crate::file_check::log_path;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const LIMIT: usize = 2 * 1024 * 1024;
const UNSAFE: &str = "modConfig.unsafePath";
const READ_FAILED: &str = "modConfig.readFailed";
const SAVE_FAILED: &str = "modConfig.saveFailed";
const TOO_LARGE: &str = "modConfig.tooLarge";
const BAD_ENCODING: &str = "modConfig.invalidUtf8";
const CHANGED: &str = "modConfig.externallyModified";

#[derive(Serialize)]
pub struct ModConfigDocument {
    pub guid: String,
    pub file_name: String,
    pub content: String,
    pub revision: String,
}

fn checked_file(game: &Path, guid: &str, diagnose: bool) -> Result<PathBuf, String> {
    if guid.trim().is_empty()
        || guid.ends_with(['.', ' '])
        || guid
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || guid == "."
        || guid == ".."
    {
        if diagnose {
            log::warn!(
                "[mod-config] 拒绝配置路径：stage=guid_validation，game_root={:?}，reason=invalid_guid",
                log_path(game),
            );
        }
        return Err(UNSAFE.into());
    }
    /*
     * 分隔符必须交给 `Path` 自己拼：`Path::join("BepInEx/config")` 只补分隔符、
     * 不改写串内已有的 `/`，产物会变成 `...\BepInEx/config\xxx.cfg` 这种混合写法。
     * Windows 的文件 API 容忍混合分隔符，所以 `metadata`/`read` 都正常；
     * 但这条路径还会被交给系统 Shell：早期用 `explorer.exe /select,` 时混合分隔符
     * 会让它直接退回「此电脑」（2026-11 实测，菜单里的「查看日志」因此打不开）。
     * 定位入口已换成 `shell_open::reveal_in_file_manager`（SHOpenFolderAndSelectItems，
     * 没有命令行解析），拆成两段 join 仍然保留：不让中间层依赖下游一定会归一。
     */
    let path = game
        .join("BepInEx")
        .join("config")
        .join(format!("{guid}.cfg"));
    if !path.is_absolute() {
        if diagnose {
            log::warn!(
                "[mod-config] 拒绝配置路径：stage=path_validation，path={:?}，reason=relative_path",
                log_path(&path),
            );
        }
        return Err(UNSAFE.into());
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            if diagnose {
                log::warn!(
                    "[mod-config] 拒绝配置路径：stage=path_validation，path={:?}，reason=unsafe_component",
                    log_path(&path),
                );
            }
            return Err(UNSAFE.into());
        }
        current.push(part);
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            if diagnose {
                log_config_io("path_metadata", &current, None, &error);
            }
            "modConfig.notFound"
        })?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                if diagnose {
                    log::warn!(
                        "[mod-config] 拒绝重解析路径：stage=path_validation，path={:?}",
                        log_path(&current),
                    );
                }
                return Err(UNSAFE.into());
            }
        }
        if metadata.file_type().is_symlink() {
            if diagnose {
                log::warn!(
                    "[mod-config] 拒绝链接路径：stage=path_validation，path={:?}",
                    log_path(&current),
                );
            }
            return Err(UNSAFE.into());
        }
    }
    if !path.is_file() {
        if diagnose {
            log::warn!(
                "[mod-config] 配置不是可用的普通文件：stage=path_validation，path={:?}",
                log_path(&path),
            );
        }
        return Err(UNSAFE.into());
    }
    Ok(path)
}

pub fn exists(game: &Path, guid: &str) -> bool {
    checked_file(game, guid, false).is_ok()
}

/// 该 GUID 是否在 `BepInEx/logs/<guid>` 下有日志目录。
///
/// 只判断「目录存在且是一个真实目录」：不创建、不读取内容，
/// 也不跟随目录联接逃出游戏根目录 —— 前端据此决定要不要显示日志按钮。
pub fn logs_dir_exists(game: &Path, guid: &str) -> bool {
    let Ok(path) = checked_logs_dir(game, guid) else {
        return false;
    };
    fs::metadata(&path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

/// 校验并返回 `BepInEx/logs/<guid>` 的真实路径。
pub fn checked_logs_dir(game: &Path, guid: &str) -> Result<PathBuf, String> {
    if guid.trim().is_empty()
        || guid.ends_with(['.', ' '])
        || guid
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || guid == "."
        || guid == ".."
    {
        return Err(UNSAFE.into());
    }
    // 同 `checked_file`：拆成两段 join，保证出口是纯原生分隔符。
    let path = game.join("BepInEx").join("logs").join(guid);
    if !path.is_absolute() {
        return Err(UNSAFE.into());
    }
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(UNSAFE.into());
    }
    Ok(path)
}

fn bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .inspect_err(|error| log_config_io("open", path, None, error))
        .map_err(|_| READ_FAILED)?;
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .inspect_err(|error| log_config_io("read", path, None, error))
        .map_err(|_| READ_FAILED)?;
    if bytes.len() > LIMIT {
        log::warn!(
            "[mod-config] 配置超过读取上限：path={:?}，limit={LIMIT}",
            log_path(path),
        );
        return Err(TOO_LARGE.into());
    }
    Ok(bytes)
}

fn revision(path: &Path, bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(path.to_string_lossy().as_bytes());
    hash.update([0]);
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

fn document(path: &Path, guid: &str, bytes: &[u8]) -> Result<ModConfigDocument, String> {
    let content = std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
        .map_err(|error| {
            log::warn!(
                "[mod-config] 配置编码无效：path={:?}，bytes={}，invalid_at={}，invalid_length={:?}",
                log_path(path),
                bytes.len(),
                error.valid_up_to(),
                error.error_len(),
            );
            BAD_ENCODING
        })?;
    if content.contains('\0') {
        log::warn!(
            "[mod-config] 配置含空字节：path={:?}，bytes={}",
            log_path(path),
            bytes.len()
        );
        return Err(BAD_ENCODING.into());
    }
    Ok(ModConfigDocument {
        guid: guid.into(),
        file_name: format!("{guid}.cfg"),
        content: content.into(),
        revision: revision(path, bytes),
    })
}

pub fn read(game: &Path, guid: &str) -> Result<ModConfigDocument, String> {
    let path = checked_file(game, guid, true)?;
    let bytes = bytes(&path)?;
    let result = document(&path, guid, &bytes)?;
    log::info!(
        "[mod-config] 配置读取完成：path={:?}，bytes={}",
        log_path(&path),
        bytes.len()
    );
    Ok(result)
}

pub fn save(
    game: &Path,
    guid: &str,
    expected: &str,
    content: &str,
) -> Result<ModConfigDocument, String> {
    let path = checked_file(game, guid, true)?;
    let original = bytes(&path)?;
    if document(&path, guid, &original)?.revision != expected {
        log::warn!(
            "[mod-config] 配置已由外部修改，未覆盖：stage=initial_revision，path={:?}，bytes={}",
            log_path(&path),
            original.len()
        );
        return Err(CHANGED.into());
    }
    let mut replacement = Vec::new();
    if original.starts_with(&[0xef, 0xbb, 0xbf]) {
        replacement.extend([0xef, 0xbb, 0xbf]);
    }
    replacement.extend_from_slice(content.as_bytes());
    if replacement.len() > LIMIT {
        log::warn!(
            "[mod-config] 待保存配置超过上限：path={:?}，bytes={}，limit={LIMIT}",
            log_path(&path),
            replacement.len()
        );
        return Err(TOO_LARGE.into());
    }
    let result = document(&path, guid, &replacement)?;
    let temporary = path.with_file_name(format!(".oc2-config-{}.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .inspect_err(|error| log_config_io("create_temporary", &path, Some(&temporary), error))
            .map_err(|_| SAVE_FAILED)?;
        file.write_all(&replacement)
            .inspect_err(|error| log_config_io("write_temporary", &path, Some(&temporary), error))
            .map_err(|_| SAVE_FAILED)?;
        file.sync_all()
            .inspect_err(|error| log_config_io("sync_temporary", &path, Some(&temporary), error))
            .map_err(|_| SAVE_FAILED)?;
        drop(file);
        checked_file(game, guid, true)?;
        if revision(&path, &bytes(&path)?) != expected {
            log::warn!(
                "[mod-config] 提交前发现配置已由外部修改，未覆盖：stage=pre_replace_revision，path={:?}，temporary={:?}",
                log_path(&path),
                log_path(&temporary),
            );
            return Err(CHANGED.into());
        }
        fs::rename(&temporary, &path)
            .inspect_err(|error| log_config_io("replace", &path, Some(&temporary), error))
            .map_err(|_| SAVE_FAILED.into())
    })();
    if temporary.exists()
        && let Err(error) = fs::remove_file(&temporary)
    {
        log_config_io("cleanup_temporary", &path, Some(&temporary), &error);
    }
    write_result?;
    log::info!(
        "[mod-config] 配置保存完成：path={:?}，before_bytes={}，after_bytes={}，保留BOM={}",
        log_path(&path),
        original.len(),
        replacement.len(),
        original.starts_with(&[0xef, 0xbb, 0xbf])
    );
    Ok(result)
}

fn log_config_io(stage: &str, path: &Path, temporary: Option<&Path>, error: &std::io::Error) {
    log::warn!(
        "[mod-config] 配置文件操作失败：stage={stage}，path={:?}，temporary={:?}，kind={:?}，os_code={:?}",
        log_path(path),
        temporary.map(log_path),
        error.kind(),
        error.raw_os_error(),
    );
}

#[cfg(test)]
mod tests;
