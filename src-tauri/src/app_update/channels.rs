use crate::archive::DownloadRoute as AssetDownloadRoute;
use crate::download::DownloadCandidate as AssetDownloadCandidate;
use crate::download::DownloadExecution;
use crate::download::history::{HistoryPurpose, RouteIdentity};
use crate::download::planning::materialize_route;
use crate::models::{
    AppUpdateChannelEvent, AppUpdateChannelStatus, AppUpdateInfo, AppUpdatePhaseEvent,
    NetworkPlanSelection, Provider, RepositoryKind,
};
use crate::network_policy::{Intent, NetworkPolicy};
use tauri::AppHandle;
use tauri::Emitter;

pub(crate) const INSTALLER_GITEE_OWNER: &str = "ch3ngyz";

pub(crate) const INSTALLER_GITEE_REPO: &str = "Overcooked-2-MOD-Manager";

pub(crate) const INSTALLER_GITHUB_OWNER: &str = "CH3NGYZ";

pub(crate) const INSTALLER_GITHUB_REPO: &str = "Overcooked-2-MOD-Manager";
pub(crate) const INSTALLER_GITCODE_OWNER: &str = "CH3NGYZ";
pub(crate) const INSTALLER_GITCODE_REPO: &str = "Overcooked-2-Manager";

pub(crate) const INSTALLER_PROVIDERS: [Provider; 3] =
    [Provider::Gitee, Provider::Gitcode, Provider::Github];

#[derive(Debug, Clone)]
pub(crate) struct InstallerChannel {
    pub(crate) id: String,
    pub(crate) provider: Provider,
    pub(crate) endpoint: String,
    pub(crate) route: AssetDownloadRoute,
    pub(crate) execution: DownloadExecution,
    pub(crate) identity: Option<RouteIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallerRegion {
    MainlandChina,
    International,
}

impl InstallerRegion {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MainlandChina => crate::config::REGION_CHINESE_MAINLAND,
            Self::International => crate::config::REGION_INTERNATIONAL,
        }
    }
}

pub(crate) fn installer_region(region: &str) -> InstallerRegion {
    if region == crate::config::REGION_CHINESE_MAINLAND {
        InstallerRegion::MainlandChina
    } else {
        InstallerRegion::International
    }
}

pub(crate) fn installer_manifest_url(provider: &Provider) -> String {
    match provider {
        Provider::Gitcode => format!(
            "https://gitcode.com/{INSTALLER_GITCODE_OWNER}/{INSTALLER_GITCODE_REPO}/releases/download/latest/latest.json"
        ),
        Provider::Gitee => format!(
            "https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases/download/latest/latest.json"
        ),
        Provider::Github => format!(
            "https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases/latest/download/latest.json"
        ),
    }
}

pub(crate) fn installer_sources_for_url(
    policy: &NetworkPolicy,
    region: InstallerRegion,
    provider: &Provider,
    url: &str,
) -> Result<Vec<InstallerChannel>, String> {
    let plan = policy.resolve(
        provider,
        RepositoryKind::Mod,
        region.as_str(),
        &NetworkPlanSelection::default(),
    )?;
    let release = plan.route(Intent::Release)?;
    let name = match provider {
        Provider::Gitee => "gitee",
        Provider::Github => "github",
        Provider::Gitcode => "gitcode",
    };
    Ok(
        materialize_route(policy, release, url, false, None, None, None)?
            .into_iter()
            .map(|(candidate, proxy_id)| InstallerChannel {
                id: match proxy_id {
                    Some(id) => format!("{name}-proxy:{id}"),
                    None => format!("{name}-direct"),
                },
                provider: provider.clone(),
                route: candidate.route,
                identity: release
                    .race
                    .then(|| {
                        RouteIdentity::for_candidate(
                            region.as_str(),
                            provider,
                            HistoryPurpose::Installer,
                            &candidate.url,
                            proxy_id.and_then(|id| policy.proxies.get(id)),
                        )
                    })
                    .flatten(),
                endpoint: candidate.url,
                execution: DownloadExecution {
                    max_parallelism: release.max_parallelism,
                    race: release.race,
                },
            })
            .collect(),
    )
}

pub(crate) fn installer_manifest_sources(
    policy: &NetworkPolicy,
    region: InstallerRegion,
) -> Vec<InstallerChannel> {
    let mut sources = Vec::new();
    for provider in &INSTALLER_PROVIDERS {
        if region == InstallerRegion::International && *provider != Provider::Github {
            continue;
        }
        match installer_sources_for_url(policy, region, provider, &installer_manifest_url(provider))
        {
            Ok(group) => sources.extend(group),
            Err(error) => log::warn!(
                "更新清单计划无法生成候选：平台={provider:?}，原因={}",
                crate::locale_tag::diagnostic_tag(&error)
            ),
        }
    }
    sources
}

pub(crate) fn installer_candidate(channel: &InstallerChannel) -> AssetDownloadCandidate {
    AssetDownloadCandidate {
        route: channel.route,
        url: channel.endpoint.clone(),
        identity: channel.identity.clone(),
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
        mode: if channel.route == AssetDownloadRoute::Proxy {
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
            status
                .error
                .as_deref()
                .map(crate::locale_tag::diagnostic_tag)
                .unwrap_or("unclassified")
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
