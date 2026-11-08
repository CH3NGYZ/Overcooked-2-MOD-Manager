use super::config::default_true;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    Direct,
    AutoDirect,
    AutoProxy,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubProxy {
    pub id: String,
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    /// Origin of this session snapshot: online manifest rather than bundled fallback.
    #[serde(default)]
    pub remote: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubProxyConfig {
    #[serde(default)]
    pub release_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitcode_api_proxies: Vec<GitHubProxy>,
    /// 头像专用代理。
    ///
    /// 单独一组而不是复用 raw：实测 raw 组全部取不到 `github.com/{owner}.png`
    /// （模板都按 `{owner}/{repo}/{branch}/{path}` 设计），能取到头像的是
    /// 「纯 {url} 包装」型。随包定义位于 `proxies/proxies-avatar.toml`，
    /// 完整校验代理索引与请求计划闭包后投影到此组，随快照整份替换。
    /// 条目不持久化到用户配置。
    #[serde(default)]
    pub avatar_proxies: Vec<GitHubProxy>,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubProxyManifest {
    #[serde(default = "default_proxy_manifest_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub release_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitcode_api_proxies: Vec<GitHubProxy>,
    /// 头像代理（`[[avatar_proxies]]`），与另外四组同一套清单格式。
    #[serde(default)]
    pub avatar_proxies: Vec<GitHubProxy>,
}

#[cfg(test)]
fn default_proxy_manifest_version() -> u32 {
    1
}

impl Default for ProxyMode {
    fn default() -> Self {
        Self::AutoDirect
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub timeout_seconds: u64,
    pub download_threads: u32,
    pub max_retry_count: u32,
}
