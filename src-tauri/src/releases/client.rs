use super::error::ReleaseError;
use reqwest::Client;
use std::time::Duration;

/// Stable identifier for Cloudflare rules and access logs. The package
/// version makes every request identify the exact desktop client release.
pub const APP_USER_AGENT: &str = concat!(
    "Overcooked 2 MOD Manager/",
    env!("CARGO_PKG_VERSION"),
    " (client=desktop; os=Windows; arch=x64; tauri=2)"
);

pub fn build_client(timeout: Duration) -> Result<Client, ReleaseError> {
    Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release API 客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout.as_secs()
            );
            ReleaseError::Http(e.to_string())
        })
}

/// Builds a client for potentially large release assets. The total request
/// timeout is disabled so a download is allowed to run as long as data keeps
/// arriving. The read timeout still aborts a connection that has gone idle.
pub fn build_download_client(first_byte_timeout: Duration) -> Result<Client, ReleaseError> {
    Client::builder()
        .connect_timeout(first_byte_timeout)
        .read_timeout(std::time::Duration::from_secs(120))
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release 下载客户端失败: requested_timeout_seconds={} error_kind=client_build",
                first_byte_timeout.as_secs()
            );
            ReleaseError::Http(e.to_string())
        })
}

#[cfg(test)]
mod tests;
