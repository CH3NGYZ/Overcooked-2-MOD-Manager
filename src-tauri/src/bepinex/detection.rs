use crate::display_path::display_path;
use crate::file_metadata::read_file_version;
use crate::models::BepInExInfo;
use std::fs;
use std::path::Path;

pub fn detect_bepinex(game: &Path, backup_root: Option<&Path>) -> BepInExInfo {
    log::debug!("开始探测 BepInEx：游戏目录={}", game.display());
    let root = game.join("BepInEx");
    let core = root.join("core");
    let primary = core.join("BepInEx.dll");
    let secondary = core.join("BepInEx.Core.dll");
    /*
     * 「已安装」= 框架核心程序集存在，**不是** `BepInEx/` 目录存在。
     *
     * 装任何 MOD 都会创建 `BepInEx/plugins/`，那个空壳会一直留在游戏目录里。
     * 旧实现用 `root.is_dir()` 判断，于是没装框架也报「已安装」，
     * 界面给出启用开关和卸载按钮 —— 而开关必然失败（找不到加载器）。
     *
     * 目录在但核心不在 = 残缺（用户装过 MOD、没装框架，或框架被手工删了一半），
     * 单独用 `has_residue` 表达，界面据此提示「安装会保留 plugins/config」。
     */
    let installed = primary.is_file() || secondary.is_file();
    let has_residue = !installed && root.is_dir();
    let (version, source) = [primary, secondary]
        .into_iter()
        .find_map(|file| {
            read_file_version(&file)
                .filter(|value| value != "0.0.0" && value != "0.0.0.0")
                .map(|v| (Some(v), Some("FileVersion".into())))
        })
        .unwrap_or((None, None));
    let mut backup_roots = vec![game.to_path_buf()];
    if let Some(root) = backup_root {
        if root != game {
            backup_roots.push(root.to_path_buf());
        }
    }
    let mut backups = backup_roots
        .into_iter()
        .flat_map(|root| {
            fs::read_dir(root)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.flatten())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("BepInEx.backup-")
                        && entry.path().is_dir()
                })
                .map(|entry| display_path(entry.path()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    backups.sort();
    let result = BepInExInfo {
        installed,
        has_residue,
        has_config: crate::mod_config::exists(game, "BepInEx"),
        version,
        version_source: source,
        backups,
    };
    log::info!(
        "BepInEx 探测完成：已安装={}，残留目录={}，版本={}",
        result.installed,
        result.has_residue,
        result.version.as_deref().unwrap_or("未知")
    );
    result
}
