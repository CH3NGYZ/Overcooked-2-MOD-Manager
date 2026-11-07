use crate::locale_tag::with_params;
use crate::releases::APP_USER_AGENT;
use std::time::Duration;

pub(super) const API_ENDPOINT: &str = "https://api.github.com";

const API_VERSION: &str = "2022-11-28";

pub(super) const RELEASE_PAGE_SIZE: u32 = 100;

const API_TIMEOUT: Duration = Duration::from_secs(30);

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) fn request_error(reason: &str) -> String {
    with_params(
        "mapPublish.requestFailed",
        &[("action", "network"), ("reason", reason)],
    )
}

pub(super) fn action_error(action: &str, reason: &str) -> String {
    with_params(
        "mapPublish.requestFailed",
        &[("action", action), ("reason", reason)],
    )
}

pub(super) fn api_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(API_TIMEOUT)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|error| request_error(&error.to_string()))
}

pub(super) fn upload_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(UPLOAD_TIMEOUT)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|error| request_error(&error.to_string()))
}

pub(super) fn authorized(request: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    request
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("Authorization", format!("Bearer {token}"))
}

/// 取一个响应头的文本值；缺失或不是 UTF-8 都当成没有。
fn header_text(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 把一次失败的写操作分类成前端能据此引导用户的 tag。
///
/// 403 混着三种完全不同的情况：App 声明里缺权限、被限流、以及令牌权限够但仓库
/// 没授权给这个安装。GitHub 会把还缺的权限放在 `x-accepted-github-permissions`
/// （形如 `contents=write`），限流则把剩余次数置 0；两个信号都没有时只能按通用
/// 无权限报，让用户自己去 App 设置里核对。
fn classify_write_failure(
    status: u16,
    accepted_permissions: Option<&str>,
    rate_remaining: Option<&str>,
    rate_reset: Option<&str>,
    detail: &str,
) -> Option<String> {
    match status {
        401 => Some("mapPublish.unauthorized".to_string()),
        403 => {
            if let Some(permissions) = accepted_permissions {
                return Some(with_params(
                    "mapPublish.missingPermission",
                    &[("permission", permissions)],
                ));
            }
            let limited =
                rate_remaining == Some("0") || detail.to_ascii_lowercase().contains("rate limit");
            if limited {
                return Some(with_params(
                    "mapPublish.rateLimited",
                    &[("reset", rate_reset.unwrap_or_default())],
                ));
            }
            Some(with_params("mapPublish.forbidden", &[("reason", detail)]))
        }
        404 => Some(with_params("mapPublish.notFound", &[("reason", detail)])),
        422 => Some(with_params("mapPublish.rejected", &[("reason", detail)])),
        _ => None,
    }
}

/// 把非 2xx 的响应翻成 tag。GitHub 的错误正文里 `message` 是给人看的原因，
/// 直接当参数带给前端 —— 比只显示状态码有用得多。
///
/// 403 的分类要看响应头，所以必须在 `text()` 消费响应之前取出来。
pub(super) async fn ensure_success(
    response: reqwest::Response,
    action: &str,
) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let accepted = header_text(&response, "x-accepted-github-permissions");
    let rate_remaining = header_text(&response, "x-ratelimit-remaining");
    let rate_reset = header_text(&response, "x-ratelimit-reset");
    let body = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
    log::warn!(
        "GitHub 写操作失败：动作={action}，状态={}，详情={detail}",
        status.as_u16()
    );
    Err(classify_write_failure(
        status.as_u16(),
        accepted.as_deref(),
        rate_remaining.as_deref(),
        rate_reset.as_deref(),
        &detail,
    )
    .unwrap_or_else(|| action_error(action, &detail)))
}

pub(super) async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    action: &str,
) -> Result<T, String> {
    let response = authorized(client.get(url), token)
        .send()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    ensure_success(response, action)
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))
}

#[cfg(test)]
mod tests;
