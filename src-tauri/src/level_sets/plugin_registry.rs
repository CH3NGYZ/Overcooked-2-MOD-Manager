//! 随包插件登记表的持久化与归属。

use super::identity::level_set_identity_matches;
use super::paths::is_safe_relative;
use crate::asset_bundle::LevelSetMetadata;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// 登记表文件名（`manifests/` 下）。
pub const LEVEL_SET_PLUGIN_REGISTRY: &str = "level-set-plugins.json";

/// 登记表结构版本：格式变化时递增，读到不认识的更高版本按空表处理。
const LEVEL_SET_PLUGIN_REGISTRY_VERSION: u32 = 1;

/// 登记表里的关卡集身份（`levelSetName + levelSetNameZH + author`）。
///
/// 与 `level_set_identity_matches` 同一套规则：中文名允许为空（老资源不写），
/// 空英文名不匹配任何东西。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelSetPluginOwner {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub name_zh: String,
    #[serde(default)]
    pub author: String,
}

impl LevelSetPluginOwner {
    pub fn from_metadata(metadata: &LevelSetMetadata) -> Self {
        Self {
            name: metadata.name.clone(),
            name_zh: metadata.name_zh.clone().unwrap_or_default(),
            author: metadata.author.clone(),
        }
    }

    /// 与一张已解析的关卡集是否同一身份。
    pub fn matches(&self, metadata: &LevelSetMetadata) -> bool {
        level_set_identity_matches(
            &metadata.name,
            metadata.name_zh.as_deref(),
            &metadata.author,
            &self.name,
            Some(self.name_zh.as_str()).filter(|value| !value.is_empty()),
            &self.author,
        )
    }
}

/// 登记表里的一条：一个随包 DLL 与它的归属集身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredLevelSetPlugin {
    /// 相对游戏目录的路径，正斜杠归一化（`BepInEx/plugins/X/Y.dll`）。
    pub dll: String,
    #[serde(default)]
    pub owner: LevelSetPluginOwner,
}

#[derive(Debug, Serialize, Deserialize)]
struct LevelSetPluginRegistryFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Vec<RegisteredLevelSetPlugin>,
}

pub(super) fn registry_path(manifests_dir: &Path) -> PathBuf {
    manifests_dir.join(LEVEL_SET_PLUGIN_REGISTRY)
}

/// 归一化登记路径：正斜杠、去掉 `./` 前缀、必须是安全的相对路径。
///
/// 登记表是磁盘上的普通 JSON，可能被用户手改；路径直接参与「跳过 MOD 扫描」
/// 与「卸载时删文件」，所以读到不安全的值一律丢弃而不是原样使用。
fn normalize_registered_dll(dll: &str) -> Option<String> {
    let normalized = dll.trim().replace('\\', "/");
    let normalized = normalized.trim_start_matches("./").to_string();
    if !is_safe_relative(&normalized) {
        return None;
    }
    Some(normalized)
}

/// 读登记表。
///
/// 文件不存在、解析失败、版本比当前新都返回空表：登记表只影响「哪些 DLL 归
/// 地图」，坏掉时退回「都算 MOD」的旧行为，不能让整次扫描失败。
pub fn read_registered_plugins(manifests_dir: &Path) -> Vec<RegisteredLevelSetPlugin> {
    let path = registry_path(manifests_dir);
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let parsed: LevelSetPluginRegistryFile = match serde_json::from_str(&text) {
        Ok(parsed) => parsed,
        Err(error) => {
            log::warn!(
                "随包插件登记表解析失败，按空表处理：文件={}，原因={}",
                path.display(),
                error
            );
            return Vec::new();
        }
    };
    if parsed.version > LEVEL_SET_PLUGIN_REGISTRY_VERSION {
        log::warn!(
            "随包插件登记表版本比当前程序新，按空表处理：文件={}，版本={}",
            path.display(),
            parsed.version
        );
        return Vec::new();
    }
    let mut seen = HashSet::new();
    parsed
        .entries
        .into_iter()
        .filter_map(|entry| {
            let dll = normalize_registered_dll(&entry.dll)?;
            if !seen.insert(dll.to_lowercase()) {
                return None;
            }
            Some(RegisteredLevelSetPlugin {
                dll,
                owner: entry.owner,
            })
        })
        .collect()
}

/// 写登记表（先写临时文件再改名，与安装清单同一手法：中途失败不会留下半个 JSON）。
pub fn write_registered_plugins(
    manifests_dir: &Path,
    entries: &[RegisteredLevelSetPlugin],
) -> Result<(), String> {
    fs::create_dir_all(manifests_dir).map_err(|error| error.to_string())?;
    let file = LevelSetPluginRegistryFile {
        version: LEVEL_SET_PLUGIN_REGISTRY_VERSION,
        entries: entries.to_vec(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
    let path = registry_path(manifests_dir);
    let temp = manifests_dir.join(format!(
        "{LEVEL_SET_PLUGIN_REGISTRY}.{}.tmp",
        uuid::Uuid::new_v4()
    ));
    fs::write(&temp, text).map_err(|error| error.to_string())?;
    fs::rename(&temp, &path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })
}

/// 登记表里全部 DLL 的路径（小写、正斜杠），供 MOD 扫描避让。
pub fn registered_plugin_paths(manifests_dir: &Path) -> Vec<String> {
    read_registered_plugins(manifests_dir)
        .into_iter()
        .map(|entry| entry.dll.to_lowercase())
        .collect()
}

/// 把新登记的条目并进已有登记表：同一个 DLL 以新归属为准（重装 / 换包）。
pub(super) fn merge_registered_plugins(
    existing: Vec<RegisteredLevelSetPlugin>,
    added: Vec<RegisteredLevelSetPlugin>,
) -> Vec<RegisteredLevelSetPlugin> {
    let mut merged = existing;
    for entry in added {
        let key = entry.dll.to_lowercase();
        merged.retain(|current| current.dll.to_lowercase() != key);
        merged.push(entry);
    }
    merged.sort_by(|left, right| left.dll.to_lowercase().cmp(&right.dll.to_lowercase()));
    merged
}

/// 从登记表里删掉 `owned` 里的条目（卸载关卡集时用）。
///
/// 写失败只 warn：集目录与 DLL 已经处理完，这里失败最多留下几条指向不存在
/// 文件的记录 —— 扫描会跳过它们，不值得把「卸载成功」变成错误。
pub(super) fn forget_registered_plugins(
    manifests_dir: &Path,
    registered: &[RegisteredLevelSetPlugin],
    owned: &[RegisteredLevelSetPlugin],
    dir_name: &str,
) {
    if owned.is_empty() {
        return;
    }
    let owned_keys: HashSet<String> = owned.iter().map(|entry| entry.dll.to_lowercase()).collect();
    let remaining: Vec<RegisteredLevelSetPlugin> = registered
        .iter()
        .filter(|entry| !owned_keys.contains(&entry.dll.to_lowercase()))
        .cloned()
        .collect();
    if let Err(error) = write_registered_plugins(manifests_dir, &remaining) {
        log::warn!(
            "清随包插件登记失败，可能留下指向不存在文件的记录：目录={}，原因={}",
            dir_name,
            error
        );
    }
}
