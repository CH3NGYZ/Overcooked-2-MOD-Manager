use crate::app_state::AppState;
use crate::archive;
use crate::download::DownloadContext;
use crate::download::planning::asset_proxy_urls;
use crate::download::planning::plan_asset_download_candidates;
use crate::download::planning::provider_proxy_entries;
use crate::download::planning::selected_asset_proxy_mode;
use crate::download::try_download_candidates as try_asset_download_candidates;
use crate::game;
use crate::models::{OperationResult, Provider, ProxyMode, RepositoryDownloadMode};
use crate::mods::install::install_downloaded_repository;
use crate::mods::install::repository_install_identity;
use crate::recycle_bin;
use crate::releases;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::State;

/// 仓库安装的结果。
///
/// `preview_only` 为真时只下载并解析，不写入游戏目录，`preview` 字段
/// 携带可供安装弹窗展示的解析结果；正常安装时 `preview` 为空。
#[derive(serde::Serialize)]
pub struct RepositoryInstallOutcome {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<crate::mod_preview::ModPreview>,
}

#[tauri::command]
pub async fn install_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    repository_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    asset_proxy_mode: Option<ProxyMode>,
    asset_proxy_id: Option<String>,
    preview_only: Option<bool>,
    // 界面语言（`zh-CN` / `en` / …）。用来在下载进度标签与安装预览标题里
    // 显示仓库的本地化名称；缺省时回退到仓库中文名。
    locale: Option<String>,
) -> Result<RepositoryInstallOutcome, String> {
    // The UI can cancel while we prepare metadata. Reset once at command
    // entry, never after that wait or while switching download candidates.
    archive::start_download();
    log::info!(
        "开始安装在线 MOD：仓库={}，指定版本={}，指定资产={}，代理模式={:?}",
        repository_id,
        tag.as_deref().unwrap_or("自动选择"),
        asset_name.as_deref().unwrap_or("自动匹配"),
        asset_proxy_mode
    );
    let config = state.config.load_or_default();
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
    if !crate::config::repository_is_available(&config, &repo, crate::config::current_user_region())
    {
        return Err("source.repositoryUnavailableForInstall".into());
    }
    /*
     * 用于下载进度标签与安装预览标题的显示名。
     *
     * 必须在下载开始前算好：进度事件在下载过程中持续发出，label 是其中
     * 一个字段；等下载完再改就晚了。跟随界面语言，而不是固定用中文原名。
     */
    let display_name = crate::config::localized_repository_name(&repo, locale.as_deref());
    let selected_proxy_mode = selected_asset_proxy_mode(
        asset_proxy_mode,
        &repo.provider,
        crate::config::current_user_region(),
    );
    let (
        download_url,
        download_name,
        release_tag,
        release_body,
        release_prerelease,
        digest,
        is_raw_file,
        expected_size,
    ) = match repo.download_mode {
        RepositoryDownloadMode::RepositoryFile => {
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
                None,
                true,
                None,
            )
        }
        RepositoryDownloadMode::Release => {
            log::debug!(
                "按 Release 模式安装：仓库={}，缓存 Release 数={}",
                repo.name,
                repo.cached_releases.len()
            );
            let fetched_releases = if repo.cached_releases.is_empty() {
                let client = releases::build_client(config.network.timeout_seconds)
                    .map_err(|e| e.to_string())?;
                let token = state.config.repository_api_token(&config, &repo);
                let api_proxies = crate::providers::api_proxies(&config, &repo.provider);
                releases::filter_channel(
                    releases::fetch_releases(
                        &client,
                        &repo,
                        token.as_deref(),
                        config.auth.official_api_only,
                        api_proxies,
                    )
                    .await
                    .map_err(|e| e.to_string())?,
                    &repo.channel,
                )
            } else {
                repo.cached_releases.clone()
            };
            let release = tag
                .as_deref()
                .and_then(|value| {
                    fetched_releases
                        .iter()
                        .find(|release| release.tag_name == value)
                })
                .or_else(|| {
                    repo.pinned_version.as_deref().and_then(|value| {
                        fetched_releases.iter().find(|release| {
                            release.tag_name == value || release.version.as_deref() == Some(value)
                        })
                    })
                })
                .or_else(|| fetched_releases.first())
                .ok_or("source.noInstallableRelease")?;
            let asset = releases::select_install_asset(&repo, release, asset_name.as_deref())
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
                release.prerelease,
                asset.digest.clone(),
                false,
                (asset.size > 0).then_some(asset.size),
            )
        }
    };
    let proxy_entries = provider_proxy_entries(&config, &repo.provider, is_raw_file);
    let proxy_urls = asset_proxy_urls(
        proxy_entries,
        &download_url,
        is_raw_file,
        &repo.branch,
        &repo.file_path,
        selected_proxy_mode,
        asset_proxy_id.as_deref(),
    );
    let candidates = plan_asset_download_candidates(selected_proxy_mode, &download_url, proxy_urls);
    let candidates = candidates.map_err(|error| {
        log::warn!("指定代理模式但没有可用的 GitHub 代理：仓库={}", repo.name);
        error
    })?;
    log::debug!(
        "下载地址准备完成：仓库={}，模式={}，下载候选数={}，原始文件={}",
        repo.name,
        if is_raw_file { "raw" } else { "release" },
        candidates.len(),
        is_raw_file
    );
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let download_threads = match &repo.provider {
        Provider::Github => repo.download_threads.clamp(1, 32),
        Provider::Gitee | Provider::Gitcode => 1,
    };
    log::info!(
        "开始下载仓库文件：仓库={}，平台={:?}，线程数={}，代理策略={:?}",
        repo.name,
        repo.provider,
        download_threads,
        selected_proxy_mode
    );
    let cache_dir = state.config.cache_dir();
    let racing = candidates.len() > 1;
    let temp = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            let label = &repo.name;
            let digest = if repo.provider == Provider::Github {
                digest.as_deref()
            } else {
                None
            };
            let is_zip = download_name.to_ascii_lowercase().ends_with(".zip");
            async move {
                let temp = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    label,
                    &reporter,
                    if racing { 1 } else { download_threads },
                    Some(archive::FIRST_BYTE_TIMEOUT),
                )
                .await?;
                let validation = archive::validate_download_file(&temp, expected_size, digest)
                    .and_then(|_| {
                        if is_zip {
                            archive::validate_zip(&temp)
                        } else {
                            Ok(())
                        }
                    });
                if let Err(error) = validation {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                Ok(temp)
            }
        },
        Some(DownloadContext {
            app: Some(&app),
            // 下载进度里显示的名字同样跟随界面语言
            label: &display_name,
            expected_size,
        }),
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
     * 之后由 confirm_mod_install 正式安装。这样仓库卡片的「安装 / 重装 /
     * 更新」与手动安装、在线直链看到的是同一个预览弹窗。
     */
    if preview_only.unwrap_or(false) {
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
        return Ok(RepositoryInstallOutcome {
            ok: true,
            message: crate::locale_tag::with_params(
                "mods.ready",
                &[("name", &repo.name.to_string())],
            ),
            preview: Some(preview),
        });
    }
    // No network awaits while the operation lock protects recycling and install.
    let files_result = (|| {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        if game::is_game_running(&game_root) {
            log::warn!("下载期间检测到厨房重新启动，取消安装：仓库={}", repo.name);
            return Err("mods.gameRestartedDuringDownload".into());
        }
        let current_config = state.config.load_or_default();
        let install_repo =
            repository_install_identity(&repo, &current_config, &state.config.manifests_dir())?;
        install_downloaded_repository(
            &temp,
            &download_name,
            &game_root,
            &state.config.manifests_dir(),
            &state.config.mod_recycle_bin_dir(),
            &install_repo,
            &release_tag,
            release_prerelease,
            current_config.recycle_deleted_mods,
        )
    })();
    let _ = fs::remove_file(&temp);
    match files_result {
        Ok(file_count) => {
            log::info!(
                "在线 MOD 安装完成：仓库={}，版本={}，文件数={}",
                repo.name,
                release_tag,
                file_count
            );
            Ok(RepositoryInstallOutcome {
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
                preview: None,
            })
        }
        Err(error) => {
            log::error!("在线 MOD 安装失败：仓库={}，原因={}", repo.name, error);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn uninstall_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<OperationResult, String> {
    // 回收整包要解析安装清单并移动文件：重活，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_repository_blocking(&state, &repository_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_repository`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_repository_blocking(
    state: &AppState,
    repository_id: &str,
) -> Result<OperationResult, String> {
    log::info!("开始卸载在线仓库 MOD：仓库={}", repository_id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let package_id = if let Some(repo) = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
    {
        repository_install_identity(repo, &config, &state.config.manifests_dir())?.id
    } else {
        repository_id.to_string()
    };
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("mods.unknownFiles")?;
    if config.recycle_deleted_mods {
        let entry = recycle_bin::recycle_package(
            &state.config.mod_recycle_bin_dir(),
            &state.config.manifests_dir(),
            &game_root,
            &manifest.repository_id,
            None,
        )?;
        log::info!(
            "在线仓库 MOD 卸载完成（已回收）：仓库={}，文件数={}",
            repository_id,
            entry.file_count
        );
    } else {
        recycle_bin::purge_package(
            &state.config.mod_recycle_bin_dir(),
            &state.config.manifests_dir(),
            &game_root,
            &manifest.repository_id,
            None,
        )?;
        log::info!(
            "在线仓库 MOD 卸载完成（已直接删除）：仓库={}",
            repository_id
        );
    }
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.packageMovedToRecycleBin".into()
        } else {
            "message.mods.packageDeleted".into()
        },
    })
}
