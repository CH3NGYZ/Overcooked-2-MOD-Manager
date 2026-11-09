use super::credentials::{GITCODE_OAUTH_CLIENT_ID, GITCODE_OAUTH_SCOPE};
use crate::models::PublishProvider;
mod callback;

use super::credentials::CALLBACK_URL;
use super::credentials::GITHUB_APP_CLIENT_ID;
use super::credentials::PUBLISH_INSTALL_URL;
use super::credentials::client_secret;
use super::credentials::exchange_code;
use super::http::action_error;
use super::validation::encode_component;
use crate::app_state::AppState;
use crate::locale_tag::with_params;
use crate::models::PublishAuthMode;
use crate::models::PublishAuthPoll;
use crate::models::PublishAuthorization;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use callback::bind_callback_listener;
use callback::spawn_callback_listener;
use sha2::Digest;
use sha2::Sha256;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::State;

const AUTHORIZE_ENDPOINT: &str = "https://github.com/login/oauth/authorize";

/// 等回调的上限：授权码本身有 10 分钟，但「装 App + 勾仓库」两分钟足够，
/// 超时就当这次作废，别把本机端口占上五分钟。
const AUTHORIZE_TIMEOUT: Duration = Duration::from_secs(120);

/// 一次授权里等回调的状态：回调线程把 `code` 塞进 `outcome`，换 Token 仍在异步
/// 命令里做（线程里没有 tokio runtime，硬塞一个阻塞客户端只会多一条网络实现）。
struct PendingAuthorization {
    provider: PublishProvider,
    /// PKCE 只在纯 OAuth 授权时才有；安装页那条路没有 code_challenge。
    verifier: Option<String>,
    cancel: Arc<AtomicBool>,
    outcome: Arc<Mutex<CallbackOutcome>>,
}

#[derive(Debug, Clone)]
enum CallbackOutcome {
    Pending,
    /// 已经在换 Token：授权码只能用一次，第二次轮询不许再发。
    Exchanging,
    Code {
        code: String,
        /// 安装页回调会带上刚装好的 installation id；纯 OAuth 授权没有。
        installation: Option<u64>,
    },
    Failed(String),
}

static PENDING_AUTH: Mutex<Option<PendingAuthorization>> = Mutex::new(None);

/// 43 字符以上、只含 `[A-Za-z0-9-._~]` 的随机串（PKCE `code_verifier` 的要求）。
fn random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// PKCE 只支持 S256：`BASE64URL(SHA256(verifier))`。
fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(super) fn clear_pending_auth() {
    if let Ok(mut pending) = PENDING_AUTH.lock() {
        if let Some(flow) = pending.take() {
            flow.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// 取消并丢弃当前待授权流程，把回调端口让出来。
fn release_pending_auth() {
    if let Ok(mut pending) = PENDING_AUTH.lock() {
        if let Some(flow) = pending.take() {
            flow.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// 这次授权该打开哪个页面。
///
/// 没记下 installation id 的第一次去安装页：装 App 与勾仓库一步完成，回调带回
/// `installation_id`；记下了就只重新授权 —— 已安装的账号进安装页会停在配置页，
/// 根本不会回调本机。
///
/// `force_authorize` 是「装过 App 但没记下 id」的兜底：那条路进安装页不会回调，
/// 只能改走 OAuth 把 Token 拿回来，再靠列安装把 id 补记上。
fn authorization_target(
    provider: PublishProvider,
    known: u64,
    nonce: &str,
    force_authorize: bool,
) -> (PublishAuthMode, String, Option<String>) {
    if provider == PublishProvider::Gitcode {
        return (
            PublishAuthMode::Authorize,
            format!(
                "https://gitcode.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
                encode_component(GITCODE_OAUTH_CLIENT_ID),
                encode_component(CALLBACK_URL),
                encode_component(GITCODE_OAUTH_SCOPE),
                encode_component(nonce)
            ),
            None,
        );
    }
    if known == 0 && !force_authorize {
        return (
            PublishAuthMode::Install,
            format!("{PUBLISH_INSTALL_URL}?state={}", encode_component(nonce)),
            None,
        );
    }
    let verifier = random_token();
    let challenge = code_challenge(&verifier);
    // `offline_access` 是 GitHub App 唯一认的 scope：它不加任何权限，只要求发
    // 会过期的 access token + refresh token。App 设置里关了令牌过期时也生效。
    let url = format!(
        "{AUTHORIZE_ENDPOINT}?client_id={}&redirect_uri={}&scope=offline_access&state={}&code_challenge={}&code_challenge_method=S256",
        encode_component(GITHUB_APP_CLIENT_ID),
        encode_component(CALLBACK_URL),
        encode_component(nonce),
        encode_component(&challenge),
    );
    (PublishAuthMode::Authorize, url, Some(verifier))
}

/// 启动一次授权：先占住本机端口，再打开浏览器。
///
/// 顺序不能反：浏览器回到构建配置的本机回调地址时，只认这一次连接，先开浏览器再 `bind`
/// 会把回调丢在地上，用户看到的是「授权成功但软件没反应」。
///
/// 打开的页面分两种，由「有没有记下 installation id」决定：
/// - 没记下（第一次用）：直接去 App 安装页。用户在 GitHub 上勾仓库的同时完成
///   授权，回调会带回 `code` + `installation_id` —— 装 App 和选仓库是一步。
/// - 记下了（之前装过）：只走 OAuth 授权。安装页对已安装的账号会停在配置页、
///   不会回调本机；而仓库范围按记下的 id 直查，本来就是最新的。
///
/// `force_authorize` 来自前端的「改用 GitHub 登录」：装过 App 却没记下 id 的用户
/// 只有这条路能拿到 Token，拿到之后列一次安装就把 id 补记上了。
#[tauri::command]
pub async fn start_publish_authorization(
    state: State<'_, AppState>,
    provider: PublishProvider,
    force_authorize: Option<bool>,
) -> Result<PublishAuthorization, String> {
    client_secret(provider)?;
    let known = {
        let config = state.config.load_or_default();
        state.config.publish_installation_id(&config)
    };
    let listener = bind_callback_listener()?;
    let nonce = random_token();
    let (mode, authorize_url, verifier) =
        authorization_target(provider, known, &nonce, force_authorize.unwrap_or(false));
    let outcome = Arc::new(Mutex::new(CallbackOutcome::Pending));
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut pending = PENDING_AUTH
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        *pending = Some(PendingAuthorization {
            provider,
            verifier,
            cancel: cancel.clone(),
            outcome: outcome.clone(),
        });
    }
    spawn_callback_listener(listener, nonce, cancel, outcome);
    if let Err(error) = crate::shell_open::open_url(&authorize_url) {
        clear_pending_auth();
        return Err(with_params(
            "mapPublish.browserFailed",
            &[("reason", &error)],
        ));
    }
    log::info!("发布授权页已在系统浏览器打开（模式={mode:?}），等待本机回调");
    Ok(PublishAuthorization {
        authorize_url,
        expires_in: AUTHORIZE_TIMEOUT.as_secs(),
        mode,
    })
}

/// 取消当前授权：离开上传页时调一次，别让端口一直被占着。
#[tauri::command]
pub fn cancel_publish_authorization() {
    if let Ok(mut pending) = PENDING_AUTH.lock() {
        if let Some(flow) = pending.take() {
            flow.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// 轮询一次授权进度：前端每 1 秒调一次，离开页面就停。
#[tauri::command]
pub async fn poll_publish_authorization(
    state: State<'_, AppState>,
    provider: PublishProvider,
) -> Result<PublishAuthPoll, String> {
    let service = state.config.clone();
    let operation = state.operation.clone();
    let (verifier, outcome) = {
        let pending = PENDING_AUTH
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        match pending.as_ref() {
            None => return Ok(PublishAuthPoll::Expired),
            Some(flow) if flow.provider == provider => {
                (flow.verifier.clone(), flow.outcome.clone())
            }
            Some(_) => return Ok(PublishAuthPoll::Expired),
        }
    };
    let snapshot = outcome
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?
        .clone();
    match snapshot {
        CallbackOutcome::Pending | CallbackOutcome::Exchanging => Ok(PublishAuthPoll::Pending),
        CallbackOutcome::Failed(reason) => {
            clear_pending_auth();
            Ok(auth_failure(reason))
        }
        CallbackOutcome::Code { code, installation } => {
            {
                let mut guard = outcome.lock().map_err(|_| "lock.poisoned".to_string())?;
                if !matches!(*guard, CallbackOutcome::Code { .. }) {
                    return Ok(PublishAuthPoll::Pending);
                }
                *guard = CallbackOutcome::Exchanging;
            }
            let _session = super::session::SESSION_LOCK.lock().await;
            let exchanged = exchange_code(provider, &code, verifier.as_deref()).await;
            // A canceled or replaced flow may finish its network exchange late.
            // Hold this short guard until persistence finishes so it cannot revive it.
            let mut pending = PENDING_AUTH
                .lock()
                .map_err(|_| "lock.poisoned".to_string())?;
            if !pending.as_ref().is_some_and(|flow| {
                flow.provider == provider
                    && Arc::ptr_eq(&flow.outcome, &outcome)
                    && !flow.cancel.load(Ordering::Relaxed)
            }) {
                return Ok(PublishAuthPoll::Expired);
            }
            match exchanged {
                Ok((tokens, login)) => {
                    {
                        let _guard = operation.lock().map_err(|_| "lock.poisoned".to_string())?;
                        let mut config = service.load_or_default();
                        service
                            .set_publish_tokens(&mut config, &tokens, provider, &login)
                            .map_err(|error| action_error("save-token", &error.to_string()))?;
                        // 安装页回调带回的 id 就是「这次勾了哪些仓库」的答案：
                        // 记下来之后列仓库按 id 直查，不用等 /user/installations。
                        if let Some(installation) =
                            installation.filter(|_| provider == PublishProvider::Github)
                        {
                            service.set_publish_installation_id(&mut config, installation);
                        }
                        service
                            .save(&config)
                            .map_err(|error| action_error("save-token", &error.to_string()))?;
                    }
                    pending.take();
                    log::info!(
                        "发布 Token 已保存：用户={login}，安装={installation:?}，可刷新={}",
                        tokens.refresh_token.is_some()
                    );
                    Ok(PublishAuthPoll::Authorized { login })
                }
                Err(reason) => {
                    pending.take();
                    log::warn!("发布授权换取 Token 失败：{reason}");
                    Ok(auth_failure(reason))
                }
            }
        }
    }
}

/// 把内部失败原因收敛成前端认识的分支。
fn auth_failure(reason: String) -> PublishAuthPoll {
    match reason.as_str() {
        "mapPublish.authorizationDenied" => PublishAuthPoll::Denied,
        "mapPublish.authorizationExpired" => PublishAuthPoll::Expired,
        _ => PublishAuthPoll::Failed { reason },
    }
}

#[cfg(test)]
mod tests;
