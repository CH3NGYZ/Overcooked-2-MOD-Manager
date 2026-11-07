use crate::app_update::channels::INSTALLER_GITEE_OWNER;
use crate::app_update::channels::INSTALLER_GITEE_REPO;
use crate::app_update::channels::INSTALLER_GITHUB_OWNER;
use crate::app_update::channels::INSTALLER_GITHUB_REPO;
use crate::app_update::channels::InstallerChannel;
use crate::app_update::channels::detect_installer_region;
use crate::app_update::channels::emit_installer_channel_status;
use crate::app_update::channels::emit_installer_phase;
use crate::app_update::channels::installer_candidate;
use crate::app_update::channels::installer_channel_status;
use crate::app_update::channels::installer_manifest_sources;
use crate::archive;
use crate::download::try_download_candidates as try_asset_download_candidates;
use crate::models::{
    AppConfig, AppUpdateChannelStatus, AppUpdateCheckResult, AppUpdateInfo, Provider,
};
use crate::releases;
use crate::repository_sources;
use futures_util::StreamExt;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::AppHandle;

pub(crate) fn inspect_installer_manifest(
    channel: &InstallerChannel,
    document: &serde_json::Value,
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
) -> Result<Option<AppUpdateInfo>, String> {
    let manifest: tauri_plugin_updater::RemoteRelease = serde_json::from_value(document.clone())
        .map_err(|_| "update.manifestInvalid".to_string())?;
    if !manifest.version.pre.is_empty() {
        return Err("update.manifestNotStable".into());
    }
    let key = target.manifest_key();
    let url = manifest
        .download_url(&key)
        .map_err(|_| "update.noMatchingPackage")?;
    let signature = manifest
        .signature(&key)
        .map_err(|_| "update.manifestOrSignatureUnavailable")?
        .clone();
    let name = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .unwrap_or_default()
        .to_string();
    let (host, owner, repo) = match channel.provider {
        Provider::Github => ("github.com", INSTALLER_GITHUB_OWNER, INSTALLER_GITHUB_REPO),
        Provider::Gitee => ("gitee.com", INSTALLER_GITEE_OWNER, INSTALLER_GITEE_REPO),
    };
    let path_prefix = format!("/{owner}/{repo}/releases/download/");
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with(&path_prefix)
        || !target.matches_name(&name, &manifest.version)
        || signature.trim().is_empty()
    {
        return Err("update.manifestPlatformMismatch".into());
    }
    if manifest.version <= *current_version {
        return Ok(None);
    }
    let direct_url = url.to_string();
    Ok(Some(AppUpdateInfo {
        version: manifest.version.to_string(),
        current_version: current_version.to_string(),
        body: manifest.notes,
        date: document["pub_date"].as_str().map(str::to_string),
        download_size: None,
        manifest_url: channel.endpoint.clone(),
        installer_url: Some(direct_url),
        installer_signature: signature.clone(),
        installer_name: name,
        installer_type: target.kind.into(),
        provider: Some(channel.provider.clone()),
        source_channel: Some(channel.id.clone()),
    }))
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
        "正在请求程序更新清单：渠道={}，地址={}",
        channel.id,
        repository_sources::safe_log_address(&channel.endpoint)
    );
    let request = async {
        let response = client
            .get(&channel.endpoint)
            .header("User-Agent", releases::APP_USER_AGENT)
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache")
            .timeout(std::time::Duration::from_secs(5))
            .send()
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
        let document = serde_json::from_slice(&bytes).map_err(|_| "update.manifestInvalid")?;
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
) -> Result<T, String>
where
    T: crate::download::DownloadOutput,
    F: FnMut(InstallerChannel, std::sync::Arc<archive::DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    if is_cancelled() {
        return Err("download.cancelled".into());
    }
    // Gitee never enters a parallel pool or uses proxy/range requests.
    if channels.len() == 1 && channels[0].provider == Provider::Gitee {
        return fetch(
            channels[0].clone(),
            std::sync::Arc::new(archive::DownloadReporter::new("")),
        )
        .await;
    }
    try_asset_download_candidates(
        channels.iter().map(installer_candidate).collect(),
        is_cancelled,
        |candidate, reporter| {
            let channel = channels
                .iter()
                .find(|channel| channel.endpoint == candidate.url)
                .unwrap();
            fetch(channel.clone(), reporter)
        },
        None,
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
) -> Result<InstallerRouteResult<T>, String>
where
    F: FnMut(AppUpdateInfo) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let on_status = Mutex::new(on_status);
    for provider in [Provider::Gitee, Provider::Github] {
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        let group = channels
            .iter()
            .filter(|channel| channel.provider == provider)
            .cloned()
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        let started = Mutex::new(HashSet::new());
        let finished = Mutex::new(HashSet::new());
        let selected = race_installer_channels(&group, &is_cancelled, |channel, reporter| {
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
        })
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
                for skipped in channels.iter().filter(|channel| {
                    provider == Provider::Gitee && channel.provider == Provider::Github
                }) {
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
) -> Result<AppUpdateCheckResult, String> {
    let region = detect_installer_region();
    emit_installer_phase(app, region, "manifest", None, None);
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let target = crate::app_update::UpdateTarget::current()?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| {
        crate::locale_tag::with_params(
            "update.currentVersionInvalid",
            &[("reason", &e.to_string())],
        )
    })?;
    let mut statuses: Vec<AppUpdateChannelStatus> = Vec::new();
    let result = run_installer_manifest_routes(
        &client,
        &installer_manifest_sources(&config.github_proxies, region),
        target,
        &current_version,
        || false,
        |status| {
            emit_installer_channel_status(app, region, status.clone());
            if let Some(previous) = statuses
                .iter_mut()
                .find(|previous| previous.id == status.id)
            {
                *previous = status;
            } else {
                statuses.push(status);
            }
        },
        |info| std::future::ready(Ok(info)),
    )
    .await?;
    let all_failed = matches!(result, InstallerRouteResult::Failed);
    let update = match result {
        InstallerRouteResult::Updated(info) => Some(info),
        _ => None,
    };
    Ok(AppUpdateCheckResult {
        update,
        channels: statuses,
        all_failed,
        region: region.as_str().into(),
    })
}
