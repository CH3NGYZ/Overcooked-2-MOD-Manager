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
use crate::app_update::download::download_confirmed_update;
use crate::app_update::manifest::discover_installer_update;
use crate::app_update::receipt::{self, Installation, InstallationState};
use crate::app_update::recovery_links;
use crate::app_update::staged::{DownloadStart, StagedInstaller};
use crate::archive;
use crate::models::{
    AppConfig, AppDistribution, AppUpdateCheckResult, AppUpdateInfo, AppUpdateInstallMode,
    AppUpdateInstallationStatus, OperationResult,
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
pub fn open_update_repository(
    state: State<'_, AppState>,
    version: Option<String>,
) -> Result<OperationResult, String> {
    if distribution::current()? == AppDistribution::MicrosoftStore {
        shell_open::open_url(distribution::STORE_UPDATES_URI)?;
        return Ok(OperationResult {
            ok: true,
            message: "update.microsoftStoreOpened".into(),
        });
    }
    if state.app_updates.offer()?.is_some() {
        return Err("update.requiredToContinue".into());
    }
    log::info!("打开安装器更新仓库页面");
    let config = state.config.load_or_default();
    let url = version
        .as_deref()
        .map(recovery_links::official_release_url)
        .unwrap_or_else(|| recovery_links::release_list_url(installer_region(&config.region)));
    open_release_page(url)
}

async fn installation_status(target: UpdateTarget) -> Result<Option<InstallationState>, String> {
    tauri::async_runtime::spawn_blocking(move || receipt::status(target))
        .await
        .map_err(|_| "update.checkInitFailed".into())
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
    let target = UpdateTarget::current()?;
    /*
     * 启动时恒检测，所以这个命令每次启动都会跑；5 分钟内重复调用直接返回
     * 上一次的成功结果。只缓存成功结果 —— 全部通道失败/超时的结论不能占住
     * 缓存，否则用户在这 5 分钟里连手动重试都会拿到同一份失败结论。
     * 手动检测传 force=true（与仓库刷新同一条规则）。
     */
    let cached = (!force)
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
    let installation = installation_status(target).await?;
    let receipt_offer = installation
        .as_ref()
        .and_then(|receipt| receipt_offer(receipt, &app.package_info().version, target));
    let installation = installation.map(|installation| installation.status);
    let update = (!result.all_failed).then(|| result.update.take()).flatten();
    let mode = if update.is_some() || state.app_updates.offer()?.is_some() {
        current_install_mode(target).await?
    } else {
        AppUpdateInstallMode::Manual
    };
    result.update = state
        .app_updates
        .confirm_offer(update, receipt_offer, mode)?;
    result.recovery_required =
        result.update.is_some() && installation == Some(Installation::Failed);
    result.installation_status = match installation {
        Some(Installation::Applying) => Some(AppUpdateInstallationStatus::Applying),
        Some(Installation::RebootRequired) => Some(AppUpdateInstallationStatus::RebootRequired),
        Some(Installation::RestartFailed) => Some(AppUpdateInstallationStatus::RestartFailed),
        _ => None,
    };
    Ok(result)
}

fn receipt_offer(
    installation: &InstallationState,
    current: &semver::Version,
    target: UpdateTarget,
) -> Option<AppUpdateInfo> {
    let version = installation
        .version
        .as_ref()
        .filter(|version| version.cmp_precedence(current).is_gt())?;
    Some(AppUpdateInfo {
        version: version.to_string(),
        current_version: current.to_string(),
        body: None,
        changelog: None,
        date: None,
        download_size: None,
        manifest_url: String::new(),
        installer_url: None,
        installer_signature: String::new(),
        installer_name: String::new(),
        installer_type: target.kind.into(),
        provider: None,
        source_channel: None,
        install_mode: AppUpdateInstallMode::Manual,
    })
}

async fn current_install_mode(target: UpdateTarget) -> Result<AppUpdateInstallMode, String> {
    tauri::async_runtime::spawn_blocking(move || target.install_mode())
        .await
        .map_err(|_| "update.checkInitFailed".into())
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
) -> Result<AppUpdateInfo, String> {
    let target = UpdateTarget::current()?;
    if current_install_mode(target).await? != AppUpdateInstallMode::Deferred {
        return Err("update.formatUnsupported".into());
    }
    let info = state
        .app_updates
        .offer()?
        .filter(|offer| offer.source_channel.as_deref() == Some(channel.as_str()))
        .ok_or("update.stagedUnavailable")?;
    if let Some(status) = installation_status(target).await? {
        return Err(status.status.update_block_reason().into());
    }
    let guard = match state.app_updates.start_download(&info.version)? {
        DownloadStart::Ready(info) => return Ok(*info),
        DownloadStart::Started(guard) => guard,
    };
    let (info, download) = archive::channel_download_progress(
        false,
        on_update,
        download_confirmed_update(&state, &app, info, || guard.cancelled()),
    )
    .await?;
    if guard.cancelled() {
        return Err("download.cancelled".into());
    }
    if info.installer_type != target.kind {
        return Err("update.manifestPlatformMismatch".into());
    }
    let staged = tauri::async_runtime::spawn_blocking(move || {
        let staged = StagedInstaller::prepare(download, info)?;
        if target.install_mode() != AppUpdateInstallMode::Deferred {
            return Err("update.formatUnsupported".into());
        }
        Ok::<_, String>(staged)
    })
    .await
    .map_err(|_| "update.stageFailed")??;
    guard.publish(staged)
}

#[tauri::command]
pub async fn defer_staged_app_update(
    state: State<'_, AppState>,
    version: String,
) -> Result<(), String> {
    let target = UpdateTarget::current()?;
    if current_install_mode(target).await? != AppUpdateInstallMode::Deferred {
        return Err("update.formatUnsupported".into());
    }
    if let Some(status) = installation_status(target).await? {
        return Err(status.status.update_block_reason().into());
    }
    state.app_updates.defer(&version)
}

#[tauri::command]
pub async fn restart_update_as_user(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        super::system::request_user_exit(
            app,
            state.inner().clone(),
            super::system::ExitAction::RestartAsUser,
        )
        .await
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state);
        Err("update.restartAsUserUnavailable".into())
    }
}

#[tauri::command]
pub async fn open_app_update_release_and_exit(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
    destination: Option<UpdateDestination>,
) -> Result<(), String> {
    let target = UpdateTarget::current()?;
    let offer = state
        .app_updates
        .offer()?
        .filter(|offer| offer.version == version)
        .ok_or("update.stagedUnavailable")?;
    let url = match destination.unwrap_or_default() {
        UpdateDestination::Release => {
            let config = state.config.load_or_default();
            recovery_links::installer_release_url(installer_region(&config.region), target, &offer)
                .await
        }
        UpdateDestination::Website => recovery_links::OFFICIAL_WEBSITE.into(),
    };
    super::system::request_user_exit(
        app,
        state.inner().clone(),
        super::system::ExitAction::OpenRecoveryPage { version, url },
    )
    .await
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateDestination {
    #[default]
    Release,
    Website,
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

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn failed_receipt_without_a_manifest_requires_a_manual_update() {
        let target = UpdateTarget::from_parts("windows", "x86_64", Some("exe")).unwrap();
        let current = Version::new(99, 0, 0);
        let receipt = InstallationState {
            status: Installation::Failed,
            version: Some(Version::new(99, 1, 0)),
        };
        let updates = crate::app_update::staged::AppUpdates::default();
        let offer = updates
            .confirm_offer(
                None,
                receipt_offer(&receipt, &current, target),
                AppUpdateInstallMode::Manual,
            )
            .unwrap()
            .unwrap();
        assert_eq!(offer.version, "99.1.0");
        assert_eq!(offer.installer_type, "exe");
        assert!(offer.source_channel.is_none());
        assert!(!offer.has_signed_package());
        assert_eq!(offer.install_mode, AppUpdateInstallMode::Manual);
        assert!(recovery_links::official_release_url(&offer.version).ends_with("/tag/v99.1.0"));
        assert!(receipt_offer(&receipt, &Version::new(99, 1, 0), target).is_none());
        assert!(receipt_offer(&receipt, &Version::new(99, 2, 0), target).is_none());
    }

    #[test]
    fn an_unbound_status_does_not_invent_an_update_version() {
        let target = UpdateTarget::from_parts("windows", "x86_64", Some("exe")).unwrap();
        let current = Version::new(99, 0, 0);
        for status in [Installation::Failed, Installation::Applying] {
            let receipt = InstallationState {
                status,
                version: None,
            };
            assert!(receipt_offer(&receipt, &current, target).is_none());
        }
    }
}
