use crate::app_state::AppState;
use crate::app_update::channels::InstallerChannel;
use crate::app_update::channels::emit_installer_channel_status;
use crate::app_update::channels::emit_installer_phase;
use crate::app_update::channels::installer_channel_status;
use crate::app_update::channels::installer_manifest_sources;
use crate::app_update::channels::installer_sources_for_url;
use crate::app_update::channels::{InstallerRegion, installer_region};
use crate::app_update::manifest::InstallerRouteResult;
use crate::app_update::manifest::race_installer_channels;
use crate::app_update::manifest::run_installer_manifest_routes;
use crate::archive;
use crate::archive::DownloadRoute as AssetDownloadRoute;
use crate::download::DownloadContext;
use crate::download::history::RaceSession;
use crate::models::{AppUpdateInfo, AppUpdateInstallMode};
use crate::network_policy::NetworkPolicy;
use crate::releases;
use futures_util::StreamExt;
use tauri::AppHandle;
use url::Url;

impl crate::download::DownloadOutput for tauri_plugin_updater::Update {
    fn discard(&self) {}
}

async fn wait_for_cancellation(is_cancelled: impl Fn() -> bool) {
    while !is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

pub(super) async fn download_installer(
    client: &reqwest::Client,
    url: &str,
    reporter: &archive::DownloadReporter,
    label: &str,
    network: &crate::models::NetworkConfig,
    is_cancelled: impl Fn() -> bool,
) -> Result<Vec<u8>, String> {
    tokio::select! {
        biased;
        _ = wait_for_cancellation(is_cancelled) => Err("download.cancelled".into()),
        result = read_installer(client, url, reporter, label, network) => result,
    }
}

async fn read_installer(
    client: &reqwest::Client,
    url: &str,
    reporter: &archive::DownloadReporter,
    label: &str,
    network: &crate::models::NetworkConfig,
) -> Result<Vec<u8>, String> {
    log::debug!("开始下载安装器文件：标签={}", label);
    let deadline = Some(tokio::time::Instant::now() + network.first_byte_timeout());
    let response = archive::send_before_first_byte_deadline(client.get(url), deadline)
        .await?
        .error_for_status()
        .map_err(|error| error.without_url().to_string())?;
    let total = response.content_length();
    let started = std::time::Instant::now();
    let mut downloaded = 0_u64;
    let mut bytes = Vec::with_capacity(total.unwrap_or_default().min(64 * 1024 * 1024) as usize);
    let mut stream = response.bytes_stream();
    let first_chunk = archive::first_nonempty_chunk(&mut stream, deadline).await?;
    let mut next_chunk = first_chunk.map(Ok::<_, reqwest::Error>);
    while let Some(chunk) = next_chunk {
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        reporter.received(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        archive::emit_progress(reporter, label, downloaded, total, started.elapsed(), false);
        next_chunk = stream.next().await;
    }
    if downloaded == 0 {
        return Err("download.empty".into());
    }
    if total.is_some_and(|expected| expected != downloaded) {
        return Err("download.incomplete".into());
    }
    archive::emit_progress(reporter, label, downloaded, total, started.elapsed(), true);
    log::info!("下载安装器文件完成：标签={}，字节数={}", label, downloaded);
    Ok(bytes)
}

pub(crate) async fn download_verified_installer(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    channels: &[InstallerChannel],
    app: Option<&AppHandle>,
    region: InstallerRegion,
    verify: impl Fn(&[u8]) -> Result<(), String> + Send + Sync + 'static,
    network: &crate::models::NetworkConfig,
    is_cancelled: impl Copy + Fn() -> bool,
    race_session: Option<&RaceSession>,
) -> Result<Vec<u8>, String> {
    let proxy_count = channels
        .iter()
        .filter(|candidate| candidate.route == AssetDownloadRoute::Proxy)
        .count();
    log::info!(
        "安装器下载候选准备完成：平台={:?}，直连={}，Release代理={}，总候选={}",
        info.provider,
        channels.len() - proxy_count,
        proxy_count,
        channels.len()
    );
    let verify = std::sync::Arc::new(verify);
    race_installer_channels(
        channels,
        is_cancelled,
        |channel, reporter| {
            let verify = verify.clone();
            async move {
                if let Some(app) = app {
                    emit_installer_channel_status(
                        app,
                        region,
                        installer_channel_status(&channel, "downloading", None),
                    );
                }
                let result = download_installer(
                    client,
                    &channel.endpoint,
                    &reporter,
                    "update.downloadingInstaller",
                    network,
                    is_cancelled,
                )
                .await;
                let result = match result {
                    Ok(bytes) if !is_cancelled() => {
                        reporter.verifying();
                        let size = info.download_size.filter(|size| *size > 0);
                        match tauri::async_runtime::spawn_blocking(move || {
                            archive::validate_download_bytes(&bytes, size, None)?;
                            verify(&bytes)?;
                            Ok(bytes)
                        })
                        .await
                        {
                            _ if is_cancelled() => Err("download.cancelled".into()),
                            Ok(result) => result,
                            Err(_) => Err("update.stageFailed".into()),
                        }
                    }
                    Ok(_) => Err("download.cancelled".into()),
                    Err(error) => Err(error),
                };
                if let Some(app) = app {
                    emit_installer_channel_status(
                        app,
                        region,
                        installer_channel_status(
                            &channel,
                            match &result {
                                Ok(_) => "downloaded",
                                Err(error) if error == "download.cancelled" => "cancelled",
                                Err(_) => "failed",
                            },
                            result.as_ref().err().cloned(),
                        ),
                    );
                }
                result
            }
        },
        network,
        Some(DownloadContext {
            execution: None,
            label: "update.downloadingInstaller",
            file_name: Some(&info.installer_name),
            expected_size: info.download_size,
            race_session,
        }),
    )
    .await
}

async fn resolve_app_update(
    app: &AppHandle,
    info: &AppUpdateInfo,
    channels: &[InstallerChannel],
    network: &crate::models::NetworkConfig,
    is_cancelled: impl Copy + Fn() -> bool,
) -> Result<tauri_plugin_updater::Update, String> {
    // Package hosting can differ from the manifest mirror. Re-read the selected
    // manifest's provider, then require identical version, URL and signature.
    let provider = channels
        .iter()
        .find(|channel| {
            channel.endpoint == info.manifest_url
                && Some(channel.id.as_str()) == info.source_channel.as_deref()
        })
        .map(|channel| &channel.provider)
        .ok_or("update.manifestUrlInvalid")?;
    let group: Vec<_> = channels
        .iter()
        .filter(|channel| &channel.provider == provider)
        .cloned()
        .collect();
    race_installer_channels(
        &group,
        is_cancelled,
        |channel, _| async move {
            resolve_app_update_at(app, info, &channel.endpoint, network, is_cancelled).await
        },
        network,
        None,
    )
    .await
}

async fn resolve_app_update_at(
    app: &AppHandle,
    info: &AppUpdateInfo,
    url: &str,
    network: &crate::models::NetworkConfig,
    is_cancelled: impl Fn() -> bool,
) -> Result<tauri_plugin_updater::Update, String> {
    use tauri_plugin_updater::UpdaterExt;
    let target = crate::app_update::UpdateTarget::current()?;
    if target.kind != info.installer_type {
        return Err("update.manifestPlatformMismatch".into());
    }
    if is_cancelled() {
        return Err("download.cancelled".into());
    }
    let endpoint = Url::parse(url).map_err(|_| "update.manifestUrlInvalid")?;
    let address = endpoint.origin().ascii_serialization();
    let started = std::time::Instant::now();
    let builder = app
        .updater_builder()
        .target(target.manifest_key())
        .timeout(network.api_timeout())
        .header("Cache-Control", "no-cache, no-store")
        .map_err(|e| e.to_string())?
        .header("Pragma", "no-cache")
        .map_err(|e| e.to_string())?;
    let updater = match builder
        .endpoints(vec![endpoint])
        .and_then(|builder| builder.build())
    {
        Ok(updater) => updater,
        Err(error) => {
            log::warn!(
                "更新清单入口不可用：来源={address:?}，原因={}",
                super::updater_error_details(&error)
            );
            return Err(error.to_string());
        }
    };
    let result = tokio::select! {
        biased;
        _ = wait_for_cancellation(is_cancelled) => return Err("download.cancelled".into()),
        result = updater.check() => result,
    };
    match result {
        Ok(Some(update)) => {
            match crate::app_update::validate_update_metadata(
                &update.version,
                &update.download_url,
                &update.signature,
                info,
            ) {
                Ok(()) => return Ok(update),
                Err(error) => return Err(error),
            }
        }
        Ok(None) => log::warn!("更新清单没有所选 Release 的更新版本：来源={address:?}"),
        Err(error) => log::warn!(
            "更新清单读取失败：来源={address:?}，elapsed_ms={}，原因={}",
            started.elapsed().as_millis(),
            super::updater_error_details(&error)
        ),
    }
    Err("update.manifestOrSignatureUnavailable".into())
}

pub(crate) struct VerifiedInstallerDownload {
    pub(crate) bytes: Vec<u8>,
    #[cfg(target_os = "macos")]
    pub(crate) update: tauri_plugin_updater::Update,
}

async fn prepare_and_download_installer(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    app: &AppHandle,
    policy: &NetworkPolicy,
    manifest_channels: &[InstallerChannel],
    region: InstallerRegion,
    network: &crate::models::NetworkConfig,
    is_cancelled: impl Copy + Fn() -> bool,
    race_session: &RaceSession,
) -> Result<VerifiedInstallerDownload, String> {
    let provider = info
        .provider
        .as_ref()
        .ok_or("update.manifestPlatformMismatch")?;
    let url = info
        .installer_url
        .as_deref()
        .ok_or("update.manifestPlatformMismatch")?;
    let package_channels = installer_sources_for_url(policy, region, provider, url)?;
    let update = resolve_app_update(app, info, manifest_channels, network, is_cancelled).await?;
    let signature = update.signature.clone();
    let version = info.version.clone();
    let bytes = download_verified_installer(
        client,
        info,
        &package_channels,
        Some(app),
        region,
        move |bytes| crate::app_update::verify_download(bytes, &signature, &version),
        network,
        is_cancelled,
        Some(race_session),
    )
    .await?;
    race_session.commit();
    Ok(VerifiedInstallerDownload {
        bytes,
        #[cfg(target_os = "macos")]
        update,
    })
}

pub(crate) enum InstallerDownloadAttempt {
    Updated {
        info: AppUpdateInfo,
        download: VerifiedInstallerDownload,
    },
    Latest,
    Failed(String),
}

/// Try mainland direct mirrors in order before GitHub's independent manifest
/// and signature-validated package races; international uses only GitHub.
pub(crate) async fn discover_and_download_installer_update(
    state: &AppState,
    app: &AppHandle,
    checked_channel: Option<&str>,
    install_mode: AppUpdateInstallMode,
    is_cancelled: impl Copy + Fn() -> bool,
) -> Result<InstallerDownloadAttempt, String> {
    if is_cancelled() {
        return Err("download.cancelled".into());
    }
    let config = state.config.load_or_default();
    let race_session = state.download_history.session();
    let region = installer_region(&config.region);
    let policy = config
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(NetworkPolicy::active)?;
    let manifest_channels = installer_manifest_sources(&policy, region);
    log::info!("开始执行安装器更新流程：地区={}", region.as_str());
    let client =
        releases::build_client(config.network.api_timeout()).map_err(|error| error.to_string())?;
    let download_client = releases::build_download_client(config.network.first_byte_timeout())
        .map_err(|error| error.to_string())?;
    let target = crate::app_update::UpdateTarget::current()?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| {
        crate::locale_tag::with_params(
            "update.currentVersionInvalid2",
            &[("reason", &error.to_string())],
        )
    })?;
    // The supplied ID can only select a previously checked, official manifest.
    if let Some(channel) = checked_channel
        && let Some(mut info) =
            super::cache::app_update_cache_get(state, region).and_then(|result| result.update)
        && info.source_channel.as_deref() == Some(channel)
    {
        info.install_mode = install_mode;
        emit_installer_phase(app, region, "download", None, Some(info.clone()));
        match prepare_and_download_installer(
            &download_client,
            &info,
            app,
            &policy,
            &manifest_channels,
            region,
            &config.network,
            is_cancelled,
            &race_session,
        )
        .await
        {
            Ok(download) => {
                return Ok(InstallerDownloadAttempt::Updated { info, download });
            }
            Err(error) if error == "download.cancelled" => return Err(error),
            Err(error) => log::warn!(
                "按已检查的渠道下载更新失败，将重新检查：渠道={channel:?}，原因={}",
                crate::locale_tag::diagnostic_tag(&error)
            ),
        }
    }
    let result = run_installer_manifest_routes(
        &client,
        &manifest_channels,
        target,
        &current_version,
        is_cancelled,
        |status| {
            if status.status == "checking" {
                emit_installer_phase(app, region, "manifest", None, None);
            }
            emit_installer_channel_status(app, region, status);
        },
        |mut info| {
            info.install_mode = install_mode;
            let download_client = &download_client;
            let policy = &policy;
            let manifest_channels = &manifest_channels;
            let network = &config.network;
            let race_session = &race_session;
            async move {
                emit_installer_phase(app, region, "download", None, Some(info.clone()));
                let download = prepare_and_download_installer(
                    download_client,
                    &info,
                    app,
                    policy,
                    manifest_channels,
                    region,
                    network,
                    is_cancelled,
                    race_session,
                )
                .await?;
                Ok((info, download))
            }
        },
        &config.network,
    )
    .await?;
    match result {
        InstallerRouteResult::Updated((info, download)) => {
            return Ok(InstallerDownloadAttempt::Updated { info, download });
        }
        InstallerRouteResult::Latest => return Ok(InstallerDownloadAttempt::Latest),
        InstallerRouteResult::Failed => {}
    }
    let message = "update.manualRequired".to_string();
    emit_installer_phase(app, region, "skipped", Some(message.clone()), None);
    Ok(InstallerDownloadAttempt::Failed(message))
}
