use crate::app_state::AppState;
use crate::app_update::cache::app_update_cache_get;
use crate::app_update::cache::app_update_cache_store;
use crate::app_update::channels::InstallerRegion;
use crate::app_update::channels::detect_installer_region;
use crate::app_update::channels::emit_installer_channel_status;
use crate::app_update::channels::emit_installer_phase;
use crate::app_update::channels::installer_channel_status;
use crate::app_update::channels::installer_manifest_sources;
use crate::app_update::channels::{
    INSTALLER_GITEE_OWNER, INSTALLER_GITEE_REPO, INSTALLER_GITHUB_OWNER, INSTALLER_GITHUB_REPO,
};
use crate::app_update::download::InstallerDownloadAttempt;
use crate::app_update::download::discover_and_download_installer_update;
use crate::app_update::manifest::discover_installer_update;
use crate::archive;
use crate::models::{AppUpdateCheckResult, GitHubProxyConfig, OperationResult};
use crate::shell_open;
use tauri::AppHandle;
use tauri::State;
use url::Url;

#[tauri::command]
pub fn open_update_repository() -> Result<OperationResult, String> {
    log::info!("打开安装器更新仓库页面");
    let url = match detect_installer_region() {
        InstallerRegion::MainlandChina => {
            format!("https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases")
        }
        InstallerRegion::International => {
            format!("https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases")
        }
    };
    open_release_page(url)
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    locale: Option<String>,
    force: Option<bool>,
) -> Result<AppUpdateCheckResult, String> {
    let _ = locale;
    let force = force.unwrap_or(false);
    let region = detect_installer_region();
    log::info!("开始检查安装器更新：跳过缓存={force}");
    /*
     * 启动时恒检测，所以这个命令每次启动都会跑；5 分钟内重复调用直接返回
     * 上一次的成功结果。只缓存成功结果 —— 全部通道失败/超时的结论不能占住
     * 缓存，否则用户在这 5 分钟里连手动重试都会拿到同一份失败结论。
     * 手动检测传 force=true（与仓库刷新同一条规则）。
     */
    if !force {
        if let Some(cached) = app_update_cache_get(&state, region) {
            log::info!("安装器更新检查命中 5 分钟缓存，跳过网络请求");
            return Ok(cached);
        }
    }
    let config = state.config.load_or_default();
    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(
            5 * installer_manifest_sources(&config.github_proxies, region).len() as u64 + 10,
        ),
        discover_installer_update(&config, &app),
    )
    .await
    {
        Ok(Ok(result)) => {
            log::info!(
                "安装器更新检查完成：发现更新={}，全部失败={}",
                result.update.is_some(),
                result.all_failed
            );
            result
        }
        Ok(Err(error)) => {
            log::error!("安装器更新检查初始化失败: {error}");
            failed_app_update_check(
                &app,
                &config.github_proxies,
                region,
                "update.checkInitFailed",
            )
        }
        Err(_) => {
            log::warn!("安装器更新检查超过总超时，已跳过自动更新");
            failed_app_update_check(&app, &config.github_proxies, region, "update.checkTimeout")
        }
    };
    if !result.all_failed {
        app_update_cache_store(&state, &result);
    }
    Ok(result)
}

/// 整次检查失败时的结果：每个通道都标成失败，前端照旧逐条显示原因。
fn failed_app_update_check(
    app: &AppHandle,
    proxies: &GitHubProxyConfig,
    region: InstallerRegion,
    tag: &str,
) -> AppUpdateCheckResult {
    let channels = installer_manifest_sources(proxies, region)
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
    }
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    let _update_guard = archive::begin_app_update()?;
    log::info!("开始安装器自更新");
    let attempt = discover_and_download_installer_update(&state, &app).await?;
    let (info, download) = match attempt {
        InstallerDownloadAttempt::Updated { info, download } => (info, download),
        InstallerDownloadAttempt::Latest => {
            log::info!("安装器已是最新版本");
            return Ok(OperationResult {
                ok: false,
                message: "update.alreadyLatest".into(),
            });
        }
        InstallerDownloadAttempt::Failed(message) => {
            log::warn!("安装器自更新未完成：{}", message);
            return Ok(OperationResult { ok: false, message });
        }
    };
    if archive::is_download_cancelled() {
        return Err("download.cancelled".into());
    }
    emit_installer_phase(
        &app,
        detect_installer_region(),
        "install",
        None,
        Some(info.clone()),
    );
    let version = info.version;
    log::info!(
        "开始通过 Tauri Updater 安装程序更新：版本={}，格式={}，已验签字节数={}",
        version,
        info.installer_type,
        download.bytes.len()
    );
    tauri::async_runtime::spawn_blocking(move || download.update.install(&download.bytes))
        .await
        .map_err(|_| "update.installFailed")?
        .map_err(|error| {
            log::error!("Tauri Updater 安装失败：{error}");
            "update.installFailedManual".to_string()
        })?;
    #[cfg(not(windows))]
    {
        log::info!("Tauri Updater 安装完成，正在重启：版本={version}");
        app.restart();
    }
    #[cfg(windows)]
    Ok(OperationResult {
        ok: true,
        // Windows normally exits inside Update::install after spawning.
        message: "update.installerStarted".into(),
    })
}

#[tauri::command]
pub fn open_release_page(url: String) -> Result<OperationResult, String> {
    log::info!("打开 Release 页面");
    let parsed = Url::parse(&url).map_err(|e| {
        crate::locale_tag::with_params("update.pageInvalid", &[("reason", &e.to_string())])
    })?;
    if parsed.scheme() != "https" {
        return Err("update.pageMustBeHttps".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("update.pageHasCredentials".into());
    }
    shell_open::open_url(parsed.as_str())?;
    Ok(OperationResult {
        ok: true,
        message: "update.pageOpenedDetail".into(),
    })
}
