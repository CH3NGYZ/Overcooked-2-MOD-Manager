//! Repository asset download and preview staging shared by plan preparation.
use crate::{
    app_state::AppState,
    archive, game,
    models::{ModDownloadMode, ProxyMode},
    releases,
};
use std::{fs, path::PathBuf};
use tauri::AppHandle;

/// 仓库安装的结果。
///
/// 仅返回下载后的预览；游戏文件只由确认后的链式事务提交。
#[derive(serde::Serialize)]
pub struct ModRepositoryInstallOutcome {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<crate::mod_preview::ModPreview>,
}

/// Existing transport and preview path, shared by single downloads and whole-chain preparation.
/// A batch resets cancellation once, never between its individual downloads.
pub(crate) async fn run_repository_install(
    app: AppHandle,
    state: AppState,
    repository_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    asset_proxy_mode: Option<ProxyMode>,
    asset_proxy_id: Option<String>,
    preview_only: Option<bool>,
    locale: Option<String>,
    reset_download: bool,
) -> Result<ModRepositoryInstallOutcome, String> {
    if preview_only != Some(true) {
        return crate::mod_changes::guard::require_shared_plan();
    }
    let snapshot = state.config.load_or_default();
    let policy = snapshot
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(|| crate::network_policy::NetworkPolicy::active())?;
    crate::network_policy::NetworkPolicy::in_snapshot(policy, async {
        // The UI can cancel while we prepare metadata. Reset once at command
        // entry, never after that wait or while switching download candidates.
        if reset_download {
            archive::start_download();
        }
        log::info!(
            "开始安装在线 MOD：仓库={}，指定版本={}，指定资产={}，代理模式={:?}",
            repository_id,
            tag.as_deref().unwrap_or("自动选择"),
            asset_name.as_deref().unwrap_or("自动匹配"),
            asset_proxy_mode
        );
        // Keep region, credentials and network policy from the same entry snapshot.
        let config = snapshot;
        let info = game::detect_game(&config);
        let game_root = PathBuf::from(info.path.ok_or("game.notFound")?);
        game::ensure_game_stopped(&game_root)?;
        if info.bepinex.version.is_none() {
            return Err("framework.bepInExRequiredBeforeMods".into());
        }
        let repo = config
            .repositories
            .iter()
            .find(|repo| repo.id == repository_id)
            .ok_or("source.repositoryMissing")?
            .clone();
        // 安装要求仓库在当前地区可用；隐藏/地区不匹配的仓库不能作为安装入口
        if !crate::config::mod_repository_is_available(&config, &repo, &config.region) {
            return Err("source.repositoryUnavailableForInstall".into());
        }
        /*
         * 用于下载进度标签与安装预览标题的显示名。
         *
         * 必须在下载开始前算好：进度事件在下载过程中持续发出，label 是其中
         * 一个字段；等下载完再改就晚了。跟随界面语言，而不是固定用中文原名。
         */
        let display_name = crate::config::localized_mod_repository_name(&repo, locale.as_deref());
        let (
            download_url,
            download_name,
            release_tag,
            release_body,
            release_prerelease,
            download_assets,
            is_raw_file,
        ) = match repo.download_mode {
            ModDownloadMode::RepositoryFile => {
                log::debug!(
                    "按仓库文件模式安装：仓库={}，分支={}，文件={}",
                    repo.name,
                    repo.branch,
                    repo.file_path
                );
                let download_url = releases::repository_file_url(
                    &repo.provider,
                    &repo.username,
                    &repo.repository,
                    &repo.branch,
                    &repo.file_path,
                )
                .map_err(|error| error.to_string())?;
                let download_name = releases::repository_file_name(&repo.file_path)
                    .map_err(|error| error.to_string())?;
                (
                    download_url,
                    download_name,
                    String::new(),
                    String::new(),
                    false,
                    Vec::new(),
                    true,
                )
            }
            ModDownloadMode::Release => {
                log::debug!(
                    "按 Release 模式安装：仓库={}，缓存 Release 数={}",
                    repo.name,
                    repo.cached_releases.len()
                );
                let release = releases::select_cached_release(
                    &repo.cached_releases,
                    tag.as_deref(),
                    repo.pinned_version.as_deref(),
                )
                .ok_or("source.noInstallableRelease")?;
                let asset = releases::select_install_asset(
                    &crate::releases::RepositoryRequest::from(&repo),
                    &repo.asset_rules,
                    release,
                    asset_name.as_deref(),
                )
                .ok_or("source.noMatchingAsset")?;
                log::info!(
                    "已选择 Release 资产：仓库={}，版本={}，资产={}",
                    repo.name,
                    release.tag_name,
                    asset.name
                );
                (
                    asset.download_url.clone(),
                    asset.name.clone(),
                    release.tag_name.clone(),
                    release.body.clone(),
                    releases::resolve_prerelease(
                        release.api_prerelease,
                        release.prerelease,
                        Some(&release.tag_name),
                    ),
                    archive::release_download::asset_set(asset, &release.assets)?,
                    false,
                )
            }
        };
        let client = releases::build_download_client(config.network.first_byte_timeout())
            .map_err(|e| e.to_string())?;
        let cache_dir = state.config.cache_dir();
        let assets = if is_raw_file {
            vec![crate::models::ReleaseAsset {
                name: download_name.clone(),
                size: 0,
                download_count: None,
                download_url,
                digest: None,
            }]
        } else {
            download_assets
        };
        let temp = archive::release_download::download(
            &client,
            assets,
            &cache_dir,
            &display_name,
            Some(&app),
            &config.network,
            |url| {
                crate::download::planning::plan_repository_download(
                    &repo.provider,
                    repo.kind,
                    &repo.network_plan,
                    &config.region,
                    url,
                    is_raw_file.then_some(crate::github_proxies::RawProxyContext {
                        branch: &repo.branch,
                        file_path: &repo.file_path,
                    }),
                    asset_proxy_mode,
                    asset_proxy_id.as_deref(),
                )
            },
        )
        .await
        .map_err(|error| {
            log::error!("在线 MOD 资产下载失败：仓库={}，原因={}", repo.name, error);
            error
        })?;
        log::info!(
            "仓库文件下载完成：仓库={}，文件名={}",
            repo.name,
            download_name
        );
        /*
         * 预览模式：只下载并解析，不写入游戏目录。
         *
         * 复用这里已经完成的下载（含代理与竞速），把临时文件移进预览暂存，
         * 后续仅由已确认链式计划消费暂存资产并统一提交。
         */
        let preview = crate::mod_preview::stage_downloaded_file(
            &state,
            &temp,
            &download_name,
            &repo.id,
            &display_name,
            &release_tag,
            &release_body,
            release_prerelease,
        )?;
        let _ = fs::remove_file(&temp);
        log::info!(
            "仓库资产已下载并解析（预览模式）：仓库={}，DLL 数={}",
            repo.name,
            preview.total_dlls
        );
        return Ok(ModRepositoryInstallOutcome {
            ok: true,
            message: crate::locale_tag::with_params(
                "mods.ready",
                &[("name", &repo.name.to_string())],
            ),
            preview: Some(preview),
        });
    })
    .await
}
