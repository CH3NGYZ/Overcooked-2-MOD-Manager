use crate::app_state::AppState;
use crate::app_update::channels::detect_installer_region;
use crate::app_update::channels::emit_installer_channel_status;
use crate::app_update::channels::emit_installer_phase;
use crate::app_update::channels::installer_channel_status;
use crate::app_update::channels::installer_download_candidates;
use crate::app_update::channels::installer_manifest_sources;
use crate::app_update::channels::installer_sources_for_url;
use crate::app_update::manifest::InstallerRouteResult;
use crate::app_update::manifest::race_installer_channels;
use crate::app_update::manifest::run_installer_manifest_routes;
use crate::archive;
use crate::archive::DownloadRoute as AssetDownloadRoute;
use crate::download::DownloadCandidate as AssetDownloadCandidate;
use crate::download::DownloadContext;
use crate::download::try_download_candidates as try_asset_download_candidates;
use crate::models::{AppUpdateChannelStatus, AppUpdateInfo, GitHubProxyConfig};
use crate::releases;
use crate::repository_sources;
use futures_util::StreamExt;
use tauri::AppHandle;
use url::Url;

impl crate::download::DownloadOutput for tauri_plugin_updater::Update {
    fn discard(&self) {}
}

async fn download_installer(
    client: &reqwest::Client,
    url: &str,
    reporter: &archive::DownloadReporter,
    label: &str,
) -> Result<Vec<u8>, String> {
    log::debug!("开始下载安装器文件：标签={}", label);
    let response = client
        .get(url)
        .header("User-Agent", releases::APP_USER_AGENT)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let total = response.content_length();
    let started = std::time::Instant::now();
    let mut downloaded = 0_u64;
    let mut bytes = Vec::with_capacity(total.unwrap_or_default().min(64 * 1024 * 1024) as usize);
    let mut stream = response.bytes_stream();
    let first_chunk = tokio::time::timeout(archive::FIRST_BYTE_TIMEOUT, stream.next())
        .await
        .map_err(|_| "download.firstByteTimeout".to_string())?;
    let mut next_chunk = first_chunk;
    while let Some(chunk) = next_chunk {
        let chunk = chunk.map_err(|error| error.to_string())?;
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

pub(super) async fn download_installer_with_timeout(
    client: &reqwest::Client,
    url: &str,
    reporter: &archive::DownloadReporter,
    label: &str,
) -> Result<Vec<u8>, String> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        download_installer(client, url, reporter, label),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            log::warn!("下载安装器文件超过 30 秒：标签={}", label);
            Err("update.downloadTimeout".into())
        }
    }
}

async fn download_installer_for_info(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    app: &AppHandle,
    signature: &str,
    proxies: &GitHubProxyConfig,
) -> Result<Vec<u8>, Vec<String>> {
    let candidates = installer_download_candidates(proxies, info);
    download_verified_installer(client, info, proxies, candidates, Some(app), |bytes| {
        crate::app_update::verify_download(bytes, signature, &info.version)
    })
    .await
    .map_err(|error| vec![error])
}

pub(crate) async fn download_verified_installer(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    proxies: &GitHubProxyConfig,
    candidates: Vec<AssetDownloadCandidate>,
    app: Option<&AppHandle>,
    verify: impl Fn(&[u8]) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let proxy_count = candidates
        .iter()
        .filter(|candidate| candidate.route == AssetDownloadRoute::Proxy)
        .count();
    log::info!(
        "安装器下载候选准备完成：平台={:?}，直连={}，Release代理={}，总候选={}",
        info.provider,
        candidates.len() - proxy_count,
        proxy_count,
        candidates.len()
    );
    let result = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let verify = &verify;
            async move {
                let mode = if candidate.route == AssetDownloadRoute::Proxy {
                    "proxy"
                } else {
                    "direct"
                };
                let channel = info.provider.as_ref().and_then(|provider| {
                    installer_sources_for_url(
                        proxies,
                        provider,
                        info.installer_url.as_deref().unwrap_or_default(),
                    )
                    .into_iter()
                    .find(|channel| channel.endpoint == candidate.url)
                });
                if let (Some(app), Some(channel)) = (app, &channel) {
                    emit_installer_channel_status(
                        app,
                        detect_installer_region(),
                        AppUpdateChannelStatus {
                            mode: mode.into(),
                            ..installer_channel_status(channel, "downloading", None)
                        },
                    );
                }
                let result = download_installer_with_timeout(
                    client,
                    &candidate.url,
                    &reporter,
                    "update.downloadingInstaller",
                )
                .await
                .and_then(|bytes| {
                    archive::validate_download_bytes(
                        &bytes,
                        info.download_size.filter(|size| *size > 0),
                        None,
                    )?;
                    verify(&bytes)?;
                    Ok(bytes)
                });
                if let (Some(app), Some(channel)) = (app, &channel) {
                    emit_installer_channel_status(
                        app,
                        detect_installer_region(),
                        AppUpdateChannelStatus {
                            mode: mode.into(),
                            ..installer_channel_status(
                                channel,
                                if result.is_ok() {
                                    "downloaded"
                                } else {
                                    "failed"
                                },
                                result.as_ref().err().cloned(),
                            )
                        },
                    );
                }
                result
            }
        },
        Some(DownloadContext {
            execution: None,
            app,
            label: "update.downloadingInstaller",
            expected_size: info.download_size,
        }),
    )
    .await;
    result
}

async fn resolve_app_update(
    app: &AppHandle,
    info: &AppUpdateInfo,
    proxies: &GitHubProxyConfig,
) -> Result<tauri_plugin_updater::Update, String> {
    // Package hosting can differ from the manifest mirror. Re-read the selected
    // manifest's provider, then require identical version, URL and signature.
    let mut channels = installer_manifest_sources(
        proxies,
        crate::app_update::channels::InstallerRegion::MainlandChina,
    );
    let provider = channels
        .iter()
        .find(|channel| {
            channel.endpoint == info.manifest_url
                && Some(channel.id.as_str()) == info.source_channel.as_deref()
        })
        .map(|channel| channel.provider.clone())
        .ok_or("update.manifestUrlInvalid")?;
    channels.retain(|channel| channel.provider == provider);
    race_installer_channels(
        &channels,
        archive::is_download_cancelled,
        |channel, _| async move { resolve_app_update_at(app, info, &channel.endpoint).await },
    )
    .await
}

async fn resolve_app_update_at(
    app: &AppHandle,
    info: &AppUpdateInfo,
    url: &str,
) -> Result<tauri_plugin_updater::Update, String> {
    use tauri_plugin_updater::UpdaterExt;
    let target = crate::app_update::UpdateTarget::current()?;
    if target.kind != info.installer_type {
        return Err("update.manifestPlatformMismatch".into());
    }
    #[cfg(windows)]
    let installer_args = {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let target_executable = crate::update_migration::update_executable(&executable)?;
        let logs = crate::user_paths::logs_dir()?;
        std::fs::create_dir_all(&logs).map_err(|error| error.to_string())?;
        // Keep the original path so installer-side relocation can snapshot it.
        crate::app_update::windows_installer_args(
            target.kind,
            &executable,
            &target_executable,
            &logs.join("update-msi.log"),
        )?
    };
    if archive::is_download_cancelled() {
        return Err("download.cancelled".into());
    }
    let endpoint = Url::parse(url).map_err(|_| "update.manifestUrlInvalid")?;
    let builder = app
        .updater_builder()
        .target(target.manifest_key())
        .timeout(std::time::Duration::from_secs(5))
        .header("User-Agent", releases::APP_USER_AGENT)
        .map_err(|e| e.to_string())?
        .header("Cache-Control", "no-cache, no-store")
        .map_err(|e| e.to_string())?
        .header("Pragma", "no-cache")
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let builder = builder
        .clear_installer_args()
        .installer_args(installer_args.clone())
        // Our success hooks launch once; do not append /R or /ARGS.
        .restart_after_install(false)
        .on_before_exit(|| {
            log::info!("Tauri Updater 准备启动 Windows 安装器并退出当前程序；安装结果尚未确认");
        });
    let updater = match builder
        .endpoints(vec![endpoint])
        .and_then(|builder| builder.build())
    {
        Ok(updater) => updater,
        Err(error) => {
            log::warn!("更新清单入口不可用：原因={error}");
            return Err(error.to_string());
        }
    };
    let cancellation = async {
        while !archive::is_download_cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    let result = tokio::select! {
        biased;
        _ = cancellation => return Err("download.cancelled".into()),
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
        Ok(None) => log::warn!("更新清单没有所选 Release 的更新版本"),
        Err(_) => log::warn!(
            "更新清单读取失败：来源={}",
            repository_sources::safe_log_address(url)
        ),
    }
    Err("update.manifestOrSignatureUnavailable".into())
}

pub(crate) struct VerifiedInstallerDownload {
    pub(crate) bytes: Vec<u8>,
    pub(crate) update: tauri_plugin_updater::Update,
}

async fn prepare_and_download_installer(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    app: &AppHandle,
    proxies: &GitHubProxyConfig,
) -> Result<VerifiedInstallerDownload, Vec<String>> {
    let update = resolve_app_update(app, info, proxies)
        .await
        .map_err(|error| vec![error])?;
    let bytes = download_installer_for_info(client, info, app, &update.signature, proxies).await?;
    Ok(VerifiedInstallerDownload { bytes, update })
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
) -> Result<InstallerDownloadAttempt, String> {
    let region = detect_installer_region();
    log::info!("开始执行安装器更新流程：地区={}", region.as_str());
    let config = state.config.load_or_default();
    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let download_client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let target = crate::app_update::UpdateTarget::current()?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| {
        crate::locale_tag::with_params(
            "update.currentVersionInvalid2",
            &[("reason", &error.to_string())],
        )
    })?;
    let result = run_installer_manifest_routes(
        &client,
        &installer_manifest_sources(&config.github_proxies, region),
        target,
        &current_version,
        archive::is_download_cancelled,
        |status| {
            if status.status == "checking" {
                emit_installer_phase(app, region, "manifest", None, None);
            }
            emit_installer_channel_status(app, region, status);
        },
        |info| {
            let download_client = &download_client;
            let proxies = &config.github_proxies;
            async move {
                emit_installer_phase(app, region, "download", None, Some(info.clone()));
                let download = prepare_and_download_installer(download_client, &info, app, proxies)
                    .await
                    .map_err(|errors| errors.join("；"))?;
                Ok((info, download))
            }
        },
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
