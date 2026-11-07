use crate::archive;
use crate::framework::backup_bepinex_dir;
use crate::framework::restore_backup_dir;
use crate::framework::restore_preserved_dirs;
use crate::game;
use std::fs;
use std::path::Path;

/// 框架安装清单的固定 id。
///
/// 框架不是 MOD，但它的 ZIP 解压结果同样需要一份清单：
/// 卸载时要按清单把**所有**解压出来的文件回收（`BepInEx/` 目录之外还有
/// `winhttp.dll`、`doorstop_config.ini`、`run_bepinex.sh` 等散落在游戏根目录的
/// 文件），只删 `BepInEx` 目录会留下这些孤儿文件。
pub(crate) const BEPINEX_MANIFEST_ID: &str = "bepinex-framework";

pub(crate) fn install_framework_archive(
    archive_path: &Path,
    game_root: &Path,
    backup_root: &Path,
    manifests_dir: &Path,
    package: crate::bepinex::Package,
    reinstall: bool,
    release_tag: &str,
) -> Result<(), String> {
    let framework = game::safe_game_path(game_root, "BepInEx")?;
    /*
     * 「已安装」必须看核心程序集，不能看 `BepInEx/` 目录是否存在。
     *
     * 装过 MOD 的游戏目录里必定有 `BepInEx/plugins/`，那个目录从来没有
     * 框架核心。用目录判断会得到「已安装」的结论，于是首次安装被拒绝，
     * 用户只能点「重新安装」——而重装路径又会把整个 BepInEx 删掉重建。
     */
    let installed = crate::bepinex::installed(game_root);
    if installed && !reinstall {
        return Err("framework.alreadyInstalled".into());
    }
    /*
     * 只有真装过框架才做「整目录备份 → 删除重建 → 恢复 plugins/config」。
     *
     * 残留空壳（目录在、核心不在）直接就地解压：ZIP 只写框架自己的文件，
     * 已有的 plugins/config 原样保留，不必先拷一份几个 GB 的备份再恢复。
     */
    let backup = if installed {
        Some(backup_bepinex_dir(game_root, backup_root)?)
    } else {
        None
    };
    let result = (|| {
        if installed {
            archive::clear_readonly(&framework)?;
            fs::remove_dir_all(&framework).map_err(|e| {
                format!(
                    "error.framework.cleanupFailed?reason={}",
                    crate::locale_tag::encode(&e.to_string())
                )
            })?;
        }
        let pending = archive::extract_framework_zip_pending(archive_path, game_root)?;
        if package.native() {
            crate::bepinex::prepare_native(game_root, package)?;
        }
        if let Some(backup) = &backup {
            restore_preserved_dirs(backup, game_root)?;
        }
        /*
         * 先落清单再提交文件：两者顺序反过来的话，commit 之后写清单失败会
         * 留下「文件已就位但没记录」的状态，卸载时就回收不到这些文件。
         * 清单内容取 `pending.files()` —— 那是本次实际解压出的游戏相对路径，
         * 覆盖 `BepInEx/` 与散落在根目录的 winhttp.dll / doorstop_config.ini 等。
         */
        let manifest = archive::new_manifest(
            BEPINEX_MANIFEST_ID,
            release_tag,
            archive::single_language("BepInEx"),
            Default::default(),
            pending.files().to_vec(),
            Vec::new(),
        );
        archive::write_manifest(manifests_dir, &manifest)?;
        pending.commit();
        Ok(())
    })();
    if let Err(error) = result {
        if let Some(backup) = backup {
            restore_backup_dir(&backup, game_root).map_err(|restore| {
                log::error!(
                    "BepInEx 失败回滚未完成：备份={}，原因={restore}",
                    backup.display()
                );
                "framework.installFailedRollbackIncomplete".to_string()
            })?;
        }
        return Err(error);
    }
    Ok(())
}

/// 列出框架自身在游戏目录里写入的文件（游戏相对路径）。
///
/// 只包含框架随 ZIP 解压出的东西：`BepInEx/core`、`BepInEx/doorstop_libs`
/// 以及散落在游戏根目录的加载器文件。`plugins/`、`config/`、`cache/`
/// 这些目录里是用户或 MOD 产生的内容，绝不能出现在这份清单里。
pub(crate) fn framework_manifest_files(game_root: &Path) -> Vec<String> {
    let root = game_root.join("BepInEx");
    let mut files = Vec::new();
    for directory in ["core", "doorstop_libs"] {
        for entry in walkdir::WalkDir::new(root.join(directory))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if let Ok(relative) = entry.path().strip_prefix(game_root) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for name in [
        "winhttp.dll",
        "winhttp.dll.disabled",
        "doorstop_config.ini",
        "run_bepinex.sh",
        "libdoorstop.so",
        "libdoorstop.dylib",
        "doorstop_libs/libdoorstop_x64.so",
        "doorstop_libs/libdoorstop_x64.dylib",
    ] {
        if game_root.join(name).is_file() {
            files.push(name.to_string());
        }
    }
    files
}

#[cfg(test)]
mod framework_install_tests;
