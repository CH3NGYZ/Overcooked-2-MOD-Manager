use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    #[serde(default)]
    pub release_id: Option<u64>,
    pub tag_name: String,
    pub version: Option<String>,
    pub name: String,
    pub body: String,
    pub published_at: Option<String>,
    /// Raw platform verdict; missing in legacy caches. Explicit false is authoritative.
    #[serde(
        default,
        rename = "apiPrerelease",
        alias = "api_prerelease",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_prerelease: Option<bool>,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRepositoryReleaseSnapshot {
    pub cached_releases: Vec<ReleaseInfo>,
    pub release_prerelease_metadata: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub download_count: Option<u64>,
    pub download_url: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub repository_id: String,
    pub local_version: Option<String>,
    pub remote_version: Option<String>,
    pub state: String,
}
