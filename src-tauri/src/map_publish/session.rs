use super::authorization::clear_pending_auth;
use super::credentials::ACCESS_TOKEN_ENDPOINT;
use super::credentials::ApiUser;
use super::credentials::GITHUB_APP_CLIENT_ID;
use super::credentials::GITHUB_APP_CLIENT_SECRET;
use super::credentials::PUBLISH_INSTALL_URL;
use super::credentials::TokenResponse;
use super::credentials::client_secret;
use super::credentials::tokens_from_response;
use super::http::API_ENDPOINT;
use super::http::action_error;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::request_error;
use crate::api_scheduler::ApiRequestExt;
use crate::app_state::AppState;
use crate::locale_tag::with_params;
use crate::models::PublishAuthState;
use crate::models::PublishSessionCheck;
use crate::models::PublishSessionOutcome;
use crate::models::PublishTokens;
use tauri::State;

/// 取出发布用的 Token；没有就让用户先登录。
pub(super) fn publish_token(state: &AppState) -> Result<String, String> {
    let config = state.config.load_or_default();
    state
        .config
        .publish_token(&config)
        .ok_or_else(|| "mapPublish.notAuthenticated".to_string())
}

/// access token 还剩不到这么多秒就当作已过期，直接刷新：省一次注定 401 的 `/user`，
/// 也避免刚检查完、点上传时恰好过期。
const ACCESS_EXPIRY_MARGIN_SECS: i64 = 120;

/// 会话检查期间只允许一个刷新在跑。
///
/// refresh token 是一次性的：两次并发刷新时第二次拿的是已作废的旧 token，GitHub
/// 会拒绝，而按「被拒绝 = 失效」处理就会把第一次刚换到的新令牌清掉。
static SESSION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

async fn probe_token(client: &reqwest::Client, token: &str) -> TokenProbe {
    let response = match authorized(client.get(format!("{API_ENDPOINT}/user")), token)
        .send_api(crate::models::Provider::Github)
        .await
    {
        Ok(response) => response,
        Err(error) => return TokenProbe::Unavailable(request_error(&error.to_string())),
    };
    let status = response.status().as_u16();
    // 只有 401 说明令牌本身失效；403/5xx 是权限或服务问题，刷新也救不回来。
    if status == 401 {
        return TokenProbe::Rejected;
    }
    match ensure_success(response, "user").await {
        Ok(response) => match response.json::<ApiUser>().await {
            Ok(user) => TokenProbe::Valid(user.login),
            Err(error) => TokenProbe::Unavailable(request_error(&error.to_string())),
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

async fn refresh_tokens(client: &reqwest::Client, refresh_token: &str) -> RefreshResult {
    let secret = match client_secret() {
        Ok(secret) => secret,
        Err(reason) => return RefreshResult::Unavailable(reason),
    };
    let form = [
        ("client_id", GITHUB_APP_CLIENT_ID),
        ("client_secret", secret),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    let response = match client
        .post(ACCESS_TOKEN_ENDPOINT)
        .header("Accept", "application/json")
        .form(&form)
        .send_api(crate::models::Provider::Github)
        .await
    {
        Ok(response) => response,
        Err(error) => return RefreshResult::Unavailable(request_error(&error.to_string())),
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
        Err(error) => return RefreshResult::Unavailable(request_error(&error.to_string())),
    };
    if let Some(error) = body.error.as_deref() {
        log::warn!(
            "GitHub 刷新发布 Token 被拒绝：{error}（{}）",
            body.error_description.as_deref().unwrap_or_default()
        );
        return RefreshResult::Rejected(refresh_rejection_reason(
            error,
            body.error_description.as_deref(),
        ));
    }
    match tokens_from_response(&body, chrono::Utc::now().timestamp()) {
        Some(tokens) => RefreshResult::Refreshed(tokens),
        None => RefreshResult::Unavailable(action_error("refresh-token", "empty token")),
    }
}

fn publish_auth_state(state: &AppState) -> PublishAuthState {
    let config = state.config.load_or_default();
    PublishAuthState {
        authenticated: state.config.publish_token(&config).is_some(),
        login: config.auth.github_publish_login.clone(),
        client_id: GITHUB_APP_CLIENT_ID.to_string(),
        install_url: PUBLISH_INSTALL_URL.to_string(),
        installed: state.config.publish_installation_id(&config) > 0,
        secret_configured: !GITHUB_APP_CLIENT_SECRET.is_empty(),
    }
}

/// 令牌失效且救不回来：清掉本地令牌（installation id 保留），让界面回到登录态。
fn drop_publish_tokens(state: &AppState) -> Result<(), String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    state.config.clear_publish_token(&mut config);
    state
        .config
        .save(&config)
        .map_err(|error| action_error("sign-out", &error.to_string()))
}

fn store_refreshed_tokens(
    state: &AppState,
    tokens: &PublishTokens,
    login: &str,
) -> Result<(), String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    state
        .config
        .set_publish_tokens(&mut config, tokens, login)
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
) -> Result<PublishSessionCheck, String> {
    let _session = SESSION_LOCK.lock().await;
    let outcome = check_session_once(&state).await?;
    Ok(PublishSessionCheck {
        outcome,
        auth: publish_auth_state(&state),
    })
}

async fn check_session_once(state: &AppState) -> Result<PublishSessionOutcome, String> {
    let config = state.config.load_or_default();
    let Some(access_token) = state.config.publish_token(&config) else {
        return Ok(PublishSessionOutcome::SignedOut);
    };
    let refresh_token = state.config.publish_refresh_token(&config);
    let login = config.auth.github_publish_login.clone();
    let now = chrono::Utc::now().timestamp();
    let client = api_client()?;

    if !access_token_expired(config.auth.github_publish_token_expires_at, now) {
        match probe_token(&client, &access_token).await {
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
        config.auth.github_publish_refresh_expires_at,
        now,
    ) {
        drop_publish_tokens(state)?;
        log::info!("发布令牌无法刷新（没有或已过期 refresh token），已清除，需重新授权");
        return Ok(PublishSessionOutcome::Reauthorize {
            reason: "mapPublish.sessionExpired".to_string(),
        });
    }
    let refresh_token = refresh_token.unwrap_or_default();
    match refresh_tokens(&client, &refresh_token).await {
        RefreshResult::Refreshed(tokens) => {
            // 旧的一对已经作废：先落盘，再查登录名（查失败也不能丢新令牌）。
            store_refreshed_tokens(state, &tokens, &login)?;
            if let TokenProbe::Valid(fresh) = probe_token(&client, &tokens.access_token).await
                && fresh != login
            {
                store_refreshed_tokens(state, &tokens, &fresh)?;
            }
            log::info!("发布令牌已刷新并保存");
            Ok(PublishSessionOutcome::Refreshed)
        }
        RefreshResult::Rejected(reason) => {
            drop_publish_tokens(state)?;
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
pub fn map_publish_state(state: State<'_, AppState>) -> PublishAuthState {
    publish_auth_state(&state)
}

/// 退出发布登录：只清发布 Token，用户自己的 API Token 不动。
#[tauri::command]
pub fn sign_out_map_publish(state: State<'_, AppState>) -> Result<PublishAuthState, String> {
    {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let mut config = state.config.load_or_default();
        state.config.clear_publish_token(&mut config);
        state
            .config
            .save(&config)
            .map_err(|error| action_error("sign-out", &error.to_string()))?;
    }
    clear_pending_auth();
    log::info!("已退出 GitHub 发布登录");
    // 退出登录不动 installation id：用户在 GitHub 上改仓库范围时它不变，
    // 下次登录还能按它直查。
    Ok(publish_auth_state(&state))
}

#[cfg(test)]
mod tests;
