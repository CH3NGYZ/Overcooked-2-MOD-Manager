use crate::models::{GitHubProxy, GitHubProxyConfig, GitHubProxyManifest};
use std::collections::HashSet;
use std::sync::OnceLock;
use url::Url;

const BUNDLED_PROXY_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/repositories-proxies.toml"
));
const MAX_PROXY_COUNT: usize = 128;
const PLACEHOLDERS: &[&str] = &[
    "url",
    "url_no_scheme",
    "github_path",
    "owner",
    "repo",
    "tag",
    "asset",
    "branch",
    "path",
    "url_path",
];

pub fn bundled_defaults() -> GitHubProxyConfig {
    static DEFAULTS: OnceLock<GitHubProxyConfig> = OnceLock::new();
    DEFAULTS.get_or_init(parse_bundled_defaults).clone()
}

fn parse_bundled_defaults() -> GitHubProxyConfig {
    let config = match parse_manifest(BUNDLED_PROXY_MANIFEST) {
        Ok(config) => config,
        Err(error) => {
            log::error!("内置 GitHub 代理清单解析失败：{}", error);
            GitHubProxyConfig::default()
        }
    };
    log::info!(
        "内置代理加载完成：Release={}，Raw={}，GitHub API={}，Gitee API={}，GitCode API={}",
        config.release_proxies.len(),
        config.raw_proxies.len(),
        config.github_api_proxies.len(),
        config.gitee_api_proxies.len(),
        config.gitcode_api_proxies.len()
    );
    config
}

pub fn parse_manifest(text: &str) -> Result<GitHubProxyConfig, String> {
    log::debug!("开始解析 GitHub 代理 TOML 清单：字节数={}", text.len());
    if text.len() > 1024 * 1024 {
        return Err("proxy.manifestTooLarge".into());
    }
    let document: toml::Value = toml::from_str(text).map_err(|_| "proxy.validationFailed")?;
    if ![
        "schema_version",
        "release_proxies",
        "raw_proxies",
        "github_api_proxies",
        "gitee_api_proxies",
        "gitcode_api_proxies",
        "avatar_proxies",
    ]
    .iter()
    .any(|key| document.get(key).is_some())
    {
        return Err("proxy.validationFailed".into());
    }
    let mut manifest: GitHubProxyManifest =
        document.try_into().map_err(|error: toml::de::Error| {
            crate::locale_tag::with_params(
                "proxy.manifestParseFailed",
                &[("reason", &error.to_string())],
            )
        })?;
    if manifest.schema_version != 1 {
        return Err(crate::locale_tag::with_params(
            "proxy.unsupportedManifestVersion",
            &[("version", &manifest.schema_version.to_string())],
        ));
    }
    for proxy in manifest
        .release_proxies
        .iter_mut()
        .chain(manifest.raw_proxies.iter_mut())
        .chain(manifest.github_api_proxies.iter_mut())
        .chain(manifest.gitee_api_proxies.iter_mut())
        .chain(manifest.gitcode_api_proxies.iter_mut())
        .chain(manifest.avatar_proxies.iter_mut())
    {
        proxy.builtin = true;
        proxy.remote = false;
        normalize_proxy(proxy)?;
    }
    if manifest.release_proxies.len() > MAX_PROXY_COUNT
        || manifest.raw_proxies.len() > MAX_PROXY_COUNT
        || manifest.github_api_proxies.len() > MAX_PROXY_COUNT
        || manifest.gitee_api_proxies.len() > MAX_PROXY_COUNT
        || manifest.gitcode_api_proxies.len() > MAX_PROXY_COUNT
        || manifest.avatar_proxies.len() > MAX_PROXY_COUNT
    {
        return Err(crate::locale_tag::with_params(
            "proxy.tooManyEntries",
            &[("max", &MAX_PROXY_COUNT.to_string())],
        ));
    }
    let config = GitHubProxyConfig {
        release_proxies: deduplicate(manifest.release_proxies),
        raw_proxies: deduplicate(manifest.raw_proxies),
        github_api_proxies: deduplicate(manifest.github_api_proxies),
        gitee_api_proxies: deduplicate(manifest.gitee_api_proxies),
        gitcode_api_proxies: deduplicate(manifest.gitcode_api_proxies),
        avatar_proxies: deduplicate(manifest.avatar_proxies),
    };
    validate_unique_ids(&config)?;
    log::info!(
        "GitHub 代理 TOML 清单解析完成：Release={}，Raw={}，GitHub API={}，Gitee API={}，GitCode API={}",
        config.release_proxies.len(),
        config.raw_proxies.len(),
        config.github_api_proxies.len(),
        config.gitee_api_proxies.len(),
        config.gitcode_api_proxies.len()
    );
    Ok(config)
}

fn normalize_proxy(proxy: &mut GitHubProxy) -> Result<(), String> {
    proxy.id = proxy.id.trim().to_string();
    proxy.name = proxy.name.trim().to_string();
    proxy.template = proxy.template.trim().to_string();
    proxy.description = proxy.description.trim().to_string();
    proxy.regions = proxy
        .regions
        .iter()
        .map(|region| region.trim().to_string())
        .filter(|region| !region.is_empty())
        .collect();
    proxy.regions.sort_by_key(|region| region.to_lowercase());
    proxy
        .regions
        .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    if proxy.id.is_empty() || proxy.name.is_empty() || proxy.template.is_empty() {
        return Err("proxy.fieldsRequired".into());
    }
    let mut probe = String::with_capacity(proxy.template.len());
    let mut rest = proxy.template.as_str();
    while let Some(start) = rest.find('{') {
        probe.push_str(&rest[..start]);
        let token_start = start + 1;
        let end = rest[token_start..]
            .find('}')
            .map(|offset| token_start + offset)
            .ok_or_else(|| "proxy.unclosedPlaceholder".to_string())?;
        let token = &rest[token_start..end];
        if !PLACEHOLDERS.contains(&token) {
            return Err(crate::locale_tag::with_params(
                "proxy.unsupportedPlaceholder",
                &[("token", &token.to_string())],
            ));
        }
        probe.push('x');
        rest = &rest[end + 1..];
    }
    probe.push_str(rest);
    if probe.contains('}') {
        return Err("proxy.invalidBraces".into());
    }
    let parsed = Url::parse(&probe).map_err(|error| {
        crate::locale_tag::with_params(
            "proxy.templateUrlInvalid",
            &[("reason", &error.to_string())],
        )
    })?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() || parsed.authority().contains('@')
    {
        return Err("proxy.templateMustBeHttps".into());
    }
    Ok(())
}

fn template_key(proxy: &GitHubProxy) -> String {
    proxy
        .template
        .trim()
        .trim_end_matches('/')
        .replace("/{url_path}", "{url_path}")
        .to_ascii_lowercase()
}

fn deduplicate(proxies: Vec<GitHubProxy>) -> Vec<GitHubProxy> {
    let mut unique = Vec::<GitHubProxy>::new();
    let mut indexes = std::collections::HashMap::<String, usize>::new();
    for proxy in proxies {
        let key = template_key(&proxy);
        if let Some(index) = indexes.get(&key).copied() {
            let existing = &mut unique[index];
            for region in proxy.regions {
                if !existing
                    .regions
                    .iter()
                    .any(|current| current.eq_ignore_ascii_case(&region))
                {
                    existing.regions.push(region);
                }
            }
            if existing.description.is_empty() {
                existing.description = proxy.description;
            }
            // Duplicate declarations cannot override an explicit disable.
            existing.enabled &= proxy.enabled;
            existing.builtin |= proxy.builtin;
            existing.remote |= proxy.remote;
        } else {
            indexes.insert(key, unique.len());
            unique.push(proxy);
        }
    }
    unique
}

pub struct RawProxyContext<'a> {
    pub branch: &'a str,
    pub file_path: &'a str,
}

pub fn proxy_urls(
    proxies: &[GitHubProxy],
    target: &str,
    is_raw: bool,
    raw_context: Option<RawProxyContext<'_>>,
) -> Vec<String> {
    let (url, owner, repo, branch, path, tag, asset) = proxy_context(target, is_raw, raw_context);
    let mut seen = HashSet::new();
    let urls = proxies
        .iter()
        .filter(|proxy| proxy.enabled)
        .filter_map(|proxy| {
            let mut rendered = proxy.template.clone();
            let url_path = github_path(target);
            let target_url_path = if is_raw {
                raw_github_path(&url)
            } else {
                target_url_path(target)
            };
            if target_url_path.starts_with('/') {
                rendered = rendered.replace("/{url_path}", "{url_path}");
            }
            // URL-derived segments are already encoded; explicit context is encoded once below.
            let encoded_branch = &branch;
            let encoded_path = &path;
            let values = [
                ("url", url.as_str()),
                ("url_no_scheme", url.trim_start_matches("https://")),
                ("github_path", url_path.as_str()),
                ("owner", owner.as_str()),
                ("repo", repo.as_str()),
                ("tag", tag.as_str()),
                ("asset", asset.as_str()),
                ("branch", encoded_branch.as_str()),
                ("path", encoded_path.as_str()),
                ("url_path", target_url_path.as_str()),
            ];
            for (key, value) in values {
                rendered = rendered.replace(&format!("{{{key}}}"), value);
            }
            Url::parse(&rendered)
                .ok()
                .filter(|parsed| parsed.scheme() == "https")
                .map(|_| rendered)
        })
        .filter(|url| seen.insert(url.clone()))
        .collect::<Vec<_>>();
    log::trace!(
        "生成代理地址：代理候选={}，可用地址={}，Raw={}",
        proxies.len(),
        urls.len(),
        is_raw
    );
    urls
}

/// 从 raw 文件 URL 里取出分支与文件路径（供代理模板的 `{branch}` / `{path}` 展开）。
///
/// 支持两种仓库 raw 入口：
/// - `https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}`
/// - `https://github.com` / `https://gitee.com` 的 `/{owner}/{repo}/raw/{ref}/{path}`
///   （`raw/refs/heads/{branch}/{path}` 也接受）
///
/// 仓库文件模式下调用方本来就知道分支与路径；只有 URL 可用时（直链下载），
/// 这里是唯一的推导实现。
pub(crate) fn raw_branch_and_path(target: &str) -> (String, String) {
    let Ok(url) = Url::parse(target) else {
        return (String::new(), String::new());
    };
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    raw_branch_and_path_segments(url.host_str().unwrap_or_default(), &segments)
}

fn raw_branch_and_path_segments(host: &str, segments: &[&str]) -> (String, String) {
    let branch_index = if host.eq_ignore_ascii_case("raw.githubusercontent.com") {
        // 这个域名没有 `raw` 段：分支紧跟在 owner/repo 之后。
        if segments.get(2) == Some(&"refs") && segments.get(3) == Some(&"heads") {
            4
        } else {
            2
        }
    } else {
        if segments.get(2) != Some(&"raw") {
            return (String::new(), String::new());
        }
        let raw_index = 2;
        if segments.get(raw_index + 1) == Some(&"refs")
            && segments.get(raw_index + 2) == Some(&"heads")
        {
            raw_index + 3
        } else {
            raw_index + 1
        }
    };
    let branch = segments
        .get(branch_index)
        .copied()
        .unwrap_or_default()
        .to_string();
    let path = segments
        .get(branch_index + 1..)
        .unwrap_or_default()
        .join("/");
    (branch, path)
}

fn proxy_context(
    target: &str,
    is_raw: bool,
    raw_context: Option<RawProxyContext<'_>>,
) -> (String, String, String, String, String, String, String) {
    let mut owner = String::new();
    let mut repo = String::new();
    let mut branch = String::new();
    let mut path = String::new();
    let mut tag = String::new();
    let mut asset = String::new();
    if let Ok(url) = Url::parse(target) {
        let segments = url
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .unwrap_or_default();
        owner = segments.first().copied().unwrap_or_default().to_string();
        repo = segments.get(1).copied().unwrap_or_default().to_string();
        if is_raw {
            // 仓库配置已知分支与文件路径时由调用方的 `RawProxyContext` 覆盖；
            // 只拿到 URL（直链下载）时用同一套推导，避免两处各写一遍。
            (branch, path) = raw_branch_and_path(target);
        } else if let Some(download_index) = segments.iter().position(|part| *part == "download") {
            tag = segments
                .get(download_index + 1)
                .copied()
                .unwrap_or_default()
                .to_string();
            asset = segments
                .get(download_index + 2..)
                .unwrap_or_default()
                .join("/");
        } else if let Some(archive_index) = segments.iter().position(|part| *part == "archive") {
            if segments.get(archive_index + 1) == Some(&"refs")
                && segments.get(archive_index + 2) == Some(&"tags")
            {
                tag = segments
                    .get(archive_index + 3)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
                asset = segments
                    .get(archive_index + 4)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
            }
        }
    }
    if is_raw && let Some(context) = raw_context {
        branch = encode_slash_path(context.branch);
        path = encode_slash_path(context.file_path);
    }
    let canonical = if is_raw {
        let raw_branch = if branch.is_empty() { "main" } else { &branch };
        let mut canonical =
            format!("https://raw.githubusercontent.com/{owner}/{repo}/{raw_branch}/{path}");
        if let Some(query) = Url::parse(target)
            .ok()
            .and_then(|url| url.query().map(str::to_owned))
        {
            canonical.push('?');
            canonical.push_str(&query);
        }
        canonical
    } else {
        target.to_string()
    };
    (canonical, owner, repo, branch, path, tag, asset)
}

/// Host-replacement proxies consume GitHub paths, regardless of the original Raw host.
fn raw_github_path(canonical: &str) -> String {
    let Ok(url) = Url::parse(canonical) else {
        return String::new();
    };
    let parts = url
        .path()
        .trim_start_matches('/')
        .splitn(3, '/')
        .collect::<Vec<_>>();
    if parts.len() != 3 {
        return target_url_path(canonical);
    }
    let rest = parts[2];
    let qualified = rest.starts_with("refs/")
        || rest
            .split('/')
            .next()
            .is_some_and(|part| part.len() == 40 && part.bytes().all(|b| b.is_ascii_hexdigit()));
    let prefix = if qualified { "raw" } else { "raw/refs/heads" };
    let mut path = format!("/{}/{}/{prefix}/{rest}", parts[0], parts[1]);
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    path
}

fn encode_slash_path(value: &str) -> String {
    let mut url = Url::parse("https://path.invalid").expect("static URL is valid");
    {
        let mut segments = url.path_segments_mut().expect("URL supports path segments");
        for segment in value.split('/') {
            segments.push(segment);
        }
    }
    url.path().trim_start_matches('/').to_string()
}

fn github_path(target: &str) -> String {
    Url::parse(target)
        .ok()
        .map(|url| url.path().trim_start_matches('/').to_string())
        .unwrap_or_default()
}

fn target_url_path(target: &str) -> String {
    let Ok(url) = Url::parse(target) else {
        return String::new();
    };
    let mut path = url.path().to_string();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    path
}

pub fn validate_unique_ids(config: &GitHubProxyConfig) -> Result<(), String> {
    for proxies in [
        &config.release_proxies,
        &config.raw_proxies,
        &config.github_api_proxies,
        &config.gitee_api_proxies,
        &config.gitcode_api_proxies,
        &config.avatar_proxies,
    ] {
        let mut ids = HashSet::new();
        for proxy in proxies {
            if !ids.insert(proxy.id.to_ascii_lowercase()) {
                return Err(crate::locale_tag::with_params(
                    "proxy.duplicateId",
                    &[("id", &proxy.id.to_string())],
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "github_proxies/tests.rs"]
mod tests;
