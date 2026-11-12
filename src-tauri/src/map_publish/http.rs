use crate::api_scheduler::ApiRequestExt;
use crate::locale_tag::with_params;
use crate::models::PublishProvider;
use crate::releases::APP_USER_AGENT;
use std::time::Duration;

pub(super) const API_ENDPOINT: &str = "https://api.github.com";

const API_VERSION: &str = "2022-11-28";

pub(super) const REPOSITORY_PAGE_SIZE: u32 = 100;

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
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|error| request_error(&error.to_string()))
}

pub(super) fn authorized(
    request: reqwest::RequestBuilder,
    token: &str,
    provider: PublishProvider,
) -> reqwest::RequestBuilder {
    let request = request.bearer_auth(token);
    if provider == PublishProvider::Gitcode {
        return request.header("Accept", "application/json");
    }
    request
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
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

/// 仅保留平台的错误说明字段，不向界面或日志传递原始响应。
fn error_detail(body: &[u8]) -> Option<String> {
    use regex::Regex;
    use std::sync::LazyLock;
    static SECRETS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            r#"(?is)\b(?:access[_-]?token|refresh[_-]?token|client[_-]?secret|password|passwd|secret|authorization|cookie|set-cookie|private-token|api[_-]?key|token)\b\s*["']?\s*[:=].*"#,
            r#"|\b(?:bearer|basic)\s+[^\s,;]+"#,
            r#"|https?://[^\s<>"']+"#,
            r#"|\b(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]+"#,
            r#"|\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"#,
        ))
        .expect("valid credential redaction pattern")
    });
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    [
        "message",
        "error_message",
        "msg",
        "error_description",
        "error",
    ]
    .into_iter()
    .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
    .find_map(|detail| {
        let detail = detail.trim();
        if detail.is_empty() || detail.contains(['<', '>', '\0']) {
            return None;
        }
        // 先脱敏再截断，避免截在凭据中间后绕过识别。URL 整段移除，
        // 不保留可能携带令牌、Cookie 或签名的查询串和路径。
        let redacted = SECRETS.replace_all(detail, "[REDACTED_SECRET]");
        let plain = redacted
            .chars()
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect::<String>();
        let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut limited = plain.chars().take(511).collect::<String>();
        if plain.chars().count() > 511 {
            limited.push('…');
        }
        (!limited.is_empty()).then_some(limited)
    })
}

/// 把非 2xx 的响应翻成 tag。兼容 GitHub message 和 GitCode error_message，
/// 只在界面返回经过筛选、脱敏和限长的纯文本。
///
/// 403 的分类要看响应头，所以必须在消费响应之前取出来。
pub(super) async fn ensure_success(
    mut response: reqwest::Response,
    action: &str,
) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let accepted = header_text(&response, "x-accepted-github-permissions");
    let rate_remaining = header_text(&response, "x-ratelimit-remaining");
    let rate_reset = header_text(&response, "x-ratelimit-reset");
    let provider = [PublishProvider::Github, PublishProvider::Gitcode]
        .into_iter()
        .find(|provider| {
            reqwest::Url::parse(provider.api_endpoint())
                .ok()
                .is_some_and(|endpoint| endpoint.host_str() == response.url().host_str())
        });
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() <= 16 * 1024 => {
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            _ => {
                body.clear();
                break;
            }
        }
    }
    let detail = error_detail(&body).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
    log::warn!(
        "发布写操作失败：平台={provider:?}，动作={action}，状态={}",
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
    provider: PublishProvider,
    url: &str,
    action: &str,
) -> Result<T, String> {
    let response = authorized(client.get(url), token, provider)
        .send_api(provider.api_provider())
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
