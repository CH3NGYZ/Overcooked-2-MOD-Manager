use crate::models::{
    AssetRule, AssetRuleType, Channel, Provider, ProxyMode, ReleaseAsset, ReleaseInfo,
    RepositoryConfig,
};
use globset::GlobBuilder;
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use url::Url;

/// Stable identifier for Cloudflare rules and access logs. The package
/// version makes every request identify the exact desktop client release.
pub const APP_USER_AGENT: &str = concat!(
    "Overcooked 2 Mod Manager/",
    env!("CARGO_PKG_VERSION"),
    " (client=desktop; os=Windows; arch=x64; tauri=2)"
);

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("Release operation failed: {0}")]
    InvalidUrl(String),
    #[error("Release operation failed: {0}")]
    Http(String),
    #[error("Release operation failed: {0}")]
    Status(StatusCode),
    #[error("Release operation failed: {0}")]
    Json(String),
}

pub fn provider_from_url(value: &str) -> Option<Provider> {
    let host = Url::parse(value).ok()?.host_str()?.to_ascii_lowercase();
    if host == "github.com" || host.ends_with(".github.com") {
        Some(Provider::Github)
    } else if host == "gitee.com" || host.ends_with(".gitee.com") {
        Some(Provider::Gitee)
    } else {
        None
    }
}

pub fn repository_path(url: &str, provider: &Provider) -> Result<(String, String), ReleaseError> {
    let parsed = Url::parse(url).map_err(|e| ReleaseError::InvalidUrl(e.to_string()))?;
    let segments = parsed
        .path_segments()
        .ok_or_else(|| ReleaseError::InvalidUrl("缺少仓库路径".into()))?
        .filter(|segment| !segment.is_empty() && *segment != "releases")
        .take(2)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.len() != 2 {
        return Err(ReleaseError::InvalidUrl("无法解析 owner/repository".into()));
    }
    if provider_from_url(url) != Some(provider.clone()) {
        return Err(ReleaseError::InvalidUrl("Provider 与 URL 不匹配".into()));
    }
    Ok((
        segments[0].clone(),
        segments[1].trim_end_matches(".git").to_string(),
    ))
}

pub fn parse_version(tag: &str) -> Option<semver::Version> {
    let cleaned = tag.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(cleaned).ok().or_else(|| {
        let re = Regex::new(r"(\d+(?:\.\d+){1,3}(?:[-+][0-9A-Za-z.-]+)?)").ok()?;
        let value = re.captures(cleaned)?.get(1)?.as_str();
        let numeric = value.split(['-', '+']).next()?;
        let parts = numeric
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let suffix = value.get(numeric.len()..).unwrap_or_default();
        let normalized = match parts.as_slice() {
            [major, minor] => format!("{major}.{minor}.0{suffix}"),
            [major, minor, patch] => format!("{major}.{minor}.{patch}{suffix}"),
            [major, minor, patch, build] => format!("{major}.{minor}.{patch}-{build}{suffix}"),
            _ => return None,
        };
        semver::Version::parse(&normalized).ok()
    })
}

pub fn filter_channel(mut releases: Vec<ReleaseInfo>, channel: &Channel) -> Vec<ReleaseInfo> {
    releases.retain(|release| {
        !release.draft
            && match channel {
                Channel::Stable => !release.prerelease,
                Channel::Beta => release.prerelease,
                Channel::All => true,
            }
    });
    releases.sort_by(|a, b| {
        match (
            a.version.as_deref().and_then(parse_version),
            b.version.as_deref().and_then(parse_version),
        ) {
            (Some(left), Some(right)) => right.cmp(&left),
            _ => b.published_at.cmp(&a.published_at),
        }
    });
    releases
}

pub fn asset_matches(asset: &ReleaseAsset, rules: &[AssetRule]) -> bool {
    rules.iter().any(|rule| match rule.rule_type {
        AssetRuleType::Exact => asset.name.eq_ignore_ascii_case(&rule.value),
        AssetRuleType::Glob => GlobBuilder::new(&rule.value)
            .case_insensitive(true)
            .build()
            .ok()
            .map(|glob| glob.compile_matcher().is_match(&asset.name))
            .unwrap_or(false),
        AssetRuleType::Regex => Regex::new(&rule.value)
            .ok()
            .map(|regex| regex.is_match(&asset.name))
            .unwrap_or(false),
    })
}

pub fn rewrite_proxy(proxy_base: &str, target: &str) -> String {
    if proxy_base.contains("{url}") {
        return proxy_base.replace("{url}", target);
    }
    let proxy = proxy_base.trim_end_matches('/');
    if let Ok(url) = Url::parse(target) {
        let suffix = match url.query() {
            Some(query) => format!("{}?{}", url.path(), query),
            None => url.path().to_string(),
        };
        format!("{proxy}{suffix}")
    } else {
        format!("{proxy}/{target}")
    }
}

pub async fn fetch_releases(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    official_api_only: bool,
    max_release_count: u32,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let (owner, name) = repository_path(&repo.url, &repo.provider)?;
    let official = match repo.provider {
        Provider::Github => format!(
            "https://api.github.com/repos/{owner}/{name}/releases?per_page={max_release_count}&direction=desc&sort=created"
        ),
        Provider::Gitee => format!(
            "https://gitee.com/api/v5/repos/{owner}/{name}/releases?per_page={max_release_count}&direction=desc&sort=created"
        ),
    };
    let proxy_url = (!repo.api_proxy_url.trim().is_empty())
        .then(|| rewrite_proxy(&repo.api_proxy_url, &official));
    let direct = fetch_release_url_with_token(client, &official, &repo.provider, token).await;
    if matches!(repo.api_proxy_mode, ProxyMode::Direct) || proxy_url.is_none() {
        return direct;
    }
    if matches!(repo.api_proxy_mode, ProxyMode::Auto) && direct.is_ok() {
        return direct;
    }
    let proxy_token = token.filter(|_| !official_api_only);
    fetch_release_url_with_token(
        client,
        proxy_url.as_deref().expect("proxy checked"),
        &repo.provider,
        proxy_token,
    )
    .await
}

/// Fetches a release list from an already selected API endpoint. This is used
/// by the application updater so its provider/proxy order is explicit.
pub async fn fetch_release_url_with_token(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let mut request = client.get(url).header("User-Agent", APP_USER_AGENT);
    if let Some(token) = token {
        request = match provider {
            Provider::Github => request.bearer_auth(token),
            Provider::Gitee => request.header("PRIVATE-TOKEN", token),
        };
    }
    let response = request
        .send()
        .await
        .map_err(|e| ReleaseError::Http(e.to_string()))?;
    if !response.status().is_success() {
        return Err(ReleaseError::Status(response.status()));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| ReleaseError::Json(e.to_string()))?;
    parse_release_json(&value, provider)
}

pub async fn fetch_gitee_asset_sizes(
    client: &Client,
    repo: &RepositoryConfig,
    release_id: u64,
    token: Option<&str>,
    official_api_only: bool,
) -> Result<Vec<ReleaseAsset>, ReleaseError> {
    if repo.provider != Provider::Gitee {
        return Err(ReleaseError::InvalidUrl("只有 Gitee Release 需要单独请求附件大小".into()));
    }
    let (owner, name) = repository_path(&repo.url, &repo.provider)?;
    let official = format!(
        "https://gitee.com/api/v5/repos/{owner}/{name}/releases/{release_id}/attach_files"
    );
    let proxy_url = (!repo.api_proxy_url.trim().is_empty())
        .then(|| rewrite_proxy(&repo.api_proxy_url, &official));
    let direct = fetch_gitee_asset_sizes_url(client, &official, token).await;
    if matches!(repo.api_proxy_mode, ProxyMode::Direct) || proxy_url.is_none() {
        return direct;
    }
    if matches!(repo.api_proxy_mode, ProxyMode::Auto) && direct.is_ok() {
        return direct;
    }
    let proxy_token = token.filter(|_| !official_api_only);
    fetch_gitee_asset_sizes_url(
        client,
        proxy_url.as_deref().expect("proxy checked"),
        proxy_token,
    )
    .await
}

async fn fetch_gitee_asset_sizes_url(
    client: &Client,
    url: &str,
    token: Option<&str>,
) -> Result<Vec<ReleaseAsset>, ReleaseError> {
    let mut request = client.get(url).header("User-Agent", APP_USER_AGENT);
    if let Some(token) = token {
        request = request.header("PRIVATE-TOKEN", token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| ReleaseError::Http(e.to_string()))?;
    if !response.status().is_success() {
        return Err(ReleaseError::Status(response.status()));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| ReleaseError::Json(e.to_string()))?;
    let array = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .ok_or_else(|| ReleaseError::Json("Gitee 附件响应不是数组".into()))?;
    array
        .iter()
        .map(|asset| {
            let name = asset
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ReleaseError::Json("Gitee 附件缺少文件名".into()))?
                .to_string();
            let download_url = ["root", "browser_download_url", "download_url", "url"]
                .iter()
                .filter_map(|key| asset.get(*key).and_then(Value::as_str))
                .find(|url| !url.trim().is_empty())
                .ok_or_else(|| ReleaseError::Json("Gitee 附件缺少下载地址".into()))?
                .to_string();
            let size = asset
                .get("size")
                .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
                .unwrap_or_default();
            Ok(ReleaseAsset {
                name,
                size,
                download_url,
                digest: None,
            })
        })
        .collect()
}

pub fn parse_release_json(
    value: &Value,
    provider: &Provider,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let owned;
    let array = if let Some(array) = value.as_array() {
        array
    } else if value.is_object() {
        owned = vec![value.clone()];
        &owned
    } else {
        return Err(ReleaseError::Json("Release 响应不是数组或对象".into()));
    };
    let mut releases = Vec::new();
    for item in array {
        let tag = item
            .get("tag_name")
            .or_else(|| item.get("tagName"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if tag.is_empty() {
            continue;
        }
        let assets = item
            .get("assets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|asset| {
                let name = asset.get("name").and_then(Value::as_str)?.to_string();
                let download_url = [
                    "browser_download_url",
                    "browserDownloadUrl",
                    "download_url",
                    "downloadUrl",
                    "url",
                ]
                .iter()
                .filter_map(|key| asset.get(*key).and_then(Value::as_str))
                .find(|url| !url.trim().is_empty())?
                .to_string();
                let digest = asset
                    .get("digest")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        asset
                            .get("sha256")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                Some(ReleaseAsset {
                    name,
                    size: asset
                        .get("size")
                        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
                        .unwrap_or(0),
                    download_url,
                    digest,
                })
            })
            .collect();
        let prerelease = item
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                tag.to_ascii_lowercase().contains("beta")
                    || tag.to_ascii_lowercase().contains("alpha")
            });
        let date = item
            .get("published_at")
            .or_else(|| item.get("created_at"))
            .and_then(Value::as_str)
            .map(str::to_string);
        releases.push(ReleaseInfo {
            release_id: item
                .get("id")
                .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok())),
            version: parse_version(&tag).map(|value| value.to_string()),
            tag_name: tag,
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            body: item
                .get("body")
                .or_else(|| item.get("description"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            published_at: date,
            prerelease,
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
            assets,
        });
    }
    let _ = provider;
    Ok(releases)
}

pub fn build_client(_timeout_seconds: u64) -> Result<Client, ReleaseError> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| ReleaseError::Http(e.to_string()))
}

/// Builds a client for potentially large release assets. The total request
/// timeout is disabled so a download is allowed to run as long as data keeps
/// arriving. The read timeout still aborts a connection that has gone idle.
pub fn build_download_client(timeout_seconds: u64) -> Result<Client, ReleaseError> {
    let connect_timeout = std::time::Duration::from_secs(timeout_seconds.max(1));
    Client::builder()
        .connect_timeout(connect_timeout)
        .read_timeout(std::time::Duration::from_secs(120))
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| ReleaseError::Http(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_and_path_are_detected() {
        assert_eq!(
            provider_from_url("https://github.com/a/b/releases"),
            Some(Provider::Github)
        );
        assert_eq!(
            repository_path("https://gitee.com/ch3ngyz/repo/releases", &Provider::Gitee).unwrap(),
            ("ch3ngyz".into(), "repo".into())
        );
    }

    #[test]
    fn versions_and_channels_are_supported() {
        assert_eq!(
            parse_version("v1.2.3-beta").unwrap().to_string(),
            "1.2.3-beta"
        );
        assert!(parse_version("5.4.23.5").is_some());
        let releases = vec![
            ReleaseInfo {
                release_id: None,
                tag_name: "v1.0.0".into(),
                version: Some("1.0.0".into()),
                name: String::new(),
                body: String::new(),
                published_at: None,
                prerelease: false,
                draft: false,
                assets: Vec::new(),
            },
            ReleaseInfo {
                release_id: None,
                tag_name: "v1.1.0-beta".into(),
                version: Some("1.1.0-beta".into()),
                name: String::new(),
                body: String::new(),
                published_at: None,
                prerelease: true,
                draft: false,
                assets: Vec::new(),
            },
        ];
        assert_eq!(filter_channel(releases.clone(), &Channel::Stable).len(), 1);
        assert_eq!(filter_channel(releases, &Channel::Beta).len(), 1);
    }

    #[test]
    fn asset_rules_and_proxy_rewrite_work() {
        let asset = ReleaseAsset {
            name: "HostUtilities.zip".into(),
            size: 1,
            download_url: "https://github.com/a/b/releases/download/v1/HostUtilities.zip".into(),
            digest: None,
        };
        assert!(asset_matches(
            &asset,
            &[AssetRule {
                rule_type: AssetRuleType::Exact,
                value: "HostUtilities.zip".into()
            }]
        ));
        assert_eq!(
            rewrite_proxy(
                "https://proxy.example",
                "https://api.github.com/repos/a/b/releases"
            ),
            "https://proxy.example/repos/a/b/releases"
        );
    }

    #[test]
    fn release_id_is_preserved_for_gitee_attachment_requests() {
        let value = serde_json::json!([{
            "id": "42",
            "tag_name": "v1.0.0",
            "assets": [{
                "name": "HostUtilities.zip",
                "browser_download_url": "https://gitee.com/example/download"
            }]
        }]);
        let releases = parse_release_json(&value, &Provider::Gitee).unwrap();
        assert_eq!(releases[0].release_id, Some(42));
        assert_eq!(releases[0].assets[0].size, 0);
    }

    #[test]
    fn user_agent_identifies_client_and_version() {
        assert!(APP_USER_AGENT.starts_with("Overcooked 2 Mod Manager/"));
        assert!(APP_USER_AGENT.contains("client=desktop"));
        assert!(APP_USER_AGENT.contains("arch=x64"));
    }
}
