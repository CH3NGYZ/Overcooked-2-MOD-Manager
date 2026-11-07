use crate::archive::DownloadRoute as AssetDownloadRoute;
use crate::download::DownloadCandidate as AssetDownloadCandidate;
use crate::models::{
    AppUpdateChannelEvent, AppUpdateChannelStatus, AppUpdateInfo, AppUpdatePhaseEvent, GitHubProxy,
    GitHubProxyConfig, Provider,
};
use std::collections::HashSet;
use tauri::AppHandle;
use tauri::Emitter;

pub(crate) const INSTALLER_GITEE_OWNER: &str = "ch3ngyz";

pub(crate) const INSTALLER_GITEE_REPO: &str = "Overcooked-2-MOD-Manager";

pub(crate) const INSTALLER_GITHUB_OWNER: &str = "CH3NGYZ";

pub(crate) const INSTALLER_GITHUB_REPO: &str = "Overcooked-2-MOD-Manager";

#[derive(Debug, Clone)]
pub(crate) struct InstallerChannel {
    pub(crate) id: String,
    pub(crate) provider: Provider,
    pub(crate) endpoint: String,
    pub(crate) proxy: Option<GitHubProxy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallerRegion {
    MainlandChina,
    International,
}

impl InstallerRegion {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MainlandChina => "mainland-china",
            Self::International => "international",
        }
    }
}

pub(crate) fn detect_installer_region() -> InstallerRegion {
    if crate::config::current_user_region() == crate::config::REGION_CHINESE_MAINLAND {
        InstallerRegion::MainlandChina
    } else {
        InstallerRegion::International
    }
}

pub(crate) fn installer_manifest_url(provider: &Provider) -> String {
    match provider {
        Provider::Gitee => format!(
            "https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases/download/latest/latest.json"
        ),
        Provider::Github => format!(
            "https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases/latest/download/latest.json"
        ),
    }
}

pub(crate) fn installer_sources_for_url(
    proxies: &GitHubProxyConfig,
    provider: &Provider,
    url: &str,
) -> Vec<InstallerChannel> {
    let mut sources = vec![InstallerChannel {
        id: if *provider == Provider::Gitee {
            "gitee-direct"
        } else {
            "github-direct"
        }
        .into(),
        provider: provider.clone(),
        endpoint: url.into(),
        proxy: None,
    }];
    if *provider == Provider::Gitee {
        return sources;
    }
    // One snapshot covers manifest discovery, signature checks and package downloads.
    let mut visited = HashSet::from([url.to_string()]);
    for proxy in proxies.release_proxies.iter().cloned() {
        let Some(endpoint) =
            crate::github_proxies::proxy_urls(std::slice::from_ref(&proxy), url, false, None)
                .into_iter()
                .next()
        else {
            continue;
        };
        if visited.insert(endpoint.clone()) {
            sources.push(InstallerChannel {
                id: format!("github-proxy:{}", proxy.id),
                provider: Provider::Github,
                endpoint,
                proxy: Some(proxy),
            });
        }
    }
    sources
}

pub(crate) fn installer_manifest_sources(
    proxies: &GitHubProxyConfig,
    region: InstallerRegion,
) -> Vec<InstallerChannel> {
    let mut sources = Vec::new();
    if region == InstallerRegion::MainlandChina {
        sources.extend(installer_sources_for_url(
            proxies,
            &Provider::Gitee,
            &installer_manifest_url(&Provider::Gitee),
        ));
    }
    sources.extend(installer_sources_for_url(
        proxies,
        &Provider::Github,
        &installer_manifest_url(&Provider::Github),
    ));
    sources
}

pub(crate) fn installer_candidate(channel: &InstallerChannel) -> AssetDownloadCandidate {
    AssetDownloadCandidate {
        route: if channel.proxy.is_some() {
            AssetDownloadRoute::Proxy
        } else {
            AssetDownloadRoute::Direct
        },
        url: channel.endpoint.clone(),
    }
}

pub(crate) fn installer_download_candidates(
    proxies: &GitHubProxyConfig,
    info: &AppUpdateInfo,
) -> Vec<AssetDownloadCandidate> {
    match (&info.provider, &info.installer_url) {
        (Some(provider), Some(url)) => installer_sources_for_url(proxies, provider, url)
            .iter()
            .map(installer_candidate)
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn installer_channel_status(
    channel: &InstallerChannel,
    status: &str,
    error: Option<String>,
) -> AppUpdateChannelStatus {
    AppUpdateChannelStatus {
        id: channel.id.clone(),
        provider: channel.provider.clone(),
        mode: if channel.proxy.is_some() {
            "proxy"
        } else {
            "direct"
        }
        .into(),
        status: status.into(),
        error,
    }
}

pub(crate) fn emit_installer_channel_status(
    app: &AppHandle,
    region: InstallerRegion,
    status: AppUpdateChannelStatus,
) {
    match status.status.as_str() {
        "failed" => log::warn!(
            "安装器更新渠道失败: id={} mode={} error={}",
            status.id,
            status.mode,
            status.error.as_deref().unwrap_or_default()
        ),
        _ => log::info!(
            "安装器更新渠道状态: id={} mode={} status={}",
            status.id,
            status.mode,
            status.status
        ),
    }
    let _ = app.emit(
        "app-update-channel",
        AppUpdateChannelEvent {
            region: region.as_str().into(),
            channel: status,
        },
    );
}

pub(crate) fn emit_installer_phase(
    app: &AppHandle,
    region: InstallerRegion,
    phase: &str,
    message: Option<String>,
    update: Option<AppUpdateInfo>,
) {
    let _ = app.emit(
        "app-update-phase",
        AppUpdatePhaseEvent {
            region: region.as_str().into(),
            phase: phase.into(),
            message,
            update,
        },
    );
}

#[cfg(test)]
mod update_status_tests;
