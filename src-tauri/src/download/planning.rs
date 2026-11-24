use super::DownloadCandidate;
use super::history::{HistoryPurpose, RouteIdentity};
use crate::archive::DownloadRoute;
use crate::models::{Provider, ProxyMode};
use std::collections::HashSet;
use url::Url;

/// 远程 URL 的下载类别。
///
/// 平台的 Release 和 Raw 文件使用各自的请求计划，其它地址保持单源直连。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteUrlKind {
    ReleaseAsset,
    Raw,
    Other,
}

/// 按 URL 判定下载类别；解析失败或不是 HTTP(S) 一律按 `Other`（真正的
/// 拒绝在下载入口，这里只决定用哪套下载策略）。
pub(crate) fn classify_remote_url(url: &str) -> RemoteUrlKind {
    let Ok(parsed) = Url::parse(url.trim()) else {
        return RemoteUrlKind::Other;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return RemoteUrlKind::Other;
    }
    let host = parsed.host_str().unwrap_or_default();
    let path = parsed.path();
    let github_latest = matches!(host, "github.com" | "www.github.com")
        && parsed.path_segments().is_some_and(|segments| {
            let mut tail = segments.skip(2);
            tail.next() == Some("releases")
                && tail.next() == Some("latest")
                && tail.next() == Some("download")
                && tail.next().is_some_and(|asset| !asset.is_empty())
        });
    if crate::providers::release_asset_reference(&parsed).is_some() || github_latest {
        return RemoteUrlKind::ReleaseAsset;
    }
    if matches!(
        host,
        "raw.githubusercontent.com"
            | "raw.giteeusercontent.com"
            | "raw.atomgit.com"
            | "raw.gitcode.com"
    ) {
        return RemoteUrlKind::Raw;
    }
    if crate::providers::provider_for_host(host).is_some() && is_repository_raw_path(path) {
        return RemoteUrlKind::Raw;
    }
    RemoteUrlKind::Other
}

/// `/{owner}/{repo}/raw/{...}`：GitHub / Gitee 仓库文件的 raw 入口。
fn is_repository_raw_path(path: &str) -> bool {
    let mut segments = path.trim_start_matches('/').split('/');
    let _owner = segments.next();
    let _repo = segments.next();
    segments.next() == Some("raw") && segments.next().is_some()
}

/// 按 URL 判定下载源平台：决定用哪组代理、以及是否需要竞速。
pub(crate) fn provider_for_remote_url(url: &str) -> Option<Provider> {
    let parsed = Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    crate::providers::provider_for_host(parsed.host_str()?)
}

#[cfg(test)]
pub(crate) fn ordered_asset_download_candidates(
    mode: ProxyMode,
    direct_url: &str,
    proxy_urls: Vec<String>,
) -> Vec<DownloadCandidate> {
    let direct = || DownloadCandidate {
        route: DownloadRoute::Direct,
        url: direct_url.to_string(),
        identity: None,
    };
    let proxies = proxy_urls
        .into_iter()
        .filter(|url| url != direct_url)
        .map(|url| DownloadCandidate {
            route: DownloadRoute::Proxy,
            url,
            identity: None,
        })
        .collect::<Vec<_>>();
    let candidates = match mode {
        ProxyMode::Direct => vec![direct()],
        ProxyMode::Fixed => proxies.into_iter().take(1).collect(),
        ProxyMode::AutoDirect | ProxyMode::AutoProxy => {
            std::iter::once(direct()).chain(proxies).collect()
        }
    };
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.url.clone()))
        .collect()
}

/// File concurrency for URLs outside the recognized Release and Raw layouts.
pub(crate) fn generic_download_threads(
    provider: Option<&Provider>,
    region: &str,
    configured: u32,
) -> u32 {
    if matches!(provider, Some(Provider::Gitee | Provider::Gitcode))
        || (provider == Some(&Provider::Github) && region == crate::config::REGION_INTERNATIONAL)
    {
        1
    } else {
        configured.clamp(1, 32)
    }
}

/// Materialized file route. The same resolved plan controls both whole-file
/// concurrency and per-candidate chunks; source fields cannot override it later.
pub(crate) struct RepositoryDownloadPlan {
    pub candidates: Vec<DownloadCandidate>,
    pub download_threads: u32,
    pub execution: super::DownloadExecution,
}

pub(crate) fn plan_pasted_file(
    url: &Url,
    url_kind: RemoteUrlKind,
    kind: crate::models::RepositoryKind,
    region: &str,
) -> Result<RepositoryDownloadPlan, String> {
    let provider = provider_for_remote_url(url.as_str()).ok_or("direct.invalidUrl")?;
    plan_repository_file(
        &provider,
        kind,
        &crate::models::NetworkPlanSelection::default(),
        region,
        url.as_str(),
        url_kind == RemoteUrlKind::Raw,
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_repository_download(
    provider: &Provider,
    kind: crate::models::RepositoryKind,
    selection: &crate::models::NetworkPlanSelection,
    region: &str,
    url: &str,
    raw: Option<crate::github_proxies::RawProxyContext<'_>>,
    requested: Option<ProxyMode>,
    selected_id: Option<&str>,
) -> Result<RepositoryDownloadPlan, String> {
    plan_repository_file(
        provider,
        kind,
        selection,
        region,
        url,
        raw.is_some(),
        raw,
        requested,
        selected_id,
    )
}

/// Source documents have an original URL rather than a repository file configuration.
pub(crate) fn plan_source_document(
    provider: &Provider,
    kind: crate::models::RepositoryKind,
    selection: &crate::models::NetworkPlanSelection,
    region: &str,
    url: &str,
    is_raw: bool,
) -> Result<RepositoryDownloadPlan, String> {
    plan_repository_file(
        provider, kind, selection, region, url, is_raw, None, None, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_repository_file(
    provider: &Provider,
    kind: crate::models::RepositoryKind,
    selection: &crate::models::NetworkPlanSelection,
    region: &str,
    url: &str,
    is_raw: bool,
    raw: Option<crate::github_proxies::RawProxyContext<'_>>,
    requested: Option<ProxyMode>,
    selected_id: Option<&str>,
) -> Result<RepositoryDownloadPlan, String> {
    use crate::network_policy::{Intent, NetworkPolicy};
    let policy = NetworkPolicy::active()?;
    let plan = policy.resolve(provider, kind, region, selection)?;
    let intent = if is_raw { Intent::Raw } else { Intent::Release };
    let purpose = if is_raw {
        HistoryPurpose::Raw
    } else {
        HistoryPurpose::Release
    };
    let route = plan.route(intent)?;
    let materialized = materialize_route(&policy, route, url, is_raw, raw, requested, selected_id)?;
    let learnable = route.race && materialized.len() > 1;
    let candidates = materialized
        .into_iter()
        .map(|(mut candidate, proxy_id)| {
            if learnable {
                candidate.identity = RouteIdentity::for_candidate(
                    region,
                    provider,
                    purpose,
                    &candidate.url,
                    proxy_id.and_then(|id| policy.proxies.get(id)),
                );
            }
            candidate
        })
        .collect();
    Ok(RepositoryDownloadPlan {
        candidates,
        download_threads: route.download_threads,
        execution: super::DownloadExecution {
            max_parallelism: route.max_parallelism,
            race: route.race,
        },
    })
}

/// Avatars belong to a platform owner, so they use the provider/region default
/// (shared MOD/map plans), not whichever repository happens to request first.
pub(crate) fn plan_avatar_download(
    provider: &Provider,
    region: &str,
    owner: &str,
) -> Result<Vec<DownloadCandidate>, String> {
    use crate::models::{NetworkPlanSelection, RepositoryKind};
    use crate::network_policy::{Intent, NetworkPolicy};
    let policy = NetworkPolicy::active()?;
    let plan = policy.resolve(
        provider,
        RepositoryKind::Mod,
        region,
        &NetworkPlanSelection::default(),
    )?;
    let route = plan.route(Intent::Avatar)?;
    let owner = owner.trim();
    if owner.is_empty() {
        return Err("avatar.ownerRequired".into());
    }
    let encoded = owner.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
        out
    });
    let url = route
        .url_template
        .as_ref()
        .ok_or("avatar.noSource")?
        .replace("{owner}", &encoded);
    // No raw context: owner PNGs must never be rewritten as repository files.
    Ok(
        materialize_route(&policy, route, &url, false, None, None, None)?
            .into_iter()
            .map(|(candidate, _)| candidate)
            .collect(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_route<'a>(
    policy: &'a crate::network_policy::NetworkPolicy,
    route: &crate::network_policy::Route,
    url: &str,
    is_raw: bool,
    raw: Option<crate::github_proxies::RawProxyContext<'_>>,
    requested: Option<ProxyMode>,
    selected_id: Option<&str>,
) -> Result<Vec<(DownloadCandidate, Option<&'a str>)>, String> {
    use crate::network_policy::Step;
    let has_query = Url::parse(url).is_ok_and(|parsed| parsed.query().is_some());
    let mut candidates = Vec::new();
    for step in &route.route {
        match step {
            Step::Direct if !matches!(requested, Some(ProxyMode::Fixed)) => candidates.push((
                DownloadCandidate {
                    route: DownloadRoute::Direct,
                    url: url.into(),
                    identity: None,
                },
                None,
            )),
            Step::Proxy { id }
                if !has_query
                    && requested != Some(ProxyMode::Direct)
                    && (requested != Some(ProxyMode::Fixed)
                        || selected_id == Some(id.as_str())) =>
            {
                let proxy = policy.proxies.get(id).ok_or("proxy.validationFailed")?;
                let expanded = crate::github_proxies::proxy_urls(
                    &[proxy.legacy_entry()],
                    url,
                    is_raw,
                    raw.as_ref()
                        .map(|context| crate::github_proxies::RawProxyContext {
                            branch: context.branch,
                            file_path: context.file_path,
                        }),
                );
                candidates.extend(expanded.into_iter().map(|url| {
                    (
                        DownloadCandidate {
                            route: DownloadRoute::Proxy,
                            url,
                            identity: None,
                        },
                        Some(proxy.id.as_str()),
                    )
                }));
            }
            _ => {}
        }
    }
    let mut seen = HashSet::new();
    candidates.retain(|(candidate, _)| seen.insert(candidate.url.clone()));
    if candidates.is_empty() {
        return Err("proxy.noGlobalProxy".into());
    }
    Ok(candidates)
}

#[cfg(test)]
#[path = "planning/tests.rs"]
mod tests;
