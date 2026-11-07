use super::error::ReleaseError;
use reqwest::Client;

/// Stable identifier for Cloudflare rules and access logs. The package
/// version makes every request identify the exact desktop client release.
pub const APP_USER_AGENT: &str = concat!(
    "Overcooked 2 MOD Manager/",
    env!("CARGO_PKG_VERSION"),
    " (client=desktop; os=Windows; arch=x64; tauri=2)"
);

pub fn build_client(timeout_seconds: u64) -> Result<Client, ReleaseError> {
    // 保留调用方的超时选择，至少留一秒请求窗口；单次 Release API 请求
    // 在 transport::fetch_api_json 中另设 API_REQUEST_TIMEOUT 覆盖它。
    let timeout = std::time::Duration::from_secs(timeout_seconds.max(1));
    Client::builder()
        .timeout(timeout)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release API 客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout_seconds
            );
            ReleaseError::Http(e.to_string())
        })
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
        .map_err(|e| {
            log::error!(
                "构建 Release 下载客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout_seconds
            );
            ReleaseError::Http(e.to_string())
        })
}

#[cfg(test)]
mod tests;
