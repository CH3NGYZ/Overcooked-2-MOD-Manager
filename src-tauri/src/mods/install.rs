//! Installed repository identity; historical package fixtures are test-only.
use crate::archive;
use crate::models::*;
#[cfg(test)]
use crate::{game, recycle_bin};
#[cfg(test)]
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use uuid::Uuid;

pub(crate) fn mod_repository_install_identity(
    repo: &ModRepositoryConfig,
    config: &AppConfig,
    manifests_dir: &Path,
) -> Result<ModRepositoryConfig, String> {
    if manifests_dir
        .join(format!("{}.toml", repo.id))
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        return Ok(repo.clone());
    }
    // Keep the installed package identity when an old source vanished. Never
    // merge another still-configured package merely because its coordinates match.
    let matches = archive::read_manifests(manifests_dir)
        .into_iter()
        .filter(|manifest| {
            !config
                .repositories
                .iter()
                .any(|configured| configured.id == manifest.repository_id)
                && manifest
                    .repository_binding
                    .as_ref()
                    .is_some_and(|binding| binding.is_valid() && binding.matches_repository(repo))
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err("mods.multipleOldPackages".into());
    }
    let mut result = repo.clone();
    if let Some(manifest) = matches.first() {
        result.id = manifest.repository_id.clone();
    }
    Ok(result)
}

// Legacy package fixtures remain test-only; live writes use mod_changes::transaction.
#[cfg(test)]
pub(crate) fn install_downloaded_mod_repository(
    asset_path: &Path,
    download_name: &str,
    game_root: &Path,
    manifests_dir: &Path,
    recycle_root: &Path,
    repo: &ModRepositoryConfig,
    release_tag: &str,
    prerelease: bool,
    // 安装成功后是否保留旧包的回收记录（= 用户开启了「优先放入回收站」）。
    keep_recycled: bool,
) -> Result<usize, String> {
    let plan = if download_name.to_ascii_lowercase().ends_with(".zip") {
        Some(archive::prepare_repository_mod_zip(
            asset_path,
            game_root,
            &repo.install_target,
            download_name,
        )?)
    } else {
        game::safe_game_path(
            game_root,
            &Path::new(&repo.install_target)
                .join(download_name)
                .to_string_lossy(),
        )?;
        None
    };
    let replacement_files = if let Some(plan) = &plan {
        archive::prepared_mod_zip_files(asset_path, game_root, plan)?
    } else {
        vec![
            Path::new(&repo.install_target)
                .join(download_name)
                .to_string_lossy()
                .replace('\\', "/"),
        ]
    };
    /*
     * 安装前回收**所有**归属该仓库的旧包，而不只是 repository_id 完全相等的
     * 那一个。同一仓库历史上可能装出过多个包（换过包 ID、旧清单未清理），
     * 只回收其中一个会让装完仍然显示「重复」。用户要的语义是：
     * 更新一次就把同仓库的旧装全部清掉，再装新的。
     *
     * 回收失败必须中止安装并给出可操作的提示：此时旧包可能只移走了一部分，
     * 继续安装会让两个版本的文件混在一起。让用户手动删除是最安全的出路。
     *
     * 即使配置了「直接删除」，这里也**先走回收**：安装过程可能失败，
     * 失败时必须能把旧包原样恢复（见下面的回滚）。回收记录在安装成功后
     * 才被清掉 —— 那时暂存区里的旧文件确实不再需要了。
     */
    let recycled = recycle_bin::recycle_repository_packages_before_install(
        recycle_root,
        manifests_dir,
        game_root,
        repo,
        &replacement_files,
    )
    .map_err(|error| {
        log::error!(
            "安装前回收旧 MOD 失败，已中止安装：仓库={}，原因={}",
            repo.id,
            error
        );
        crate::locale_tag::with_params("mods.recycleFailed", &[("reason", &error.to_string())])
    })?;
    if recycled.is_empty() {
        log::info!("安装前没有需要回收的同仓库旧包：仓库={}", repo.id);
    }
    let installation: Result<usize, String> = (|| {
        let pending = if let Some(plan) = &plan {
            archive::extract_prepared_mod_zip(asset_path, game_root, plan)?
        } else {
            archive::copy_asset_pending(asset_path, game_root, &repo.install_target, download_name)?
        };
        /*
         * 包名与备注都只存多语言快照，格式 `{仓库名} - {平台}`。
         *
         * 仓库的规范形式本来就是 `name_i18n` / `note_i18n`（源文档里
         * `name`/`note` 是可选兜底，`normalize_repository_i18n` 会把
         * zh-CN 回填到它们）。清单再存一份单语副本属于冗余双写，
         * 两者迟早不一致。所以这里直接把 i18n 交给 `new_manifest`。
         */
        let package_name_i18n = archive::repository_package_names(repo);
        let note_i18n = repo
            .note_i18n
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut manifest = archive::new_manifest(
            &repo.id,
            release_tag,
            package_name_i18n,
            note_i18n,
            pending.files().to_vec(),
            crate::mods::effective_relations(repo, Some(release_tag)),
        );
        manifest.repository_binding = ModRepositoryBinding::from_release(repo, release_tag);
        // 固化安装时的分组：之后即使源文档还没刷新出来（刚启动），
        // MOD 管理页也能按当时的分组稳定归类。
        manifest.group_i18n = repo.group_i18n.clone();
        // 预发布标记跟着版本号一起固化：它只存在于当时的 Release 元数据里
        manifest.prerelease = prerelease;
        archive::write_manifest(manifests_dir, &manifest)?;
        Ok(pending.commit().len())
    })();
    if let Err(error) = &installation {
        // 安装失败：把本次回收的所有同仓库旧包按逆序恢复。
        // 逆序是为了让「后回收的先恢复」，与原清单的提交顺序相反，避免中间态冲突。
        for entry in recycled.iter().rev() {
            if let Err(restore_error) =
                recycle_bin::restore(recycle_root, manifests_dir, game_root, &entry.id)
            {
                log::error!(
                    "安装失败且旧 MOD 包恢复失败，保留回收记录：仓库={}，记录={}，安装错误={}，恢复错误={}",
                    repo.id,
                    entry.id,
                    error,
                    restore_error
                );
                return Err("recycle.partialFailure".into());
            }
        }
    } else if !keep_recycled {
        /*
         * 安装成功且用户关闭了回收站：现在才清理暂存记录。
         * 待迁移的旧 DIYChef 模型沿用同一记录保留，不因回收站设置而永久丢失。
         * 放在成功分支里而不是回收时立即删，是因为上面那段回滚依赖这些记录；
         * 提前清掉会让「安装失败 → 旧包也找不回来」变成数据丢失。
         */
        for entry in &recycled {
            if let Err(error) = recycle_bin::purge_entry(recycle_root, &entry.id) {
                // 清理失败只影响回收站里多出一条记录，不影响本次安装结果
                log::warn!(
                    "覆盖安装成功后清理旧包暂存记录失败：仓库={}，记录={}，原因={}",
                    repo.id,
                    entry.id,
                    error
                );
            }
        }
    }
    installation.map_err(|error| {
        crate::locale_tag::with_params(
            "mods.installFailedRolledBack",
            &[("reason", &error.to_string())],
        )
    })
}

#[cfg(test)]
mod tests;
