use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const LIMIT: usize = 2 * 1024 * 1024;
const UNSAFE: &str = "modConfig.unsafePath";

#[derive(Serialize)]
pub struct ModConfigDocument {
    pub guid: String,
    pub file_name: String,
    pub content: String,
    pub revision: String,
}

fn checked_file(game: &Path, guid: &str) -> Result<PathBuf, String> {
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
    /*
     * 分隔符必须交给 `Path` 自己拼：`Path::join("BepInEx/config")` 只补分隔符、
     * 不改写串内已有的 `/`，产物会变成 `...\BepInEx/config\xxx.cfg` 这种混合写法。
     * Windows 的文件 API 容忍混合分隔符，所以 `metadata`/`read` 都正常；
     * 但 Explorer 的 `/select,` 参数解析不容忍 —— 拿到混合路径时直接退回
     * 「此电脑」（2026-11 实测，菜单里的「查看日志」因此打不开日志目录）。
     * 拆成两段 join 才能保证出口永远是原生反斜杠。
     */
    let path = game.join("BepInEx").join("config").join(format!("{guid}.cfg"));
    if !path.is_absolute() {
        return Err(UNSAFE.into());
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err(UNSAFE.into());
        }
        current.push(part);
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| "modConfig.notFound")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(UNSAFE.into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err(UNSAFE.into());
        }
    }
    if !path.is_file() {
        return Err(UNSAFE.into());
    }
    Ok(path)
}

pub fn exists(game: &Path, guid: &str) -> bool {
    checked_file(game, guid).is_ok()
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
    let file = fs::File::open(path).map_err(|_| "modConfig.readFailed")?;
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "modConfig.readFailed")?;
    if bytes.len() > LIMIT {
        return Err("modConfig.tooLarge".into());
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
        .map_err(|_| "modConfig.invalidUtf8")?;
    if content.contains('\0') {
        return Err("modConfig.invalidUtf8".into());
    }
    Ok(ModConfigDocument {
        guid: guid.into(),
        file_name: format!("{guid}.cfg"),
        content: content.into(),
        revision: revision(path, bytes),
    })
}

pub fn read(game: &Path, guid: &str) -> Result<ModConfigDocument, String> {
    let path = checked_file(game, guid)?;
    document(&path, guid, &bytes(&path)?)
}

pub fn save(
    game: &Path,
    guid: &str,
    expected: &str,
    content: &str,
) -> Result<ModConfigDocument, String> {
    let path = checked_file(game, guid)?;
    let original = bytes(&path)?;
    document(&path, guid, &original)?;
    if revision(&path, &original) != expected {
        return Err("modConfig.externallyModified".into());
    }
    let mut replacement = Vec::new();
    if original.starts_with(&[0xef, 0xbb, 0xbf]) {
        replacement.extend([0xef, 0xbb, 0xbf]);
    }
    replacement.extend_from_slice(content.as_bytes());
    if replacement.len() > LIMIT {
        return Err("modConfig.tooLarge".into());
    }
    let result = document(&path, guid, &replacement)?;
    let temporary = path.with_file_name(format!(".oc2-config-{}.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "modConfig.saveFailed")?;
        file.write_all(&replacement)
            .and_then(|_| file.sync_all())
            .map_err(|_| "modConfig.saveFailed")?;
        drop(file);
        checked_file(game, guid)?;
        if revision(&path, &bytes(&path)?) != expected {
            return Err("modConfig.externallyModified".into());
        }
        fs::rename(&temporary, &path).map_err(|_| "modConfig.saveFailed".to_string())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_directory_junction_is_rejected() {
        let root = std::env::temp_dir().join(format!("oc2-config-test-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let outside = root.join("outside");
        fs::create_dir_all(game.join("BepInEx")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("example.cfg"), "private").unwrap();
        let link = game.join("BepInEx/config");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let status = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(outside.to_string_lossy().replace('/', "\\"))
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "junction creation failed: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(!exists(&game, "example"));
        assert_eq!(read(&game, "example").err().as_deref(), Some(UNSAFE));
        assert!(save(&game, "example", "", "changed").is_err());
        assert_eq!(
            fs::read_to_string(outside.join("example.cfg")).unwrap(),
            "private"
        );
        #[cfg(windows)]
        fs::remove_dir(&link).unwrap();
        #[cfg(unix)]
        fs::remove_file(&link).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn config_round_trip_preserves_bom_crlf_and_rejects_stale_or_unsafe_writes() {
        let game = std::env::temp_dir().join(format!("oc2-config-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(game.join("BepInEx/config")).unwrap();
        let guid = "com.ch3ngyz.plugin.HostUtilities";
        let path = game.join(format!("BepInEx/config/{guid}.cfg"));
        fs::write(&path, b"\xef\xbb\xbf[General]\r\nEnabled = true\r\n").unwrap();
        assert!(exists(&game, guid));
        assert!(!exists(&game, "missing"));
        for bad in ["../escape", "foo/bar", "foo\\bar", "foo:stream", "", ".."] {
            assert!(read(&game, bad).is_err());
        }
        let loaded = read(&game, guid).unwrap();
        let saved = save(
            &game,
            guid,
            &loaded.revision,
            "[General]\r\nEnabled = false\r\n",
        )
        .unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            b"\xef\xbb\xbf[General]\r\nEnabled = false\r\n"
        );
        assert!(save(&game, guid, &loaded.revision, "stale").is_err());
        fs::write(&path, "external change").unwrap();
        assert!(save(&game, guid, &saved.revision, "overwrite").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(read(&game, guid).is_err());
        fs::write(&path, vec![b'x'; LIMIT + 1]).unwrap();
        assert!(read(&game, guid).is_err());
        assert_eq!(
            fs::read_dir(game.join("BepInEx/config")).unwrap().count(),
            1
        );
        fs::remove_dir_all(&game).unwrap();
    }
}
