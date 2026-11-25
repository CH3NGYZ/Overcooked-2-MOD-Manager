use crate::app_update::channels::INSTALLER_PROVIDERS;
use crate::app_update::channels::InstallerChannel;
use crate::app_update::channels::InstallerRegion;
use crate::app_update::channels::emit_installer_channel_status;
use crate::app_update::channels::emit_installer_phase;
use crate::app_update::channels::installer_candidate;
use crate::app_update::channels::installer_channel_status;
use crate::archive;
use crate::download::DownloadContext;
use crate::download::try_download_candidates_with_progress;
use crate::http_diagnostics::HttpRequestExt;
use crate::models::{
    AppConfig, AppUpdateChannelStatus, AppUpdateCheckResult, AppUpdateInfo, AppUpdateInstallMode,
};
use crate::releases;
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::AppHandle;

mod changelog;
mod package;

#[derive(Deserialize)]
struct ReleaseMetadata {
    version: semver::Version,
    notes: Option<String>,
    pub_date: Option<String>,
}

pub(crate) fn inspect_installer_manifest(
    channel: &InstallerChannel,
    document: &serde_json::Value,
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
) -> Result<Option<AppUpdateInfo>, String> {
    let manifest =
        ReleaseMetadata::deserialize(document).map_err(|_| "update.manifestInvalid".to_string())?;
    if !manifest.version.pre.is_empty() || !manifest.version.build.is_empty() {
        return Err("update.manifestNotStable".into());
    }
    if manifest
        .pub_date
        .as_deref()
        .is_some_and(|date| chrono::DateTime::parse_from_rfc3339(date).is_err())
    {
        return Err("update.manifestInvalid".into());
    }
    let package = package::select(document, target, &manifest.version)?;
    if manifest.version.cmp_precedence(current_version) != std::cmp::Ordering::Greater {
        return Ok(None);
    }
    let mut info = AppUpdateInfo {
        version: manifest.version.to_string(),
        current_version: current_version.to_string(),
        changelog: changelog::versioned_notes(
            document.get("changelog"),
            current_version,
            &manifest.version,
            manifest.notes.as_deref(),
        ),
        body: manifest.notes,
        date: manifest.pub_date,
        download_size: None,
        manifest_url: channel.endpoint.clone(),
        installer_url: None,
        installer_signature: String::new(),
        installer_name: String::new(),
        installer_type: target.kind.into(),
        provider: None,
        source_channel: Some(channel.id.clone()),
        install_mode: AppUpdateInstallMode::Manual,
    };
    if let Some(package) = package {
        info.installer_url = package.url;
        info.installer_signature = package.signature;
        info.installer_name = package.name;
        info.provider = Some(package.provider);
    }
    Ok(Some(info))
}

pub(super) async fn fetch_installer_manifest(
    client: &reqwest::Client,
    channel: &InstallerChannel,
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
    is_cancelled: impl Fn() -> bool,
    reporter: Option<&archive::DownloadReporter>,
) -> Result<Option<AppUpdateInfo>, String> {
    const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
    log::info!(
        "正在请求程序更新清单：渠道={:?}，来源={:?}",
        channel.id,
        url::Url::parse(&channel.endpoint)
            .ok()
            .map(|url| url.origin().ascii_serialization())
    );
    let request = async {
        let response = client
            .get(&channel.endpoint)
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache")
            .send_logged_metadata()
            .await
            .map_err(|_| "update.manifestRequestFailed")?;
        if !response.status().is_success() {
            return Err(crate::locale_tag::with_params(
                "update.manifestHttpStatus",
                &[("status", &response.status().as_u16().to_string())],
            ));
        }
        let total = response.content_length();
        if total.is_some_and(|size| size > MAX_MANIFEST_BYTES as u64) {
            return Err("update.manifestTooLarge".into());
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "update.manifestReadFailed")?;
            if bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
                return Err("update.manifestTooLarge".into());
            }
            bytes.extend_from_slice(&chunk);
            if let Some(reporter) = reporter {
                reporter.received(chunk.len() as u64);
                let mut progress = reporter.snapshot();
                progress.downloaded = bytes.len() as u64;
                progress.total = total;
                reporter.update(progress);
            }
        }
        let document = serde_json::from_slice(&bytes).map_err(|error| {
            log::warn!(
                "更新清单解析失败：渠道={:?}，字节数={}，JSON错误={:?}，行={}，列={}",
                channel.id,
                bytes.len(),
                error.classify(),
                error.line(),
                error.column()
            );
            "update.manifestInvalid"
        })?;
        inspect_installer_manifest(channel, &document, target, current_version)
    };
    let cancellation = async {
        loop {
            if is_cancelled() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        biased;
        _ = cancellation => Err("download.cancelled".into()),
        result = request => result,
    }
}

pub(crate) enum InstallerRouteResult<T> {
    Updated(T),
    Latest,
    Failed,
}

struct SelectedInstallerManifest {
    channel: InstallerChannel,
    info: Option<AppUpdateInfo>,
}

impl crate::download::DownloadOutput for SelectedInstallerManifest {
    fn discard(&self) {}
}

pub(crate) async fn race_installer_channels<T, F, Fut>(
    channels: &[InstallerChannel],
    is_cancelled: impl Fn() -> bool,
    mut fetch: F,
    network: &crate::models::NetworkConfig,
    context: Option<DownloadContext<'_>>,
) -> Result<T, String>
where
    T: crate::download::DownloadOutput,
    F: FnMut(InstallerChannel, std::sync::Arc<archive::DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    if is_cancelled() {
        return Err("download.cancelled".into());
    }
    let execution = channels.first().ok_or("proxy.noGlobalProxy")?.execution;
    let report_progress = context.is_some();
    let mut context = context.unwrap_or(DownloadContext {
        execution: None,
        label: "",
        file_name: None,
        expected_size: None,
        race_session: None,
    });
    context.execution = Some(execution);
    try_download_candidates_with_progress(
        channels.iter().map(installer_candidate).collect(),
        is_cancelled,
        |candidate, reporter| {
            let channel = channels
                .iter()
                .find(|channel| channel.endpoint == candidate.url)
                .unwrap();
            fetch(channel.clone(), reporter)
        },
        network,
        Some(context),
        |progress| {
            if report_progress {
                archive::publish_download_progress(progress);
            }
        },
    )
    .await
}

pub(crate) async fn run_installer_manifest_routes<T, F, Fut>(
    client: &reqwest::Client,
    channels: &[InstallerChannel],
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
    is_cancelled: impl Fn() -> bool,
    on_status: impl FnMut(AppUpdateChannelStatus),
    mut on_update: F,
    network: &crate::models::NetworkConfig,
) -> Result<InstallerRouteResult<T>, String>
where
    F: FnMut(AppUpdateInfo) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let on_status = Mutex::new(on_status);
    let providers = INSTALLER_PROVIDERS;
    for (index, provider) in providers.iter().enumerate() {
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        let group = channels
            .iter()
            .filter(|channel| &channel.provider == provider)
            .cloned()
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        let started = Mutex::new(HashSet::new());
        let finished = Mutex::new(HashSet::new());
        let selected = race_installer_channels(
            &group,
            &is_cancelled,
            |channel, reporter| {
                started.lock().unwrap().insert(channel.id.clone());
                on_status.lock().unwrap()(installer_channel_status(&channel, "checking", None));
                let (on_status, finished, is_cancelled) = (&on_status, &finished, &is_cancelled);
                async move {
                    let result = fetch_installer_manifest(
                        client,
                        &channel,
                        target,
                        current_version,
                        is_cancelled,
                        Some(&reporter),
                    )
                    .await;
                    finished.lock().unwrap().insert(channel.id.clone());
                    match result {
                        Ok(info) => {
                            on_status.lock().unwrap()(installer_channel_status(
                                &channel,
                                if info.is_some() {
                                    "update_available"
                                } else {
                                    "latest"
                                },
                                None,
                            ));
                            Ok(SelectedInstallerManifest { channel, info })
                        }
                        Err(error) => {
                            on_status.lock().unwrap()(installer_channel_status(
                                &channel,
                                "failed",
                                Some(error.clone()),
                            ));
                            Err(error)
                        }
                    }
                }
            },
            network,
            None,
        )
        .await;
        // The coordinator has dropped losers before their final UI state.
        for channel in &group {
            if !finished.lock().unwrap().contains(&channel.id) {
                let status = if started.lock().unwrap().contains(&channel.id) {
                    "cancelled"
                } else {
                    "skipped"
                };
                on_status.lock().unwrap()(installer_channel_status(channel, status, None));
            }
        }
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) if error == "download.cancelled" => return Err(error),
            Err(_) => continue,
        };
        let result = match selected.info {
            Some(info) => on_update(info).await.map(InstallerRouteResult::Updated),
            None => Ok(InstallerRouteResult::Latest),
        };
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        match result {
            Ok(result) => {
                for skipped in channels
                    .iter()
                    .filter(|channel| providers[index + 1..].contains(&channel.provider))
                {
                    on_status.lock().unwrap()(installer_channel_status(skipped, "skipped", None));
                }
                return Ok(result);
            }
            Err(error) if error == "download.cancelled" => return Err(error),
            Err(error) => on_status.lock().unwrap()(installer_channel_status(
                &selected.channel,
                "failed",
                Some(error),
            )),
        }
    }
    Ok(InstallerRouteResult::Failed)
}

pub(crate) async fn discover_installer_update(
    config: &AppConfig,
    app: &AppHandle,
    region: InstallerRegion,
    channels: &[InstallerChannel],
) -> Result<AppUpdateCheckResult, String> {
    emit_installer_phase(app, region, "manifest", None, None);
    let client = releases::build_client(config.network.api_timeout()).map_err(|e| e.to_string())?;
    let target = crate::app_update::UpdateTarget::current()?;
    let current_version = app.package_info().version.clone();
    log::info!(
        "安装器更新发现开始：当前版本={current_version}，目标平台={}，渠道={}",
        target.manifest_key(),
        channels
            .iter()
            .map(|channel| channel.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut statuses = Vec::<AppUpdateChannelStatus>::new();
    let result = compare_all_installer_manifest_routes(
        &client,
        channels,
        target,
        &current_version,
        {
            let statuses = &mut statuses;
            move |status| {
                emit_installer_channel_status(app, region, status.clone());
                if let Some(previous) = statuses
                    .iter_mut()
                    .find(|previous| previous.id == status.id)
                {
                    *previous = status;
                } else {
                    statuses.push(status);
                }
            }
        },
        &config.network,
    )
    .await?;
    let (update, all_failed) = match result {
        InstallerRouteResult::Updated(info) => (Some(info), false),
        InstallerRouteResult::Latest => (None, false),
        InstallerRouteResult::Failed => (None, true),
    };
    Ok(AppUpdateCheckResult {
        distribution: crate::models::AppDistribution::Standalone,
        update,
        channels: statuses,
        all_failed,
        region: region.as_str().into(),
        recovery_required: false,
        installation_status: None,
    })
}

pub(super) async fn compare_all_installer_manifest_routes(
    client: &reqwest::Client,
    channels: &[InstallerChannel],
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
    mut on_status: impl FnMut(AppUpdateChannelStatus),
    network: &crate::models::NetworkConfig,
) -> Result<InstallerRouteResult<AppUpdateInfo>, String> {
    let mut best = Option::<AppUpdateInfo>::None;
    let mut any_success = false;
    for provider in INSTALLER_PROVIDERS {
        let group = channels
            .iter()
            .filter(|channel| channel.provider == provider)
            .cloned()
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        let result = run_installer_manifest_routes(
            client,
            &group,
            target,
            current_version,
            || false,
            |status| on_status(status),
            |info| std::future::ready(Ok(info)),
            network,
        )
        .await?;
        match result {
            InstallerRouteResult::Updated(info) => {
                let version = semver::Version::parse(&info.version)
                    .map_err(|_| "update.manifestInvalid".to_string())?;
                let prefer = if let Some(best) = best.as_ref() {
                    let previous = semver::Version::parse(&best.version)
                        .map_err(|_| "update.manifestInvalid".to_string())?;
                    previous.cmp_precedence(&version) == std::cmp::Ordering::Less
                        || (previous.cmp_precedence(&version) == std::cmp::Ordering::Equal
                            && !best.has_signed_package()
                            && info.has_signed_package())
                } else {
                    true
                };
                if prefer {
                    best = Some(info);
                }
                any_success = true;
            }
            InstallerRouteResult::Latest => any_success = true,
            InstallerRouteResult::Failed => {}
        }
    }
    if let Some(info) = best {
        Ok(InstallerRouteResult::Updated(info))
    } else if any_success {
        Ok(InstallerRouteResult::Latest)
    } else {
        Ok(InstallerRouteResult::Failed)
    }
}
