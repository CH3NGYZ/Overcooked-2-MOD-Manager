use super::http::API_ENDPOINT;
use super::http::action_error;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::request_error;
use crate::api_scheduler::ApiRequestExt;
use crate::locale_tag::with_params;
use crate::models::PublishTokens;
use serde::Deserialize;

// 构建期注入凭据；只引用生成项，不在源码或日志中展开其值。
include!(concat!(env!("OUT_DIR"), "/github_app.rs"));

/// 管理器内置的 GitHub App Client ID（public client，不是秘密）。
pub const GITHUB_APP_CLIENT_ID: &str = "Iv23liMDOtLBrrkv781U";

/// App 的安装入口：安装时选「Only select repositories」，只把地图仓库交出来。
pub const PUBLISH_INSTALL_URL: &str = "https://github.com/apps/oc2-manager/installations/new";

pub(super) const ACCESS_TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";

pub(super) const CALLBACK_URL: &str = "http://127.0.0.1:55748/callback";

/// `/login/oauth/access_token` 的响应：成功给 Token，失败给 `error`。
///
/// 授权码换令牌与 refresh token 换令牌是同一个端点、同一种响应。App 开启了
/// 「令牌过期」（或授权时带了 `offline_access`）才会有后三个字段；关掉时只有
/// `access_token`，那张令牌不过期，也就不需要刷新。
#[derive(Deserialize)]
pub(super) struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    /// access token 剩余秒数（GitHub 固定 8 小时）。
    #[serde(default)]
    expires_in: Option<i64>,
    /// refresh token 剩余秒数（GitHub 固定 6 个月）。
    #[serde(default)]
    refresh_token_expires_in: Option<i64>,
    #[serde(default)]
    pub(super) error: Option<String>,
    #[serde(default)]
    pub(super) error_description: Option<String>,
}

/// 把成功响应换算成要落盘的一组令牌；access token 为空时返回 `None`。
///
/// 剩余秒数在这里一次性换成绝对时刻：落盘的是「几点过期」，下次进页面
/// 不需要知道这张令牌是什么时候拿到的。
pub(super) fn tokens_from_response(body: &TokenResponse, now: i64) -> Option<PublishTokens> {
    let access_token = body
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())?
        .to_string();
    let refresh_token = body
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string);
    let at = |seconds: Option<i64>| seconds.filter(|value| *value > 0).map(|value| now + value);
    Some(PublishTokens {
        access_token,
        expires_at: at(body.expires_in),
        refresh_expires_at: refresh_token
            .as_ref()
            .and(at(body.refresh_token_expires_in)),
        refresh_token,
    })
}

#[derive(Deserialize)]
pub(super) struct ApiUser {
    pub(super) login: String,
}

/// Client Secret 由 `build.rs` 注入；空串说明这个构建没配，换不到 Token。
pub(super) fn client_secret() -> Result<&'static str, String> {
    if GITHUB_APP_CLIENT_SECRET.is_empty() {
        return Err("mapPublish.missingClientSecret".to_string());
    }
    Ok(GITHUB_APP_CLIENT_SECRET)
}

pub(super) async fn exchange_code(
    code: &str,
    verifier: Option<&str>,
) -> Result<(PublishTokens, String), String> {
    let secret = client_secret()?;
    let client = api_client()?;
    let mut form = vec![
        ("client_id", GITHUB_APP_CLIENT_ID),
        ("client_secret", secret),
        ("code", code),
        ("redirect_uri", CALLBACK_URL),
    ];
    // 只有发起过 PKCE 挑战的流程才带 verifier；安装页那条路没有挑战，
    // 硬带上会被 GitHub 当成不匹配。
    if let Some(verifier) = verifier {
        form.push(("code_verifier", verifier));
    }
    let response = client
        .post(ACCESS_TOKEN_ENDPOINT)
        .header("Accept", "application/json")
        .form(&form)
        .send_api(crate::models::Provider::Github)
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let body: TokenResponse = ensure_success(response, "access-token")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    if let Some(error) = body.error.as_deref() {
        let reason = body.error_description.as_deref().unwrap_or(error);
        log::warn!("GitHub 授权换 Token 被拒绝：{error}（{reason}）");
        return Err(match error {
            "bad_verification_code" | "expired_token" => {
                "mapPublish.authorizationExpired".to_string()
            }
            "access_denied" => "mapPublish.authorizationDenied".to_string(),
            _ => with_params("mapPublish.authorizationFailed", &[("reason", reason)]),
        });
    }
    let Some(tokens) = tokens_from_response(&body, chrono::Utc::now().timestamp()) else {
        return Err(action_error("access-token", "empty token"));
    };
    let login = fetch_login(&client, &tokens.access_token).await?;
    Ok((tokens, login))
}

async fn fetch_login(client: &reqwest::Client, token: &str) -> Result<String, String> {
    let response = authorized(client.get(format!("{API_ENDPOINT}/user")), token)
        .send_api(crate::models::Provider::Github)
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let user: ApiUser = ensure_success(response, "user")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    Ok(user.login)
}

#[cfg(test)]
mod tests;
