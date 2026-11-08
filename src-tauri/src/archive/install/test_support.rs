use super::*;

pub(super) fn extract_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    target_dir: &str,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    log::info!(
        "开始解压 MOD ZIP：文件={}，目标={}",
        zip_path.display(),
        target_dir
    );
    let plan = prepare_repository_mod_zip(zip_path, game_root, target_dir, archive_name)?;
    extract_prepared_mod_zip(zip_path, game_root, &plan).map(PendingModInstall::commit)
}

pub(super) fn extract_local_mod_zip(
    zip_path: &Path,
    game_root: &Path,
    archive_name: &str,
) -> Result<Vec<String>, String> {
    extract_local_mod_zip_pending(zip_path, game_root, archive_name).map(PendingModInstall::commit)
}

pub(super) fn copy_asset(
    asset_path: &Path,
    game_root: &Path,
    install_target: &str,
    file_name: &str,
) -> Result<Vec<String>, String> {
    copy_asset_pending(asset_path, game_root, install_target, file_name)
        .map(PendingModInstall::commit)
}
