use super::DownloadCandidate;
use crate::archive::DownloadRoute;
use crate::models::{AppConfig, GitHubProxy, Provider, ProxyMode};
use std::collections::HashSet;
use url::Url;

/// 远程 URL 的下载类别。
///
/// 三类走完全不同的路由：Release 资产只认 `release_proxies`，仓库 raw
/// 文件认 `raw_proxies` 且代理模板要用分支与文件路径展开，其它直链没有
/// 代理可用、直接单源多线程。判定结果只决定「用哪套代理与线程策略」，
/// 不决定 URL 是否可信（那由下载入口校验）。
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
    let is_github = host.eq_ignore_ascii_case("github.com");
    let is_gitee = host.eq_ignore_ascii_case("gitee.com");
    let is_gitcode = matches!(host, "gitcode.com" | "atomgit.com");
    let path = parsed.path();
    if (is_github || is_gitee || is_gitcode) && path.contains("/releases/download/") {
        return RemoteUrlKind::ReleaseAsset;
    }
    if matches!(
        host,
        "raw.githubusercontent.com" | "raw.atomgit.com" | "raw.gitcode.com"
    ) {
        return RemoteUrlKind::Raw;
    }
    if (is_github || is_gitee || is_gitcode) && is_repository_raw_path(path) {
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

// Update and MOD downloads share candidate ordering, routing and deduplication.
// Callers supply the proxy snapshot available at their point in startup.
pub(crate) fn ordered_asset_download_candidates(
    mode: ProxyMode,
    direct_url: &str,
    proxy_urls: Vec<String>,
) -> Vec<DownloadCandidate> {
    let direct = || DownloadCandidate {
        route: DownloadRoute::Direct,
        url: direct_url.to_string(),
    };
    let proxies = proxy_urls
        .into_iter()
        .filter(|url| url != direct_url)
        .map(|url| DownloadCandidate {
            route: DownloadRoute::Proxy,
            url,
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

pub(crate) fn selected_asset_proxy_mode(
    requested: Option<ProxyMode>,
    provider: &Provider,
    region: &str,
) -> ProxyMode {
    if matches!(provider, Provider::Gitee | Provider::Gitcode) {
        return ProxyMode::Direct;
    }
    requested.unwrap_or_else(|| {
        if region == crate::config::REGION_INTERNATIONAL {
            ProxyMode::Direct
        } else {
            ProxyMode::AutoDirect
        }
    })
}

/// File concurrency is shared by MOD, map and pasted-URL downloads. A direct
/// Gitee/GitCode URL must not accidentally inherit the generic 16/32 chunks.
/// Concurrent complete-file candidates never also open per-source chunks.
pub(crate) fn file_download_threads(
    provider: Option<&Provider>,
    region: &str,
    configured: u32,
    racing: bool,
) -> u32 {
    if racing
        || matches!(provider, Some(Provider::Gitee | Provider::Gitcode))
        || (provider == Some(&Provider::Github) && region == crate::config::REGION_INTERNATIONAL)
    {
        1
    } else {
        configured.clamp(1, 32)
    }
}

pub(crate) fn asset_proxy_urls(
    entries: &[GitHubProxy],
    direct_url: &str,
    is_raw_file: bool,
    branch: &str,
    file_path: &str,
    mode: ProxyMode,
    selected_id: Option<&str>,
) -> Vec<String> {
    if mode == ProxyMode::Direct {
        return Vec::new();
    }
    let entries = entries
        .iter()
        .filter(|proxy| {
            mode != ProxyMode::Fixed
                || selected_id.is_some_and(|id| !id.trim().is_empty() && proxy.id == id)
        })
        .cloned()
        .collect::<Vec<_>>();
    crate::github_proxies::proxy_urls(
        &entries,
        direct_url,
        is_raw_file,
        (is_raw_file && !branch.is_empty())
            .then_some(crate::github_proxies::RawProxyContext { branch, file_path }),
    )
}

/// 按平台与文件类型选代理组。
///
/// Gitee 没有代理（只有 API 代理，资产直连）；GitHub 的 Release 资产与
/// raw 文件各用一组。MOD 仓库安装、地图 Release 安装与通用直链下载共用
/// 这份选择，避免每个调用点各写一遍「哪些字段是 Release 需要的」。
pub(crate) fn provider_proxy_entries<'a>(
    config: &'a AppConfig,
    provider: &Provider,
    is_raw_file: bool,
) -> &'a [GitHubProxy] {
    if matches!(provider, Provider::Gitee | Provider::Gitcode) {
        return &[];
    }
    if is_raw_file {
        &config.github_proxies.raw_proxies
    } else {
        &config.github_proxies.release_proxies
    }
}

/// 为一个远程 URL 规划完整文件下载候选（含代理展开与去重）。
///
/// 仓库/地图安装与直链下载共用同一套候选规则；代理模式按平台与地区取
/// [`selected_asset_proxy_mode`] 的默认值（Gitee 恒直连）。返回值按实际
/// 启用情况排序，长度 > 1 即表示调用方应当竞速。
pub(crate) fn plan_remote_download_candidates(
    config: &AppConfig,
    provider: &Provider,
    kind: RemoteUrlKind,
    url: &str,
) -> Result<Vec<DownloadCandidate>, String> {
    let mode = selected_asset_proxy_mode(None, provider, crate::config::current_user_region());
    let is_raw_file = kind == RemoteUrlKind::Raw;
    let entries = provider_proxy_entries(config, provider, is_raw_file);
    let proxy_urls = asset_proxy_urls(entries, url, is_raw_file, "", "", mode, None);
    plan_asset_download_candidates(mode, url, proxy_urls)
}

pub(crate) fn plan_asset_download_candidates(
    mode: ProxyMode,
    direct_url: &str,
    proxy_urls: Vec<String>,
) -> Result<Vec<DownloadCandidate>, String> {
    let candidates = ordered_asset_download_candidates(mode, direct_url, proxy_urls);
    if mode == ProxyMode::Fixed && candidates.is_empty() {
        return Err("proxy.noGlobalProxy".into());
    }
    Ok(candidates)
}

/// Materialized file route. The same resolved plan controls both whole-file
/// concurrency and per-candidate chunks; source fields cannot override it later.
pub(crate) struct RepositoryDownloadPlan {
    pub candidates: Vec<DownloadCandidate>,
    pub download_threads: u32,
    pub execution: super::DownloadExecution,
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
    use crate::network_policy::{Intent, NetworkPolicy};
    let policy = NetworkPolicy::shared()?;
    let plan = policy.resolve(provider, kind, region, selection)?;
    let intent = if raw.is_some() {
        Intent::Raw
    } else {
        Intent::Release
    };
    let route = plan.route(intent)?;
    let candidates = materialize_route(policy, route, url, raw, requested, selected_id)?;
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
    let policy = NetworkPolicy::shared()?;
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
    materialize_route(policy, route, &url, None, None, None)
}

fn materialize_route(
    policy: &crate::network_policy::NetworkPolicy,
    route: &crate::network_policy::Route,
    url: &str,
    raw: Option<crate::github_proxies::RawProxyContext<'_>>,
    requested: Option<ProxyMode>,
    selected_id: Option<&str>,
) -> Result<Vec<DownloadCandidate>, String> {
    use crate::network_policy::Step;
    let mut candidates = Vec::new();
    for step in &route.route {
        match step {
            Step::Direct if !matches!(requested, Some(ProxyMode::Fixed)) => {
                candidates.push(DownloadCandidate {
                    route: DownloadRoute::Direct,
                    url: url.into(),
                })
            }
            Step::Proxy { id }
                if requested != Some(ProxyMode::Direct)
                    && (requested != Some(ProxyMode::Fixed)
                        || selected_id == Some(id.as_str())) =>
            {
                let proxy = policy.proxies.get(id).ok_or("proxy.validationFailed")?;
                let expanded = crate::github_proxies::proxy_urls(
                    &[proxy.legacy_entry()],
                    url,
                    raw.is_some(),
                    raw.as_ref()
                        .map(|context| crate::github_proxies::RawProxyContext {
                            branch: context.branch,
                            file_path: context.file_path,
                        }),
                );
                candidates.extend(expanded.into_iter().map(|url| DownloadCandidate {
                    route: DownloadRoute::Proxy,
                    url,
                }));
            }
            _ => {}
        }
    }
    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.url.clone()));
    if candidates.is_empty() {
        return Err("proxy.noGlobalProxy".into());
    }
    Ok(candidates)
}

#[cfg(test)]
#[path = "planning/tests.rs"]
mod tests;
