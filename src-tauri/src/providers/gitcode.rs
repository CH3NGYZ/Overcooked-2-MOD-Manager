use super::Infrastructure;

pub(super) const INFRASTRUCTURE: Infrastructure = Infrastructure {
    homepage: "https://gitcode.com/",
    repository_api: "https://api.atomgit.com/api/v5/repos/",
    hosts: &[
        "gitcode.com",
        "www.gitcode.com",
        "atomgit.com",
        "www.atomgit.com",
        "raw.atomgit.com",
        "raw.gitcode.com",
    ],
};

/// Generated archives are identified by the API type or the exact platform
/// archive route; uploaded ZIPs containing "archive" in their name remain valid.
pub(crate) fn is_generated_asset(asset: &serde_json::Value) -> bool {
    if asset.get("type").and_then(serde_json::Value::as_str) == Some("source") {
        return true;
    }
    let Some(url) = asset
        .get("browser_download_url")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| url::Url::parse(value).ok())
    else {
        return false;
    };
    if !matches!(
        url.host_str(),
        Some(
            "gitcode.com"
                | "atomgit.com"
                | "raw.atomgit.com"
                | "raw.gitcode.com"
                | "api.atomgit.com"
        )
    ) {
        return false;
    }
    let parts: Vec<_> = url.path_segments().into_iter().flatten().collect();
    parts.get(2) == Some(&"archive")
        || (parts.get(2) == Some(&"-") && parts.get(3) == Some(&"archive"))
}
