//! MOD 文件与清单的统一安装事务；预览、直链和仓库安装共用。
use crate::app_state::AppState;
use crate::models::*;
use crate::{archive, game, mods, recycle_bin};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) fn install_manual_mod(
    state: &AppState,
    source: &Path,
    expected_game_root: Option<&Path>,
) -> Result<OperationResult, String> {
    log::info!("开始安装手动 MOD");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if expected_game_root.is_some_and(|expected| expected != game_root) {
        return Err("game.directoryChanged".into());
    }
    if !source.is_file() {
        return Err("mods.fileMissing".into());
    }
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("mods.invalidFileName")?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    log::debug!(
        "本地 MOD 文件已确认：名称={}，类型={}",
        file_name,
        extension
    );
    let repository_id = format!("manual-{}", Uuid::new_v4());
    let (pending, display_name) = if extension.eq_ignore_ascii_case("dll") {
        log::debug!("按单 DLL 规则安装到 BepInEx/plugins：名称={}", file_name);
        /*
         * 显示名优先用 DLL 里的 BepInPlugin 名称 —— 用户认的是插件名，
         * 不是 `XXX.dll` 这样的文件名。读不出元数据时退回文件名去扩展名。
         * 仓库安装那边用仓库名，两条路径的提示体验保持一致。
         */
        let display =
            mods::plugin_name_from_file(&source).unwrap_or_else(|| mods::file_stem_name(file_name));
        let pending =
            archive::copy_asset_pending(&source, &game_root, "BepInEx/plugins", file_name)?;
        (pending, display)
    } else if extension.eq_ignore_ascii_case("zip") {
        log::debug!("按本地 ZIP 布局规则解压：名称={}", file_name);
        let folder = source
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("archive.invalidZipName")?;
        if folder.is_empty()
            || folder.contains('/')
            || folder.contains('\\')
            || folder.contains(':')
        {
            return Err("archive.invalidZipName".into());
        }
        let pending = archive::extract_local_mod_zip_pending(&source, &game_root, file_name)?;
        /*
         * 显示名：优先包内第一个 DLL 的 BepInPlugin 名称，退回 ZIP 文件名。
         * pending 的文件此时已落盘，可以直接读元数据。
         */
        let display = pending
            .files()
            .iter()
            .filter(|relative| relative.to_ascii_lowercase().ends_with(".dll"))
            .find_map(|relative| mods::plugin_name_from_file(&game_root.join(relative)))
            .unwrap_or_else(|| folder.to_string());
        (pending, display)
    } else {
        return Err("mods.unsupportedFileType".into());
    };
    if pending.files().is_empty() {
        log::warn!("本地 MOD 没有可安装内容：名称={}", file_name);
        return Err("archive.noInstallableContent".into());
    }
    let manifest = archive::new_manifest(
        &repository_id,
        // release_tag 是「Release 版本标签」，手动安装没有 Release，保持空字符串。
        // 之前这里塞了文件名，语义是错的，还会被更新判定相关的逻辑当成版本号尝试解析。
        "",
        // 手动安装没有仓库，包名与备注只有当前语言这一种写法
        archive::single_language(&display_name),
        archive::single_language(file_name),
        pending.files().to_vec(),
        Vec::new(),
    );
    if let Err(error) = archive::write_manifest(&state.config.manifests_dir(), &manifest) {
        log::error!(
            "本地 MOD 安装清单写入失败，已尝试回滚：名称={}，原因={}",
            display_name,
            error
        );
        return Err(error);
    }
    let files = pending.commit();
    log::info!(
        "本地 MOD 安装完成：名称={}，文件数={}",
        display_name,
        files.len()
    );
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "mods.installCompleted",
            &[("name", &display_name.to_string())],
        ),
    })
}

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

/// 从预览暂存安装一个仓库资产。
///
/// 预览阶段已经把文件下载并解析过了，这里只做最后一跳：按仓库身份回收
/// 该仓库的全部旧包，再解压/复制新文件。必须走这条路径而不是
/// `install_manual_mod` —— 后者每次生成新的 `manual-<uuid>` 清单，
/// 同仓库的旧包不会被回收，装几次就有几份副本。
pub(crate) fn install_staged_mod_repository(
    state: &AppState,
    staged_file: &Path,
    repository_id: &str,
    release_tag: &str,
    prerelease: bool,
) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?
        .clone();

    let download_name = staged_file
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("mods.invalidFileName")?;

    let manifests_dir = state.config.manifests_dir();
    // 沿用仓库安装的身份解析：源可能已从配置消失，但旧清单仍要能被找到
    let install_repo = mod_repository_install_identity(&repo, &config, &manifests_dir)?;

    let file_count = install_downloaded_mod_repository(
        staged_file,
        download_name,
        &game_root,
        &manifests_dir,
        &state.config.mod_recycle_bin_dir(),
        &install_repo,
        release_tag,
        prerelease,
        config.recycle_deleted_mods,
    )?;

    log::info!(
        "从预览安装仓库资产完成：仓库={}，版本={}，文件数={}",
        repo.name,
        release_tag,
        file_count
    );
    Ok(OperationResult {
        ok: true,
        message: if release_tag.is_empty() {
            crate::locale_tag::with_params(
                "mods.installCompletedSimple",
                &[("name", &repo.name.to_string())],
            )
        } else {
            crate::locale_tag::with_params(
                "mods.installCompletedWithVersion",
                &[
                    ("name", &repo.name.to_string()),
                    ("version", &release_tag.to_string()),
                ],
            )
        },
    })
}

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
        let package_name_i18n = repo
            .name_i18n
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| {
                (
                    key.clone(),
                    archive::package_display_name(value, &repo.provider),
                )
            })
            .collect::<BTreeMap<_, _>>();
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
         * 安装成功且用户关闭了回收站：旧包已确认不再需要，现在才清理暂存记录。
         * 放在成功分支里而不是回收时立即删，是因为上面那段回滚依赖这些记录；
         * 提前清掉会让「安装失败 → 旧包也找不回来」变成数据丢失。
         */
        for entry in &recycled {
            if let Err(error) = recycle_bin::delete(recycle_root, &entry.id) {
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
