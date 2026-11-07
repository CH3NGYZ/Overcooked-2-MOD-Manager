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
    let path = parsed.path();
    if (is_github || is_gitee) && path.contains("/releases/download/") {
        return RemoteUrlKind::ReleaseAsset;
    }
    if host.eq_ignore_ascii_case("raw.githubusercontent.com") {
        return RemoteUrlKind::Raw;
    }
    if (is_github || is_gitee) && is_repository_raw_path(path) {
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
pub(crate) fn provider_for_remote_url(url: &str) -> Provider {
    let host = Url::parse(url.trim())
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase));
    match host.as_deref() {
        Some("gitee.com") => Provider::Gitee,
        _ => Provider::Github,
    }
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
    _region: &str,
) -> ProxyMode {
    if *provider == Provider::Gitee {
        return ProxyMode::Direct;
    }
    requested.unwrap_or(ProxyMode::AutoDirect)
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
    if *provider == Provider::Gitee {
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

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECT: &str = "https://github.com/example/mod/releases/download/v1/mod.zip";

    #[test]
    fn remote_url_kind_follows_host_and_path() {
        // Release 资产：两个平台的 /releases/download/ 路径
        for url in [
            "https://github.com/CH3NGYZ/demo/releases/download/v1.0.0/demo.zip",
            "https://gitee.com/ch3ngyz/demo/releases/download/v1.0.0/demo.zip",
            "https://github.com/CH3NGYZ/demo/releases/download/v1.0.0/sub/demo.dll",
        ] {
            assert_eq!(
                classify_remote_url(url),
                RemoteUrlKind::ReleaseAsset,
                "{url}"
            );
        }
        // Raw：raw.githubusercontent 与两个平台的仓库 raw 入口
        for url in [
            "https://raw.githubusercontent.com/CH3NGYZ/demo/main/README.md",
            "https://github.com/CH3NGYZ/demo/raw/main/maps/plus.zip",
            "https://github.com/CH3NGYZ/demo/raw/refs/heads/main/maps/plus.zip",
            "https://gitee.com/ch3ngyz/demo/raw/main/maps/plus.zip",
        ] {
            assert_eq!(classify_remote_url(url), RemoteUrlKind::Raw, "{url}");
        }
        // 其它直链：普通站点、仓库主页、以及只出现在查询串里的关键词都不改类别
        for url in [
            "https://example.com/files/map.zip",
            "https://github.com/CH3NGYZ/demo",
            "https://github.com/CH3NGYZ/demo/releases",
            "https://example.com/?next=/releases/download/v1/x.zip",
            "https://github.com/CH3NGYZ/demo/blob/main/README.md",
            "ftp://github.com/CH3NGYZ/demo/releases/download/v1/x.zip",
            "not a url",
        ] {
            assert_eq!(classify_remote_url(url), RemoteUrlKind::Other, "{url}");
        }
    }

    #[test]
    fn remote_provider_comes_from_the_url_host() {
        assert_eq!(
            provider_for_remote_url("https://gitee.com/ch3ngyz/demo/raw/main/a.zip"),
            Provider::Gitee
        );
        assert_eq!(
            provider_for_remote_url("https://raw.githubusercontent.com/o/r/main/a.zip"),
            Provider::Github
        );
        assert_eq!(
            provider_for_remote_url("https://example.com/a.zip"),
            Provider::Github
        );
    }

    #[test]
    fn shared_candidate_ordering_deduplicates_each_automatic_mode() {
        let proxy_a = "https://proxy-a.example/asset".to_string();
        let proxy_b = "https://proxy-b.example/asset".to_string();
        let duplicated = vec![
            DIRECT.to_string(),
            proxy_a.clone(),
            proxy_a.clone(),
            proxy_b.clone(),
            DIRECT.to_string(),
        ];
        let direct_first =
            plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, duplicated.clone())
                .unwrap();
        let proxy_first =
            plan_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, duplicated).unwrap();
        assert_eq!(
            direct_first
                .iter()
                .map(|candidate| candidate.url.as_str())
                .collect::<Vec<_>>(),
            [DIRECT, &proxy_a, &proxy_b]
        );
        assert_eq!(
            proxy_first
                .iter()
                .map(|candidate| candidate.url.as_str())
                .collect::<Vec<_>>(),
            [DIRECT, &proxy_a, &proxy_b]
        );
        assert!(plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, Vec::new()).is_err());
    }

    #[test]
    fn remote_urls_pick_their_own_proxy_group_and_gitee_stays_direct() {
        let mut config = crate::config::default_config();
        config.github_proxies.release_proxies = vec![GitHubProxy {
            id: "release-only".into(),
            name: "Release".into(),
            template: "https://release.example/{url}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];
        config.github_proxies.raw_proxies = vec![GitHubProxy {
            id: "raw-only".into(),
            name: "Raw".into(),
            template: "https://raw.example/{owner}/{repo}@{branch}/{path}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];

        // Release 资产只用 release_proxies，raw 组不能混进来
        let release_url = "https://github.com/acme/mod/releases/download/v1/mod.zip";
        let release = plan_remote_download_candidates(
            &config,
            &Provider::Github,
            RemoteUrlKind::ReleaseAsset,
            release_url,
        )
        .unwrap();
        assert!(
            release
                .iter()
                .any(|candidate| candidate.url.contains("release.example"))
        );
        assert!(
            !release
                .iter()
                .any(|candidate| candidate.url.contains("raw.example"))
        );

        // Raw 文件只用 raw_proxies，且模板里的分支/路径来自 URL 自身
        let raw_url = "https://github.com/acme/mod/raw/main/bin/mod.dll";
        let raw = plan_remote_download_candidates(
            &config,
            &Provider::Github,
            RemoteUrlKind::Raw,
            raw_url,
        )
        .unwrap();
        assert!(
            raw.iter()
                .any(|candidate| candidate.url == "https://raw.example/acme/mod@main/bin/mod.dll")
        );
        assert!(
            !raw.iter()
                .any(|candidate| candidate.url.contains("release.example"))
        );

        // Gitee 没有资产代理：只剩直连候选
        let gitee = plan_remote_download_candidates(
            &config,
            &Provider::Gitee,
            RemoteUrlKind::ReleaseAsset,
            "https://gitee.com/acme/mod/releases/download/v1/mod.zip",
        )
        .unwrap();
        assert_eq!(gitee.len(), 1);
        assert_eq!(gitee[0].route, DownloadRoute::Direct);
    }

    fn proxy(id: &str, host: &str, remote: bool, enabled: bool) -> GitHubProxy {
        GitHubProxy {
            id: id.into(),
            name: id.into(),
            template: format!("https://{host}/{{url}}"),
            regions: Vec::new(),
            description: String::new(),
            enabled,
            builtin: !remote,
            remote,
        }
    }

    #[test]
    fn defaults_are_direct_first_in_all_regions_and_gitee_forces_direct() {
        assert_eq!(
            selected_asset_proxy_mode(
                None,
                &Provider::Github,
                crate::config::REGION_CHINESE_MAINLAND
            ),
            ProxyMode::AutoDirect
        );
        assert_eq!(
            selected_asset_proxy_mode(None, &Provider::Github, crate::config::REGION_INTERNATIONAL),
            ProxyMode::AutoDirect
        );
        assert_eq!(
            selected_asset_proxy_mode(
                Some(ProxyMode::AutoProxy),
                &Provider::Gitee,
                crate::config::REGION_CHINESE_MAINLAND
            ),
            ProxyMode::Direct
        );
    }

    #[test]
    fn legacy_auto_proxy_and_auto_direct_use_the_same_order() {
        let proxies = vec![
            "https://proxy-a.example/asset".into(),
            "https://proxy-b.example/asset".into(),
        ];
        let direct_first =
            plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, proxies.clone()).unwrap();
        assert_eq!(
            direct_first
                .iter()
                .map(|item| item.route)
                .collect::<Vec<_>>(),
            [
                DownloadRoute::Direct,
                DownloadRoute::Proxy,
                DownloadRoute::Proxy
            ]
        );
        assert_eq!(direct_first[0].url, DIRECT);
        let proxy_first =
            plan_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, proxies).unwrap();
        assert_eq!(
            proxy_first
                .iter()
                .map(|item| item.route)
                .collect::<Vec<_>>(),
            [
                DownloadRoute::Direct,
                DownloadRoute::Proxy,
                DownloadRoute::Proxy
            ]
        );
        assert_eq!(proxy_first[0].url, DIRECT);
        assert_eq!(proxy_first, direct_first);
    }

    #[test]
    fn automatic_proxy_preserves_manifest_order_without_origin_priority() {
        let entries = vec![
            proxy("bundled", "bundled.example", false, true),
            proxy("disabled", "disabled.example", true, false),
            proxy("remote", "remote.example", true, true),
        ];
        let urls = asset_proxy_urls(
            &entries,
            DIRECT,
            false,
            "",
            "",
            ProxyMode::AutoProxy,
            Some("bundled"),
        );
        assert_eq!(urls.len(), 2);
        assert!(urls[0].starts_with("https://bundled.example/"));
        assert!(urls[1].starts_with("https://remote.example/"));

        let bundled_only = asset_proxy_urls(
            &entries[..1],
            DIRECT,
            false,
            "",
            "",
            ProxyMode::AutoProxy,
            None,
        );
        assert_eq!(bundled_only.len(), 1);
        assert!(bundled_only[0].starts_with("https://bundled.example/"));
    }

    #[test]
    fn fixed_mode_uses_only_selected_enabled_proxy() {
        let entries = vec![
            proxy("first", "first.example", true, true),
            proxy("selected", "selected.example", false, true),
            proxy("disabled", "disabled.example", false, false),
        ];
        let selected = asset_proxy_urls(
            &entries,
            DIRECT,
            false,
            "",
            "",
            ProxyMode::Fixed,
            Some("selected"),
        );
        let candidates =
            plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, selected).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].route, DownloadRoute::Proxy);
        assert!(candidates[0].url.starts_with("https://selected.example/"));
        assert!(plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, Vec::new()).is_err());
        assert!(
            asset_proxy_urls(
                &entries,
                DIRECT,
                false,
                "",
                "",
                ProxyMode::Fixed,
                Some("disabled")
            )
            .is_empty()
        );
        for selected_id in [None, Some(" ")] {
            assert!(
                asset_proxy_urls(
                    &entries,
                    DIRECT,
                    false,
                    "",
                    "",
                    ProxyMode::Fixed,
                    selected_id
                )
                .is_empty()
            );
        }
    }
}
