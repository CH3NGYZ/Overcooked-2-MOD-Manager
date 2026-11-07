use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// 地图依赖快照的文件名（与 MOD 安装清单同目录，但不是 TOML，`read_manifests` 不会读它）。
const MAP_RELATION_SNAPSHOTS_FILE: &str = "map-relations.json";

fn map_relations_path(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join(MAP_RELATION_SNAPSHOTS_FILE)
}

/// 读取地图依赖快照。
///
/// 缺失或损坏都返回默认值：这份文件只是「线上声明查不到时的回退」，
/// 读不出来不该让地图管理页打不开，也不该覆盖用户已有的安装状态。
pub fn read_map_relation_snapshots(manifest_dir: &Path) -> crate::models::MapRelationSnapshots {
    let path = map_relations_path(manifest_dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            log::trace!(
                "读取地图依赖快照失败或不存在：路径={}，原因={}",
                path.display(),
                error
            );
            return Default::default();
        }
    };
    match serde_json::from_str(&text) {
        Ok(snapshots) => snapshots,
        Err(error) => {
            log::warn!(
                "解析地图依赖快照失败，按空快照处理：路径={}，原因={}",
                path.display(),
                error
            );
            Default::default()
        }
    }
}

/// 写入地图依赖快照（临时文件 + rename，与安装清单同一种落盘方式）。
pub fn write_map_relation_snapshots(
    manifest_dir: &Path,
    snapshots: &crate::models::MapRelationSnapshots,
) -> Result<(), String> {
    fs::create_dir_all(manifest_dir).map_err(|e| e.to_string())?;
    let path = map_relations_path(manifest_dir);
    /*
     * 写入一律标当前版本。
     *
     * 读出来的默认值（文件缺失或损坏）`schema_version` 是 0，直接回写会把
     * 0 固化进文件；把版本归一到写入口，调用方就不必各自记得补。
     */
    let mut snapshots = snapshots.clone();
    snapshots.schema_version = crate::models::default_snapshot_schema_version();
    let text = serde_json::to_string_pretty(&snapshots).map_err(|e| e.to_string())?;
    let temp = manifest_dir.join(format!(
        "{}.{}.tmp",
        MAP_RELATION_SNAPSHOTS_FILE,
        Uuid::new_v4()
    ));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    let result = fs::rename(&temp, &path).map_err(|e| e.to_string());
    match &result {
        Ok(()) => log::debug!("地图依赖快照写入完成：条目={}", snapshots.entries.len()),
        Err(error) => log::error!("地图依赖快照写入失败：原因={}", error),
    }
    result
}

#[cfg(test)]
mod tests;
