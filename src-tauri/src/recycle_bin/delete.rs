use super::errors::UNSAFE_PATH;
use super::errors::io_error;
use super::paths::is_link;
use super::paths::record_directory;
use super::paths::validate_absolute_path;
use super::records::list;
use super::records::load_record;
use super::selection::recycle_mod_dll;
use super::selection::recycle_package;
use super::selection::recycle_selected_mod;
use super::selection::recycle_unmanaged;
use crate::archive;
use std::fs;
use std::path::Path;

// Walk without following links, and validate the entire tree before deleting any file.
// Metadata paths are never used for permanent deletion: only the UUID-owned directory.
pub(super) fn remove_entry_directory(directory: &Path) -> Result<(), String> {
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

/// 逐条删除 `list` 返回的有效、未恢复记录，而不递归删除回收站根目录。
///
/// 列表会跳过损坏记录和已恢复记录；这些目录不在本次清理范围内。
/// 单条删除再次校验记录，失败立即中止并报告已删除数；此前成功项不回滚。
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
 * ===== 直接删除（复用回收事务，成功后清理暂存记录）=====
 *
 * 用户关闭「删除 MOD 时优先放入回收站」后，删除操作应直接移除文件。
 *
 * 实现方式是**先按原回收流程处理，再立即彻底删除刚生成的记录**，
 * 而不是另写一套删除逻辑。原因：
 *   · 回收流程已经处理了共享文件保护、`.disabled` 变体、清单快照、
 *     空目录清理、失败回滚 —— 这些都是踩过多轮坑才稳定的；
 *   · 复制一份「删除」实现迟早与回收逻辑分叉，出现「回收安全、直删
 *     丢共享文件」这类只在特定配置下复现的问题；
 *
 * 清理成功才不留记录；清理失败会直接上抛，可能留下完整或部分暂存文件，
 * 不能保证失败后一定可恢复。移动优先 rename，失败走校验复制，不假定同盘。
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
