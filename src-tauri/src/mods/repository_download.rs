//! Repository asset downloads shared by whole-chain preparation and the legacy preview IPC.
use crate::{
    app_state::AppState,
    archive::{self, release_download::DownloadedRelease},
    game,
    models::{ModDownloadMode, ProxyMode, ReleaseAsset},
    releases,
};
use std::path::PathBuf;

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

#[derive(Default)]
pub(crate) struct RepositoryDownloadRequest<'a> {
    pub(crate) repository_id: &'a str,
    pub(crate) tag: Option<&'a str>,
    pub(crate) asset_name: Option<&'a str>,
    pub(crate) asset_proxy_mode: Option<ProxyMode>,
    pub(crate) asset_proxy_id: Option<&'a str>,
    pub(crate) locale: Option<&'a str>,
}

pub(crate) struct RepositoryAssetDownload {
    pub(crate) file: DownloadedRelease,
    pub(crate) file_name: String,
    pub(crate) repository_id: String,
    pub(crate) release_tag: String,
    pub(crate) prerelease: bool,
    repository_name: String,
    display_name: String,
    release_body: String,
}

pub(crate) async fn run_repository_install(
    state: AppState,
    request: RepositoryDownloadRequest<'_>,
    preview_only: Option<bool>,
) -> Result<ModRepositoryInstallOutcome, String> {
    if preview_only != Some(true) {
        return crate::mod_changes::guard::require_shared_plan();
    }
    archive::start_download();
    let download = download_repository_asset(&state, request).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let preview = crate::mod_preview::stage_downloaded_file(
            &state,
            download.file,
            &download.file_name,
            &download.repository_id,
            &download.display_name,
            &download.release_tag,
            &download.release_body,
            download.prerelease,
        )?;
        let name = &download.repository_name;
        log::info!(
            "仓库资产已下载并解析（预览模式）：仓库={name}，DLL 数={}",
            preview.total_dlls
        );
        Ok(ModRepositoryInstallOutcome {
            ok: true,
            message: crate::locale_tag::with_params("mods.ready", &[("name", name)]),
            preview: Some(preview),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Select from the current repository snapshot and return the owned artifact.
/// The caller controls cancellation reset; this never creates a display preview.
pub(crate) async fn download_repository_asset(
    state: &AppState,
    request: RepositoryDownloadRequest<'_>,
) -> Result<RepositoryAssetDownload, String> {
    let RepositoryDownloadRequest {
        repository_id,
        tag,
        asset_name,
        asset_proxy_mode,
        asset_proxy_id,
        locale,
    } = request;
    let snapshot = state.config.load_or_default();
    let policy = snapshot
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(|| crate::network_policy::NetworkPolicy::active())?;
    crate::network_policy::NetworkPolicy::in_snapshot(policy, async {
        log::info!(
            "开始安装在线 MOD：仓库={}，指定版本={}，指定资产={}，代理模式={:?}",
            repository_id,
            tag.unwrap_or("自动选择"),
            asset_name.unwrap_or("自动匹配"),
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
            .ok_or("source.repositoryMissing")?;
        // 安装要求仓库在当前地区可用；隐藏/地区不匹配的仓库不能作为安装入口
        if !crate::config::mod_repository_is_available(&config, repo, &config.region) {
            return Err("source.repositoryUnavailableForInstall".into());
        }
        /*
         * 用于下载进度标签与安装预览标题的显示名。
         *
         * 必须在下载开始前算好：进度事件在下载过程中持续发出，label 是其中
         * 一个字段；等下载完再改就晚了。跟随界面语言，而不是固定用中文原名。
         */
        let display_name = crate::config::localized_mod_repository_name(repo, locale);
        let is_raw_file = repo.download_mode == ModDownloadMode::RepositoryFile;
        let (assets, release_tag, release_body, prerelease) = match repo.download_mode {
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
                    vec![ReleaseAsset {
                        name: download_name,
                        size: 0,
                        download_count: None,
                        download_url,
                        digest: None,
                    }],
                    "",
                    "",
                    false,
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
                    tag,
                    repo.pinned_version.as_deref(),
                )
                .ok_or("source.noInstallableRelease")?;
                let asset = releases::select_install_asset(
                    &releases::RepositoryRequest::from(repo),
                    &repo.asset_rules,
                    release,
                    asset_name,
                )
                .ok_or("source.noMatchingAsset")?;
                log::info!(
                    "已选择 Release 资产：仓库={}，版本={}，资产={}",
                    repo.name,
                    release.tag_name,
                    asset.name
                );
                (
                    archive::release_download::asset_set(asset, &release.assets)?,
                    release.tag_name.as_str(),
                    release.body.as_str(),
                    releases::resolve_prerelease(
                        release.api_prerelease,
                        release.prerelease,
                        Some(&release.tag_name),
                    ),
                )
            }
        };
        // For split ZIPs the selected main asset is the last volume.
        let file_name = assets.last().ok_or("source.noMatchingAsset")?.name.clone();
        let client = releases::build_download_client(config.network.first_byte_timeout())
            .map_err(|e| e.to_string())?;
        let cache_dir = state.config.cache_dir();
        let cache_scope = (!is_raw_file).then(|| {
            archive::release_download::ReleaseCacheScope::for_mod(
                repo.provider.clone(),
                &repo.username,
                &repo.repository,
                release_tag,
            )
        });
        let file = archive::release_download::download(
            &client,
            assets,
            &cache_dir,
            cache_scope.as_ref(),
            &display_name,
            Some(&state.download_history),
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
                    asset_proxy_id,
                )
            },
        )
        .await
        .map_err(|error| {
            log::error!("在线 MOD 资产下载失败：仓库={}，原因={}", repo.name, error);
            error
        })?;
        log::info!("仓库文件下载完成：仓库={}，文件名={}", repo.name, file_name);
        Ok(RepositoryAssetDownload {
            file,
            file_name,
            repository_id: repo.id.clone(),
            repository_name: repo.name.clone(),
            display_name,
            release_tag: release_tag.into(),
            release_body: release_body.into(),
            prerelease,
        })
    })
    .await
}
