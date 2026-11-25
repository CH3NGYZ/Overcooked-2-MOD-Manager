use crate::app_state::AppState;
use crate::app_update::UpdateTarget;
use crate::app_update::cache::app_update_cache_get;
use crate::app_update::cache::app_update_cache_store;
use crate::app_update::channels::InstallerChannel;
use crate::app_update::channels::InstallerRegion;
use crate::app_update::channels::emit_installer_channel_status;
use crate::app_update::channels::installer_channel_status;
use crate::app_update::channels::installer_manifest_sources;
use crate::app_update::channels::installer_region;
use crate::app_update::distribution;
use crate::app_update::download::InstallerDownloadAttempt;
use crate::app_update::download::discover_and_download_installer_update;
use crate::app_update::manifest::discover_installer_update;
use crate::app_update::receipt::{self, Installation};
use crate::app_update::recovery_links::{self, Destination};
use crate::app_update::staged::{DownloadStart, Recovery, StagedInstaller};
use crate::archive;
use crate::models::{
    AppConfig, AppDistribution, AppUpdateCheckResult, AppUpdateInfo, AppUpdateInstallationStatus,
    OperationResult,
};
use crate::network_policy::NetworkPolicy;
use crate::shell_open;
use tauri::AppHandle;
use tauri::State;

#[tauri::command]
pub fn get_app_distribution() -> Result<AppDistribution, String> {
    distribution::current()
}

#[tauri::command]
pub fn open_update_repository(state: State<'_, AppState>) -> Result<OperationResult, String> {
    if distribution::current()? == AppDistribution::MicrosoftStore {
        shell_open::open_url(distribution::STORE_UPDATES_URI)?;
        return Ok(OperationResult {
            ok: true,
            message: "update.microsoftStoreOpened".into(),
        });
    }
    log::info!("打开安装器更新仓库页面");
    let config = state.config.load_or_default();
    open_release_page(recovery_links::release_list_url(installer_region(
        &config.region,
    )))
}

#[tauri::command]
pub async fn get_pending_app_update_recovery(state: State<'_, AppState>) -> Result<bool, String> {
    if distribution::current()? != AppDistribution::Standalone {
        return Ok(false);
    }
    Ok(state.app_updates.recovery()?.is_some()
        || installation_status(UpdateTarget::current()?).await? == Some(Installation::Failed))
}

async fn installation_status(target: UpdateTarget) -> Result<Option<Installation>, String> {
    tauri::async_runtime::spawn_blocking(move || receipt::status(target))
        .await
        .map_err(|_| "update.checkInitFailed")?
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<AppUpdateCheckResult, String> {
    let force = force.unwrap_or(false);
    let config = state.config.load_or_default();
    let region = installer_region(&config.region);
    if distribution::current()? == AppDistribution::MicrosoftStore {
        log::info!("当前为 MSIX 包运行态，程序更新由 Microsoft Store 管理");
        return Ok(distribution::store_check(region.as_str()));
    }
    log::info!("开始检查安装器更新：跳过缓存={force}");
    if let Some(recovery) = state.app_updates.recovery()? {
        return Ok(AppUpdateCheckResult {
            distribution: AppDistribution::Standalone,
            update: Some(recovery.update),
            channels: Vec::new(),
            all_failed: false,
            region: region.as_str().into(),
            recovery_required: true,
            installation_status: None,
        });
    }
    let target = UpdateTarget::current()?;
    let installation = installation_status(target).await?;
    /*
     * 启动时恒检测，所以这个命令每次启动都会跑；5 分钟内重复调用直接返回
     * 上一次的成功结果。只缓存成功结果 —— 全部通道失败/超时的结论不能占住
     * 缓存，否则用户在这 5 分钟里连手动重试都会拿到同一份失败结论。
     * 手动检测传 force=true（与仓库刷新同一条规则）。
     */
    let cached = (!force && installation != Some(Installation::Failed))
        .then(|| app_update_cache_get(&state, region))
        .flatten();
    let mut result = if let Some(cached) = cached {
        log::info!("安装器更新检查命中 5 分钟缓存，跳过网络请求");
        cached
    } else {
        let result = run_update_check(&app, &config, region).await;
        if !result.all_failed {
            app_update_cache_store(&state, &result);
        }
        result
    };
    if !result.all_failed
        && let Some(update) = result.update.as_ref()
        && installation == Some(Installation::Failed)
    {
        let alternate_link = recovery_links::alternate_recovery_url(region, target, update).await;
        state.app_updates.require_recovery(Recovery {
            update: update.clone(),
            alternate_link,
        })?;
        result.recovery_required = true;
    }
    result.installation_status = match installation {
        Some(Installation::Applying) => Some(AppUpdateInstallationStatus::Applying),
        Some(Installation::RebootRequired) => Some(AppUpdateInstallationStatus::RebootRequired),
        Some(Installation::RestartFailed) => Some(AppUpdateInstallationStatus::RestartFailed),
        _ => None,
    };
    Ok(result)
}

async fn run_update_check(
    app: &AppHandle,
    config: &AppConfig,
    region: InstallerRegion,
) -> AppUpdateCheckResult {
    let policy = config
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(NetworkPolicy::active);
    let channels = match policy {
        Ok(policy) => installer_manifest_sources(&policy, region),
        Err(error) => {
            log::error!("安装器更新检查初始化失败: {error}");
            return failed_app_update_check(app, &[], region, "update.checkInitFailed");
        }
    };
    match discover_installer_update(config, app, region, &channels).await {
        Ok(result) => {
            log::info!(
                "安装器更新检查完成：发现更新={}，全部失败={}",
                result.update.is_some(),
                result.all_failed
            );
            result
        }
        Err(error) => {
            log::error!("安装器更新检查初始化失败: {error}");
            failed_app_update_check(app, &channels, region, "update.checkInitFailed")
        }
    }
}

/// 整次检查失败时的结果：每个通道都标成失败，前端照旧逐条显示原因。
fn failed_app_update_check(
    app: &AppHandle,
    channels: &[InstallerChannel],
    region: InstallerRegion,
    tag: &str,
) -> AppUpdateCheckResult {
    let channels = channels
        .iter()
        .map(|channel| {
            let status = installer_channel_status(channel, "failed", Some(tag.into()));
            emit_installer_channel_status(app, region, status.clone());
            status
        })
        .collect();
    AppUpdateCheckResult {
        update: None,
        channels,
        all_failed: true,
        region: region.as_str().into(),
        distribution: AppDistribution::Standalone,
        recovery_required: false,
        installation_status: None,
    }
}

#[tauri::command]
pub async fn stage_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
    on_update: tauri::ipc::Channel<archive::DownloadProgress>,
) -> Result<Option<AppUpdateInfo>, String> {
    let target = UpdateTarget::current()?;
    if !target.supports_background_install() {
        return Err("update.formatUnsupported".into());
    }
    if let Some(status) = installation_status(target).await? {
        return Err(match status {
            Installation::Failed => "update.installFailedManual",
            Installation::Applying => "update.inProgress",
            Installation::RebootRequired => "update.rebootRequired",
            Installation::RestartFailed => "update.restartFailed",
        }
        .into());
    }
    let guard = match state.app_updates.start_download()? {
        DownloadStart::Ready(info) => return Ok(Some(*info)),
        DownloadStart::Started(guard) => guard,
    };
    let attempt = archive::channel_download_progress(
        false,
        on_update,
        discover_and_download_installer_update(&state, &app, Some(&channel), || guard.cancelled()),
    )
    .await?;
    let (info, download) = match attempt {
        InstallerDownloadAttempt::Updated { info, download } => (info, download),
        InstallerDownloadAttempt::Latest => return Ok(None),
        InstallerDownloadAttempt::Failed(message) => return Err(message),
    };
    if guard.cancelled() {
        return Err("download.cancelled".into());
    }
    if info.installer_type != target.kind {
        return Err("update.manifestPlatformMismatch".into());
    }
    let staged =
        tauri::async_runtime::spawn_blocking(move || StagedInstaller::prepare(download, info))
            .await
            .map_err(|_| "update.stageFailed")??;
    guard.publish(staged).map(Some)
}

#[tauri::command]
pub async fn finish_app_update_recovery(
    app: AppHandle,
    state: State<'_, AppState>,
    destination: Destination,
    locale: String,
) -> Result<(), String> {
    distribution::require_standalone()?;
    let recovery = state
        .app_updates
        .recovery()?
        .ok_or("update.stagedUnavailable")?;
    let url = match destination {
        Destination::Website => recovery_links::website_recovery_url(&locale),
        Destination::Alternate => recovery.alternate_link,
    };
    super::system::request_user_exit(
        app,
        state.inner().clone(),
        super::system::ExitAction::OpenRecoveryPage(url),
    )
    .await
}

#[tauri::command]
pub fn open_release_page(url: String) -> Result<OperationResult, String> {
    log::info!("打开 Release 页面");
    shell_open::open_web_page(&url, shell_open::WebPageKind::Release)?;
    Ok(OperationResult {
        ok: true,
        message: "update.pageOpenedDetail".into(),
    })
}
