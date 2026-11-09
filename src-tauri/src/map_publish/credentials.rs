use super::http::action_error;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::request_error;
use crate::api_scheduler::ApiRequestExt;
use crate::locale_tag::with_params;
use crate::models::PublishProvider;
use crate::models::PublishTokens;
use serde::Deserialize;

// 构建期注入凭据；只引用生成项，不在源码或日志中展开其值。
include!(concat!(env!("OUT_DIR"), "/github_app.rs"));
include!(concat!(env!("OUT_DIR"), "/gitcode_oauth.rs"));

pub(super) const ACCESS_TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";

/// Only literal loopback IPs are accepted: no DNS resolution or external bind.
pub(super) fn callback_endpoint(value: &str) -> Option<(std::net::SocketAddr, String)> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() == "/"
    {
        return None;
    }
    let ip = match url.host()? {
        url::Host::Ipv4(ip) => std::net::IpAddr::V4(ip),
        url::Host::Ipv6(ip) => std::net::IpAddr::V6(ip),
        _ => return None,
    };
    let port = url.port_or_known_default()?;
    if !ip.is_loopback() || port == 0 {
        return None;
    }
    Some((std::net::SocketAddr::new(ip, port), url.path().to_string()))
}

fn valid_install_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let parts: Vec<_> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && parts.len() == 4
        && parts[0] == "apps"
        && parts[2] == "installations"
        && parts[3] == "new"
}

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
    #[serde(deserialize_with = "optional_seconds")]
    expires_in: Option<i64>,
    /// refresh token 剩余秒数（GitHub 固定 6 个月）。
    #[serde(default)]
    refresh_token_expires_in: Option<i64>,
    #[serde(default)]
    pub(super) error: Option<String>,
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
pub(super) fn client_secret(provider: PublishProvider) -> Result<&'static str, String> {
    if provider == PublishProvider::Gitcode {
        if GITCODE_OAUTH_CLIENT_ID.is_empty()
            || GITCODE_OAUTH_CLIENT_SECRET.is_empty()
            || callback_endpoint(CALLBACK_URL).is_none()
        {
            return Err("mapPublish.missingOAuthConfig".to_string());
        }
        return Ok(GITCODE_OAUTH_CLIENT_SECRET);
    }
    if GITHUB_APP_CLIENT_ID.is_empty()
        || GITHUB_APP_CLIENT_SECRET.is_empty()
        || !valid_install_url(PUBLISH_INSTALL_URL)
        || callback_endpoint(CALLBACK_URL).is_none()
    {
        return Err("mapPublish.missingClientSecret".to_string());
    }
    Ok(GITHUB_APP_CLIENT_SECRET)
}

pub(super) async fn exchange_code(
    provider: PublishProvider,
    code: &str,
    verifier: Option<&str>,
) -> Result<(PublishTokens, String), String> {
    let secret = client_secret(provider)?;
    let client = api_client()?;
    let response = token_request(&client, provider, secret, Some(code), verifier, None)?
        .send_api(provider.api_provider())
        .await
        .map_err(|error| request_error(&error.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(action_error(
            "access-token",
            &format!("HTTP {}", response.status().as_u16()),
        ));
    }
    let body: TokenResponse = response
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    if let Some(error) = body.error.as_deref() {
        let reason = "OAuth authorization rejected";
        log::warn!("发布授权换 Token 被拒绝");
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
    let login = fetch_login(&client, &tokens.access_token, provider).await?;
    Ok((tokens, login))
}

async fn fetch_login(
    client: &reqwest::Client,
    token: &str,
    provider: PublishProvider,
) -> Result<String, String> {
    let response = authorized(
        client.get(format!("{}/user", provider.api_endpoint())),
        token,
        provider,
    )
    .send_api(provider.api_provider())
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

fn optional_seconds<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(serde_json::Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("invalid expiry")),
        Some(serde_json::Value::String(s)) => s.parse().map(Some).map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("invalid expiry")),
    }
}

/// Keep the documented secret in multipart data, never in a URL. Token request
/// errors must remove their URL because GitCode's refresh token is a query field.
pub(super) fn token_request(
    client: &reqwest::Client,
    provider: PublishProvider,
    secret: &str,
    code: Option<&str>,
    verifier: Option<&str>,
    refresh: Option<&str>,
) -> Result<reqwest::RequestBuilder, String> {
    if provider == PublishProvider::Github {
        let mut form = vec![
            ("client_id", GITHUB_APP_CLIENT_ID),
            ("client_secret", secret),
        ];
        if let Some(refresh) = refresh {
            form.extend([("grant_type", "refresh_token"), ("refresh_token", refresh)]);
        } else {
            form.extend([
                ("code", code.unwrap_or_default()),
                ("redirect_uri", CALLBACK_URL),
            ]);
            if let Some(verifier) = verifier {
                form.push(("code_verifier", verifier));
            }
        }
        return Ok(client
            .post(ACCESS_TOKEN_ENDPOINT)
            .header("Accept", "application/json")
            .form(&form));
    }
    let request = client
        .post("https://gitcode.com/oauth/token")
        .header("Accept", "application/json");
    if let Some(refresh) = refresh {
        return Ok(request.query(&[("grant_type", "refresh_token"), ("refresh_token", refresh)]));
    }
    let boundary = format!("oc2{}", uuid::Uuid::new_v4().simple());
    // Generated configuration is a single field; reject multipart control bytes.
    if secret.contains(['\r', '\n']) {
        return Err("mapPublish.missingOAuthConfig".into());
    }
    Ok(request.query(&[("grant_type", "authorization_code"), ("code", code.unwrap_or_default()), ("client_id", GITCODE_OAUTH_CLIENT_ID)])
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"client_secret\"\r\n\r\n{secret}\r\n--{boundary}--\r\n")))
}
