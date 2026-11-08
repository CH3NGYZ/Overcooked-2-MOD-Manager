use super::authorization::clear_pending_auth;
use super::credentials::ApiUser;
use super::credentials::GITHUB_APP_CLIENT_ID;
use super::credentials::GITHUB_APP_CLIENT_SECRET;
use super::credentials::PUBLISH_INSTALL_URL;
use super::credentials::TokenResponse;
use super::credentials::client_secret;
use super::credentials::token_request;
use super::credentials::tokens_from_response;
use super::credentials::{
    GITCODE_OAUTH_CLIENT_ID, GITCODE_OAUTH_CLIENT_SECRET, GITCODE_OAUTH_SCOPE,
};
use super::http::action_error;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::request_error;
use crate::api_scheduler::ApiRequestExt;
use crate::app_state::AppState;
use crate::locale_tag::with_params;
use crate::models::PublishAuthState;
use crate::models::PublishProvider;
use crate::models::PublishSessionCheck;
use crate::models::PublishSessionOutcome;
use crate::models::PublishTokens;
use tauri::State;

/// 取出发布用的 Token；没有就让用户先登录。
pub(super) fn publish_token(state: &AppState, provider: PublishProvider) -> Result<String, String> {
    let config = state.config.load_or_default();
    state
        .config
        .publish_token(&config, provider)
        .ok_or_else(|| "mapPublish.notAuthenticated".to_string())
}

/// access token 还剩不到这么多秒就当作已过期，直接刷新：省一次注定 401 的 `/user`，
/// 也避免刚检查完、点上传时恰好过期。
const ACCESS_EXPIRY_MARGIN_SECS: i64 = 120;

/// 会话检查期间只允许一个刷新在跑。
///
/// refresh token 是一次性的：两次并发刷新时第二次拿的是已作废的旧 token，GitHub
/// 会拒绝，而按「被拒绝 = 失效」处理就会把第一次刚换到的新令牌清掉。
pub(super) static SESSION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 按本地记下的过期时刻判断 access token 是否已经（或即将）失效。
/// 不知道过期时刻（不过期的令牌、旧配置）就交给 `/user` 去判。
fn access_token_expired(expires_at: Option<i64>, now: i64) -> bool {
    expires_at.is_some_and(|at| now + ACCESS_EXPIRY_MARGIN_SECS >= at)
}

/// refresh token 还能不能用：要有，且没过本地记下的过期时刻。
fn refresh_token_usable(refresh: Option<&str>, refresh_expires_at: Option<i64>, now: i64) -> bool {
    refresh.is_some_and(|value| !value.trim().is_empty())
        && refresh_expires_at.is_none_or(|at| now < at)
}

/// 一次 `/user` 探测的结论：令牌可用、被 GitHub 判失效、或者网络 / 服务暂时不可用。
#[derive(Debug, PartialEq, Eq)]
enum TokenProbe {
    Valid(String),
    Rejected,
    Unavailable(String),
}

async fn probe_token(
    client: &reqwest::Client,
    token: &str,
    provider: PublishProvider,
) -> TokenProbe {
    let response = match authorized(
        client.get(format!("{}/user", provider.api_endpoint())),
        token,
        provider,
    )
    .send_api(provider.api_provider())
    .await
    {
        Ok(response) => response,
        Err(error) => {
            return TokenProbe::Unavailable(request_error(&error.without_url().to_string()));
        }
    };
    let status = response.status().as_u16();
    // 只有 401 说明令牌本身失效；403/5xx 是权限或服务问题，刷新也救不回来。
    if status == 401 {
        return TokenProbe::Rejected;
    }
    match ensure_success(response, "user").await {
        Ok(response) => match response.json::<ApiUser>().await {
            Ok(user) => TokenProbe::Valid(user.login),
            Err(error) => TokenProbe::Unavailable(request_error(&error.without_url().to_string())),
        },
        Err(reason) => TokenProbe::Unavailable(reason),
    }
}

/// 一次 refresh 请求的结论。
#[derive(Debug, PartialEq, Eq)]
enum RefreshResult {
    Refreshed(PublishTokens),
    /// GitHub 明确拒绝（refresh token 失效 / 被撤销 / 应用配置不符）：只能重新授权。
    Rejected(String),
    /// 网络或服务暂时不可用：本地令牌保留，下次进页面再试。
    Unavailable(String),
}

/// 刷新失败的 `error` 字段 → 语言表 tag。
///
/// `bad_refresh_token` 是最常见的一种（过期、已用过、被撤销都报它）；其余按通用
/// 「授权失效」处理，原因原样带上方便排查。
fn refresh_rejection_reason(error: &str, description: Option<&str>) -> String {
    match error {
        "bad_refresh_token" => "mapPublish.sessionExpired".to_string(),
        _ => with_params(
            "mapPublish.sessionRefreshRejected",
            &[("reason", description.unwrap_or(error))],
        ),
    }
}

async fn refresh_tokens(
    client: &reqwest::Client,
    refresh_token: &str,
    provider: PublishProvider,
) -> RefreshResult {
    let secret = match client_secret(provider) {
        Ok(secret) => secret,
        Err(reason) => return RefreshResult::Unavailable(reason),
    };
    let request = match token_request(client, provider, secret, None, None, Some(refresh_token)) {
        Ok(request) => request,
        Err(reason) => return RefreshResult::Unavailable(reason),
    };
    let response = match request.send_api(provider.api_provider()).await {
        Ok(response) => response,
        Err(error) => {
            return RefreshResult::Unavailable(request_error(&error.without_url().to_string()));
        }
    };
    if !response.status().is_success() {
        // 这个端点的业务错误都是 200 + `error` 字段；非 2xx 是服务本身出了问题。
        let status = response.status().as_u16();
        return RefreshResult::Unavailable(action_error(
            "refresh-token",
            &format!("HTTP {status}"),
        ));
    }
    let body: TokenResponse = match response.json().await {
        Ok(body) => body,
        Err(error) => {
            return RefreshResult::Unavailable(request_error(&error.without_url().to_string()));
        }
    };
    if let Some(error) = body.error.as_deref() {
        log::warn!("发布 Token 刷新被拒绝");
        return RefreshResult::Rejected(refresh_rejection_reason(
            error,
            Some("OAuth refresh rejected"),
        ));
    }
    match tokens_from_response(&body, chrono::Utc::now().timestamp()) {
        Some(tokens) => RefreshResult::Refreshed(tokens),
        None => RefreshResult::Unavailable(action_error("refresh-token", "empty token")),
    }
}

fn publish_auth_state(state: &AppState, provider: PublishProvider) -> PublishAuthState {
    let config = state.config.load_or_default();
    PublishAuthState {
        authenticated: state.config.publish_token(&config, provider).is_some(),
        login: publish_login(&config, provider).to_string(),
        client_id: if provider == PublishProvider::Github {
            GITHUB_APP_CLIENT_ID
        } else {
            GITCODE_OAUTH_CLIENT_ID
        }
        .to_string(),
        install_url: if provider == PublishProvider::Github {
            PUBLISH_INSTALL_URL
        } else {
            ""
        }
        .to_string(),
        installed: provider == PublishProvider::Github
            && state.config.publish_installation_id(&config) > 0,
        secret_configured: match provider {
            PublishProvider::Github => !GITHUB_APP_CLIENT_SECRET.is_empty(),
            PublishProvider::Gitcode => {
                !GITCODE_OAUTH_CLIENT_ID.is_empty()
                    && !GITCODE_OAUTH_CLIENT_SECRET.is_empty()
                    && !GITCODE_OAUTH_SCOPE.is_empty()
            }
        },
    }
}

/// 令牌失效且救不回来：清掉本地令牌（installation id 保留），让界面回到登录态。
fn drop_publish_tokens(state: &AppState, provider: PublishProvider) -> Result<(), String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    state.config.clear_publish_token(&mut config, provider);
    state
        .config
        .save(&config)
        .map_err(|error| action_error("sign-out", &error.to_string()))
}

fn store_refreshed_tokens(
    state: &AppState,
    tokens: &PublishTokens,
    provider: PublishProvider,
    login: &str,
) -> Result<(), String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    state
        .config
        .set_publish_tokens(&mut config, tokens, provider, login)
        .map_err(|error| action_error("save-token", &error.to_string()))?;
    state
        .config
        .save(&config)
        .map_err(|error| action_error("save-token", &error.to_string()))
}

/// 进入地图发布页时检查一次登录状态；access token 失效时用 refresh token 换新。
///
/// 只有这一个入口会刷新：页面里的其他请求遇到 401 只报错，不在请求链里暗中
/// 换令牌（约定见 `docs/map-publish.md`）。网络失败不清令牌 —— 断网不等于掉线。
#[tauri::command]
pub async fn check_publish_session(
    state: State<'_, AppState>,
    provider: PublishProvider,
) -> Result<PublishSessionCheck, String> {
    let _session = SESSION_LOCK.lock().await;
    let outcome = check_session_once(&state, provider).await?;
    Ok(PublishSessionCheck {
        outcome,
        auth: publish_auth_state(&state, provider),
    })
}

async fn check_session_once(
    state: &AppState,
    provider: PublishProvider,
) -> Result<PublishSessionOutcome, String> {
    let config = state.config.load_or_default();
    let Some(access_token) = state.config.publish_token(&config, provider) else {
        return Ok(PublishSessionOutcome::SignedOut);
    };
    let refresh_token = state.config.publish_refresh_token(&config, provider);
    let login = publish_login(&config, provider).to_string();
    let now = chrono::Utc::now().timestamp();
    let client = api_client()?;

    if !access_token_expired(publish_expiries(&config, provider).0, now) {
        match probe_token(&client, &access_token, provider).await {
            TokenProbe::Valid(_) => return Ok(PublishSessionOutcome::Valid),
            TokenProbe::Unavailable(reason) => {
                log::warn!("发布页会话检查失败（保留本地令牌）：{reason}");
                return Ok(PublishSessionOutcome::Unavailable { reason });
            }
            TokenProbe::Rejected => log::info!("发布 access token 已失效，尝试刷新"),
        }
    } else {
        log::info!("发布 access token 已到期，尝试刷新");
    }

    if !refresh_token_usable(
        refresh_token.as_deref(),
        publish_expiries(&config, provider).1,
        now,
    ) {
        drop_publish_tokens(state, provider)?;
        log::info!("发布令牌无法刷新（没有或已过期 refresh token），已清除，需重新授权");
        return Ok(PublishSessionOutcome::Reauthorize {
            reason: "mapPublish.sessionExpired".to_string(),
        });
    }
    let refresh_token = refresh_token.unwrap_or_default();
    match refresh_tokens(&client, &refresh_token, provider).await {
        RefreshResult::Refreshed(tokens) => {
            // 旧的一对已经作废：先落盘，再查登录名（查失败也不能丢新令牌）。
            store_refreshed_tokens(state, &tokens, provider, &login)?;
            if let TokenProbe::Valid(fresh) =
                probe_token(&client, &tokens.access_token, provider).await
                && fresh != login
            {
                store_refreshed_tokens(state, &tokens, provider, &fresh)?;
            }
            log::info!("发布令牌已刷新并保存");
            Ok(PublishSessionOutcome::Refreshed)
        }
        RefreshResult::Rejected(reason) => {
            drop_publish_tokens(state, provider)?;
            Ok(PublishSessionOutcome::Reauthorize { reason })
        }
        RefreshResult::Unavailable(reason) => {
            log::warn!("刷新发布令牌暂时失败（保留本地令牌）：{reason}");
            Ok(PublishSessionOutcome::Unavailable { reason })
        }
    }
}

/// 读当前登录状态。前端进页面先调这个，不联网。
#[tauri::command]
pub fn map_publish_state(
    state: State<'_, AppState>,
    provider: PublishProvider,
) -> PublishAuthState {
    publish_auth_state(&state, provider)
}

/// 退出发布登录：只清发布 Token，用户自己的 API Token 不动。
#[tauri::command]
pub async fn sign_out_map_publish(
    state: State<'_, AppState>,
    provider: PublishProvider,
) -> Result<PublishAuthState, String> {
    clear_pending_auth();
    let _session = SESSION_LOCK.lock().await;
    {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let mut config = state.config.load_or_default();
        state.config.clear_publish_token(&mut config, provider);
        state
            .config
            .save(&config)
            .map_err(|error| action_error("sign-out", &error.to_string()))?;
    }
    clear_pending_auth();
    log::info!("已退出发布登录");
    // 退出登录不动 installation id：用户在 GitHub 上改仓库范围时它不变，
    // 下次登录还能按它直查。
    Ok(publish_auth_state(&state, provider))
}

#[cfg(test)]
mod tests;

fn publish_login(config: &crate::models::AppConfig, provider: PublishProvider) -> &str {
    match provider {
        PublishProvider::Github => &config.auth.github_publish_login,
        PublishProvider::Gitcode => &config.auth.gitcode_publish.login,
    }
}
fn publish_expiries(
    config: &crate::models::AppConfig,
    provider: PublishProvider,
) -> (Option<i64>, Option<i64>) {
    match provider {
        PublishProvider::Github => (
            config.auth.github_publish_token_expires_at,
            config.auth.github_publish_refresh_expires_at,
        ),
        PublishProvider::Gitcode => (
            config.auth.gitcode_publish.expires_at,
            config.auth.gitcode_publish.refresh_expires_at,
        ),
    }
}
