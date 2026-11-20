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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Kept for old configuration files; requests use the explicit settings below.
    pub timeout_seconds: u64,
    pub download_threads: u32,
    pub max_retry_count: u32,
    #[serde(default = "default_race_min_speed_kbps")]
    pub race_min_speed_kbps: u64,
    #[serde(default = "default_race_window_seconds")]
    pub race_window_seconds: u64,
    #[serde(default = "default_api_timeout_seconds")]
    pub api_timeout_seconds: u64,
    #[serde(default = "default_first_byte_timeout_seconds")]
    pub first_byte_timeout_seconds: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 10,
            download_threads: 32,
            max_retry_count: 3,
            race_min_speed_kbps: default_race_min_speed_kbps(),
            race_window_seconds: default_race_window_seconds(),
            api_timeout_seconds: default_api_timeout_seconds(),
            first_byte_timeout_seconds: default_first_byte_timeout_seconds(),
        }
    }
}

const RACE_SPEED_OPTIONS_KBPS: [u64; 7] = [32, 64, 128, 256, 512, 1024, 2048];

// Built from the same defaults used by the settings UI and tutorial.
include!(concat!(env!("OUT_DIR"), "/download_defaults.rs"));

const fn default_race_min_speed_kbps() -> u64 {
    RACE_MIN_SPEED_KBPS
}
const fn default_race_window_seconds() -> u64 {
    RACE_WINDOW_SECONDS
}
const fn default_api_timeout_seconds() -> u64 {
    API_TIMEOUT_SECONDS
}
const fn default_first_byte_timeout_seconds() -> u64 {
    FIRST_BYTE_TIMEOUT_SECONDS
}

impl NetworkConfig {
    /// Used by persisted config, settings patches and runtime snapshots alike.
    pub(crate) fn normalize(&mut self) -> bool {
        let previous = self.clone();
        self.race_min_speed_kbps = self.race_minimum_speed() / 1024;
        self.race_window_seconds = self.race_window().as_secs();
        self.api_timeout_seconds = self.api_timeout().as_secs();
        self.first_byte_timeout_seconds = self.first_byte_timeout().as_secs();
        *self != previous
    }

    pub(crate) fn race_minimum_speed(&self) -> u64 {
        let kbps = if RACE_SPEED_OPTIONS_KBPS.contains(&self.race_min_speed_kbps) {
            self.race_min_speed_kbps
        } else {
            default_race_min_speed_kbps()
        };
        kbps * 1024
    }

    pub(crate) fn race_window(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.race_window_seconds.clamp(5, 20))
    }

    pub(crate) fn api_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.api_timeout_seconds.clamp(5, 20))
    }

    pub(crate) fn first_byte_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.first_byte_timeout_seconds.clamp(5, 20))
    }
}

#[cfg(test)]
mod tests;
