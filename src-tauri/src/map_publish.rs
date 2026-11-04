//! 社区地图上传页的后端：GitHub App 用户授权 + 仓库 / Release / 附件的写操作。
//!
//! 这条路只做**写**。地图的读仍然是 `map_sources` 的 `release-tags` 源：上传页
//! 写进 Release note 的 `<!-- oc2-map -->` 块，管理器的地图页刷新时就按同一份
//! 契约消费，不需要第二套读取逻辑。
//!
//! ## 为什么用 GitHub App 而不是 OAuth App
//!
//! GitHub App 安装时可以选择「Only select repositories」：作者只勾几个地图仓库，
//! 写权限就只覆盖这几个，不会顺带拿到整个账号的写权限。用户 Token 的权限是
//! App 权限 ∩ 用户权限 ∩ 已安装仓库三层取交集，比 OAuth App 的 `repo` 全量授权窄。
//!
//! ## 为什么单独存一份 Token
//!
//! 授权换来的 Token 写进用户自己的 `github_token` 会**降低**权限：那张 Token 可能
//! 带私有仓库权限，被覆盖后配置里私有仓库的 Release 立刻读不出来。所以
//! `github_publish_token` 与 `github_token` 各存一份，各带自己的 `*_set` 标志。
//!
//! ## 为什么待授权状态放在模块内的 static
//!
//! `AppState` 的构造点有 4 处（`commands.rs` 里两处测试与两处 setup 分支），
//! 为一个只在授权时存在的临时状态给每一处都加一行不值得。它的寿命就是一次
//! 授权（最长 [`AUTHORIZE_TIMEOUT`]），放模块级 `Mutex` 最省事，进程退出即丢。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::commands::AppState;
use crate::locale_tag::with_params;
use crate::map_note::{
    VERSION_PLACEHOLDER, publish_metadata_from_note, render_release_body, strip_note_block,
};
use crate::models::{
    OperationResult, PublishAsset, PublishAuthMode, PublishAuthPoll, PublishAuthState,
    PublishAuthorization, PublishDeleteReleaseRequest, PublishRelease, PublishRepository,
    PublishRepositoryList, PublishSaveRequest, PublishUploadRequest,
};
use crate::releases::APP_USER_AGENT;

// Client Secret 由 `build.rs` 在编译期注入（环境变量，或本机被忽略的
// `.tmp/github-app-secret.txt`），仓库里既没有明文也不该有。
include!(concat!(env!("OUT_DIR"), "/github_app.rs"));

/// 管理器内置的 GitHub App Client ID（public client，不是秘密）。
pub const GITHUB_APP_CLIENT_ID: &str = "Iv23liMDOtLBrrkv781U";
/// App 的安装入口：安装时选「Only select repositories」，只把地图仓库交出来。
pub const PUBLISH_INSTALL_URL: &str = "https://github.com/apps/oc2-manager/installations/new";

const AUTHORIZE_ENDPOINT: &str = "https://github.com/login/oauth/authorize";
const ACCESS_TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
const API_ENDPOINT: &str = "https://api.github.com";
const UPLOAD_ENDPOINT: &str = "https://uploads.github.com";

/// 本机回调地址必须与 App 设置里登记的 Callback URL **完全一致**：GitHub 的通配
/// 匹配只放宽子域与子路径，端口必须相同，所以这里写死 55748。
const CALLBACK_ADDR: &str = "127.0.0.1:55748";
const CALLBACK_URL: &str = "http://127.0.0.1:55748/callback";
const CALLBACK_PATH: &str = "/callback";
/// 等回调的上限：授权码本身有 10 分钟，但「装 App + 勾仓库」两分钟足够，
/// 超时就当这次作废，别把本机端口占上五分钟。
const AUTHORIZE_TIMEOUT: Duration = Duration::from_secs(120);

/// 上一轮授权还没结束就再点一次时，端口还在旧监听手里。监听线程每 120 ms 看一次
/// 取消标志，所以重试两秒足够；直接 bind 失败会让用户看到「端口被占用」而浏览器
/// 根本没打开，只能干等到上一轮超时。
const BIND_RETRIES: usize = 25;
const BIND_RETRY_DELAY: Duration = Duration::from_millis(80);

const API_VERSION: &str = "2022-11-28";

/// 新仓库自动打上的标签。`overcooked-2` + `bepinex` 是社区发现的准入口径，
/// `map-source` 是「这是地图源仓库」的约定标记（见 `docs/map-sources.md` 3.8）。
const PUBLISH_TOPICS: [&str; 3] = ["overcooked-2", "bepinex", "map-source"];

const RELEASE_PAGE_SIZE: u32 = 100;
/// 单份附件上限。GitHub 允许到 2 GiB，但这里是整份读进内存再上传的，
/// 定在 256 MiB 足够放下任何地图合集，同时不至于把内存吃光。
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const API_TIMEOUT: Duration = Duration::from_secs(30);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// 一次授权里等回调的状态：回调线程把 `code` 塞进 `outcome`，换 Token 仍在异步
/// 命令里做（线程里没有 tokio runtime，硬塞一个阻塞客户端只会多一条网络实现）。
struct PendingAuthorization {
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

// —— 线上数据结构：只声明用得到的字段 ——

/// `/login/oauth/access_token` 的响应：成功给 Token，失败给 `error`。
#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// `/user/installations` 里的一条安装记录（只用到 id）。
#[derive(Deserialize)]
struct ApiInstallation {
    id: u64,
    /// `all` 表示这次安装覆盖账号下全部仓库（含以后新建的）。
    #[serde(default)]
    repository_selection: String,
}

/// `GET /user/installations/{id}`：直查一次安装，只用到仓库范围。
#[derive(Deserialize)]
struct ApiInstallationDetail {
    #[serde(default)]
    repository_selection: String,
}

#[derive(Deserialize)]
struct ApiInstallationList {
    #[serde(default)]
    installations: Vec<ApiInstallation>,
}

#[derive(Deserialize)]
struct ApiInstallationRepositories {
    #[serde(default)]
    repositories: Vec<ApiRepository>,
}

#[derive(Deserialize)]
struct ApiUser {
    login: String,
}

#[derive(Deserialize)]
struct ApiPermissions {
    #[serde(default)]
    push: bool,
}

#[derive(Deserialize)]
struct ApiRepository {
    name: String,
    full_name: String,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    default_branch: String,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    permissions: Option<ApiPermissions>,
}

#[derive(Deserialize)]
struct ApiAsset {
    id: u64,
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct ApiRelease {
    id: u64,
    tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

// —— 错误 tag ——

fn request_error(reason: &str) -> String {
    with_params(
        "mapPublish.requestFailed",
        &[("action", "network"), ("reason", reason)],
    )
}

fn action_error(action: &str, reason: &str) -> String {
    with_params(
        "mapPublish.requestFailed",
        &[("action", action), ("reason", reason)],
    )
}

fn invalid_repository(value: &str) -> String {
    with_params("mapPublish.invalidRepository", &[("value", value)])
}

fn invalid_tag(value: &str) -> String {
    with_params("mapPublish.invalidTag", &[("value", value)])
}

fn invalid_name(value: &str) -> String {
    with_params("mapPublish.invalidName", &[("value", value)])
}

fn invalid_asset(reason: &str) -> String {
    with_params("mapPublish.invalidAsset", &[("reason", reason)])
}

// —— 通用工具 ——

fn api_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(API_TIMEOUT)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|error| request_error(&error.to_string()))
}

fn upload_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(UPLOAD_TIMEOUT)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|error| request_error(&error.to_string()))
}

fn authorized(request: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    request
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("Authorization", format!("Bearer {token}"))
}

/// 把非 2xx 的响应翻成 tag。GitHub 的错误正文里 `message` 是给人看的原因，
/// 直接当参数带给前端 —— 比只显示状态码有用得多。
async fn ensure_success(
    response: reqwest::Response,
    action: &str,
) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
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
    match status.as_u16() {
        401 => Err("mapPublish.unauthorized".to_string()),
        403 => Err(with_params("mapPublish.forbidden", &[("reason", &detail)])),
        404 => Err(with_params("mapPublish.notFound", &[("reason", &detail)])),
        422 => Err(with_params("mapPublish.rejected", &[("reason", &detail)])),
        _ => Err(action_error(action, &detail)),
    }
}

/// 取出发布用的 Token；没有就让用户先登录。
fn publish_token(state: &AppState) -> Result<String, String> {
    let config = state.config.load_or_default();
    state
        .config
        .publish_token(&config)
        .ok_or_else(|| "mapPublish.notAuthenticated".to_string())
}

/// GitHub 的 slug：owner / 仓库名都只由这些字符构成。
///
/// 只做**形状**校验，不做真实性校验（真实与否由 API 判定）。拒绝 `/` 与
/// `..` 之类能改变请求路径的东西 —— 后续 URL 是字符串拼接出来的。
fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '-' | '_' | '.'))
}

fn validate_coordinates(owner: &str, repository: &str) -> Result<(String, String), String> {
    let owner = owner.trim();
    let repository = repository.trim();
    if !valid_slug(owner) || !valid_slug(repository) {
        return Err(invalid_repository(&format!("{owner}/{repository}")));
    }
    Ok((owner.to_string(), repository.to_string()))
}

/// tag 就是给消费端看的版本号，必须是纯版本：`1.2.0` 而不是 `v1.2.0`。
///
/// 首字符限定数字，是因为 `load_release_tag_maps` 把 `tag_name` 直接当版本写进
/// 卡片；带 `v` 前缀的 tag 与「本地已装的 1.2.0」永远不相等，用户会一直看到
/// 「可更新」。
fn validate_tag(tag: &str) -> Result<String, String> {
    let tag = tag.trim();
    let starts_with_digit = tag
        .chars()
        .next()
        .map(|item| item.is_ascii_digit())
        .unwrap_or(false);
    let body_ok = tag.len() <= 100
        && tag
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '.' | '-' | '+'));
    if !starts_with_digit || !body_ok {
        return Err(invalid_tag(tag));
    }
    Ok(tag.to_string())
}

/// 附件名放行 `A-Z a-z 0-9 . _ -`；其余字符会让 `?name=` 查询串变形。
fn validate_asset_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '.' | '_' | '-'));
    if !ok {
        return Err(invalid_asset("name"));
    }
    Ok(name.to_string())
}

/// 严格百分号编码（不保留 `/`，与 `locale_tag::encode` 的宽松口径不同：
/// 这里的值要进 URL 查询串）。
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn asset_from_api(asset: ApiAsset) -> PublishAsset {
    PublishAsset {
        id: asset.id,
        name: asset.name,
        size: asset.size,
    }
}

/// 把一个 Release 拆成「块外正文 + 解析出的元数据」。
///
/// 解析失败不当作整条列表失败：作者可能手写了半截块，界面至少要能把这一版
/// 列出来并让他改回去。失败时正文用 `strip_note_block` 的近似结果（去掉块），
/// 元数据留空。
fn release_from_api(release: ApiRelease) -> PublishRelease {
    let (body, metadata) = match crate::map_note::parse_release_note(&release.body) {
        Ok(Some(block)) => (
            block.body,
            Some(publish_metadata_from_note(&block.metadata)),
        ),
        Ok(None) => (release.body.clone(), None),
        Err(error) => {
            log::warn!(
                "Release 的 oc2-map 块解析失败，按无元数据展示：tag={}，原因={error}",
                release.tag_name
            );
            (strip_note_block(&release.body), None)
        }
    };
    PublishRelease {
        id: release.id,
        tag: release.tag_name,
        title: release.name,
        draft: release.draft,
        prerelease: release.prerelease,
        created_at: release.created_at,
        published_at: release.published_at,
        body,
        assets: release.assets.into_iter().map(asset_from_api).collect(),
        metadata,
    }
}

fn repository_from_api(repository: ApiRepository) -> PublishRepository {
    let (owner, _) = repository
        .full_name
        .split_once('/')
        .map(|(owner, _)| (owner.to_string(), String::new()))
        .unwrap_or_else(|| (String::new(), repository.name.clone()));
    PublishRepository {
        owner,
        name: repository.name,
        full_name: repository.full_name,
        private: repository.private,
        default_branch: repository.default_branch,
        topics: repository.topics,
        push: repository
            .permissions
            .map(|permissions| permissions.push)
            .unwrap_or(false),
    }
}

// —— 登录（GitHub App 用户授权：系统浏览器 + 本机回调 + PKCE）——

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

/// Client Secret 由 `build.rs` 注入；空串说明这个构建没配，换不到 Token。
fn client_secret() -> Result<&'static str, String> {
    if GITHUB_APP_CLIENT_SECRET.is_empty() {
        return Err("mapPublish.missingClientSecret".to_string());
    }
    Ok(GITHUB_APP_CLIENT_SECRET)
}

fn clear_pending_auth() {
    if let Ok(mut pending) = PENDING_AUTH.lock() {
        *pending = None;
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

/// 占住回调端口：上一轮授权没结束时先取消，再等它松手。
fn bind_callback_listener() -> Result<TcpListener, String> {
    let mut last_error = None;
    for attempt in 0..BIND_RETRIES {
        if attempt > 0 {
            std::thread::sleep(BIND_RETRY_DELAY);
        }
        release_pending_auth();
        match TcpListener::bind(CALLBACK_ADDR) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(error),
        }
    }
    let error = last_error.expect("至少尝试过一次 bind");
    log::warn!("本机授权回调端口被占用：{error}");
    Err(with_params(
        "mapPublish.callbackUnavailable",
        &[("reason", &error.to_string())],
    ))
}

fn finish_callback(outcome: &Mutex<CallbackOutcome>, result: CallbackOutcome) {
    if let Ok(mut guard) = outcome.lock() {
        *guard = result;
    }
}

async fn get_json<T: serde::de::DeserializeOwned>(
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

/// 这次授权该打开哪个页面。
///
/// 没记下 installation id 的第一次去安装页：装 App 与勾仓库一步完成，回调带回
/// `installation_id`；记下了就只重新授权 —— 已安装的账号进安装页会停在配置页，
/// 根本不会回调本机。
///
/// `force_authorize` 是「装过 App 但没记下 id」的兜底：那条路进安装页不会回调，
/// 只能改走 OAuth 把 Token 拿回来，再靠列安装把 id 补记上。
fn authorization_target(
    known: u64,
    nonce: &str,
    force_authorize: bool,
) -> (PublishAuthMode, String, Option<String>) {
    if known == 0 && !force_authorize {
        return (
            PublishAuthMode::Install,
            format!("{PUBLISH_INSTALL_URL}?state={}", encode_component(nonce)),
            None,
        );
    }
    let verifier = random_token();
    let challenge = code_challenge(&verifier);
    let url = format!(
        "{AUTHORIZE_ENDPOINT}?client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256",
        encode_component(GITHUB_APP_CLIENT_ID),
        encode_component(CALLBACK_URL),
        encode_component(nonce),
        encode_component(&challenge),
    );
    (PublishAuthMode::Authorize, url, Some(verifier))
}

/// 启动一次授权：先占住本机端口，再打开浏览器。
///
/// 顺序不能反：浏览器回到 `127.0.0.1:55748` 只认这一次连接，先开浏览器再 `bind`
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
pub async fn start_github_authorization(
    state: State<'_, AppState>,
    force_authorize: Option<bool>,
) -> Result<PublishAuthorization, String> {
    client_secret()?;
    let known = {
        let config = state.config.load_or_default();
        state.config.publish_installation_id(&config)
    };
    let listener = bind_callback_listener()?;
    let nonce = random_token();
    let (mode, authorize_url, verifier) =
        authorization_target(known, &nonce, force_authorize.unwrap_or(false));
    let outcome = Arc::new(Mutex::new(CallbackOutcome::Pending));
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut pending = PENDING_AUTH
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        *pending = Some(PendingAuthorization {
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
    log::info!("GitHub 授权页已在系统浏览器打开（模式={mode:?}），等待本机回调");
    Ok(PublishAuthorization {
        authorize_url,
        expires_in: AUTHORIZE_TIMEOUT.as_secs(),
        mode,
    })
}

/// 取消当前授权：离开上传页时调一次，别让端口一直被占着。
#[tauri::command]
pub fn cancel_github_authorization() {
    if let Ok(mut pending) = PENDING_AUTH.lock() {
        if let Some(flow) = pending.take() {
            flow.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// 轮询一次授权进度：前端每 1 秒调一次，离开页面就停。
#[tauri::command]
pub async fn poll_github_authorization(
    state: State<'_, AppState>,
) -> Result<PublishAuthPoll, String> {
    let service = state.config.clone();
    let operation = state.operation.clone();
    let (verifier, outcome) = {
        let pending = PENDING_AUTH
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        match pending.as_ref() {
            None => return Ok(PublishAuthPoll::Expired),
            Some(flow) => (flow.verifier.clone(), flow.outcome.clone()),
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
            match exchange_code(&code, verifier.as_deref()).await {
                Ok((token, login)) => {
                    {
                        let _guard = operation.lock().map_err(|_| "lock.poisoned".to_string())?;
                        let mut config = service.load_or_default();
                        service.set_publish_token(&mut config, &token, &login);
                        // 安装页回调带回的 id 就是「这次勾了哪些仓库」的答案：
                        // 记下来之后列仓库按 id 直查，不用等 /user/installations。
                        if let Some(installation) = installation {
                            service.set_publish_installation_id(&mut config, installation);
                        }
                        service
                            .save(&config)
                            .map_err(|error| action_error("save-token", &error.to_string()))?;
                    }
                    clear_pending_auth();
                    log::info!("GitHub 发布 Token 已保存：用户={login}，安装={installation:?}");
                    Ok(PublishAuthPoll::Authorized { login })
                }
                Err(reason) => {
                    clear_pending_auth();
                    log::warn!("GitHub 授权换取 Token 失败：{reason}");
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

async fn exchange_code(code: &str, verifier: Option<&str>) -> Result<(String, String), String> {
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
        .send()
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
    let Some(token) = body.access_token.filter(|token| !token.trim().is_empty()) else {
        return Err(action_error("access-token", "empty token"));
    };
    let login = fetch_login(&client, &token).await?;
    Ok((token, login))
}

/// 后台等一次回调。用 std 线程而不是 tokio：这里只做「accept 一次 + 读一行请求」，
/// 不需要 runtime，也不需要把 `TcpListener` 塞进 async 状态。
fn spawn_callback_listener(
    listener: TcpListener,
    expected_state: String,
    cancel: Arc<AtomicBool>,
    outcome: Arc<Mutex<CallbackOutcome>>,
) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + AUTHORIZE_TIMEOUT;
        let _ = listener.set_nonblocking(true);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            if Instant::now() >= deadline {
                finish_callback(
                    &outcome,
                    CallbackOutcome::Failed("mapPublish.authorizationExpired".to_string()),
                );
                return;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    if handle_callback(stream, &expected_state, &outcome) {
                        return;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(error) => {
                    log::warn!("本机授权回调监听中断：{error}");
                    finish_callback(
                        &outcome,
                        CallbackOutcome::Failed(with_params(
                            "mapPublish.callbackUnavailable",
                            &[("reason", &error.to_string())],
                        )),
                    );
                    return;
                }
            }
        }
    });
}

/// 读一条 HTTP 请求，认路径与 `state`，把结果写回 `outcome`。
///
/// 返回 true 表示这次授权该收尾了（成功、拒绝、state 不符都算）。
fn handle_callback(
    mut stream: TcpStream,
    expected_state: &str,
    outcome: &Mutex<CallbackOutcome>,
) -> bool {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 8192];
    let read = match stream.read(&mut buffer) {
        Ok(read) if read > 0 => read,
        _ => return false,
    };
    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(target) = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return false;
    };
    let Some((path, query)) = target.split_once('?') else {
        let _ = write_callback_page(&mut stream, "400 Bad Request", "这不是授权回调地址。");
        return false;
    };
    if path != CALLBACK_PATH {
        let _ = write_callback_page(&mut stream, "404 Not Found", "这不是授权回调地址。");
        return false;
    }
    let params = parse_query(query);
    if params.get("state").map(String::as_str) != Some(expected_state) {
        let _ = write_callback_page(
            &mut stream,
            "400 Bad Request",
            "state 校验失败，请回到管理器重新发起授权。",
        );
        finish_callback(
            outcome,
            CallbackOutcome::Failed("mapPublish.stateMismatch".to_string()),
        );
        return true;
    }
    if let Some(error) = params.get("error") {
        let reason = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(error);
        let denied = error == "access_denied";
        let _ = write_callback_page(
            &mut stream,
            "200 OK",
            if denied {
                "已取消授权。"
            } else {
                "授权没有完成，请回到管理器重试。"
            },
        );
        log::warn!("GitHub 授权回调报错：{error}（{reason}）");
        let result = if denied {
            CallbackOutcome::Failed("mapPublish.authorizationDenied".to_string())
        } else {
            CallbackOutcome::Failed(with_params(
                "mapPublish.authorizationFailed",
                &[("reason", reason)],
            ))
        };
        finish_callback(outcome, result);
        return true;
    }
    // 安装页那条路会把刚装好的 installation id 一起带回来，它就是「用户勾了
    // 哪些仓库」的答案；纯 OAuth 授权没有这个参数。
    let installation = params
        .get("installation_id")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|id| *id > 0);
    match params.get("code").filter(|code| !code.is_empty()) {
        Some(code) => {
            let _ = write_callback_page(
                &mut stream,
                "200 OK",
                "授权完成，可以关闭此页面，回到 OC2 MOD Manager。",
            );
            finish_callback(
                outcome,
                CallbackOutcome::Code {
                    code: code.to_string(),
                    installation,
                },
            );
            true
        }
        None => {
            // 装上了却没有授权码：App 设置里没勾「Request user authorization
            // (OAuth) during installation」，那条路根本不会回传 code。
            let installed_only = installation.is_some();
            let _ = write_callback_page(
                &mut stream,
                "400 Bad Request",
                if installed_only {
                    "已安装 App，但没有拿到授权码：请回到管理器按提示检查 App 设置。"
                } else {
                    "回调里没有授权码，请回到管理器重试。"
                },
            );
            finish_callback(
                outcome,
                CallbackOutcome::Failed(if installed_only {
                    "mapPublish.installNotConfigured".to_string()
                } else {
                    with_params(
                        "mapPublish.authorizationFailed",
                        &[("reason", "missing code")],
                    )
                }),
            );
            true
        }
    }
}

fn write_callback_page(stream: &mut TcpStream, status: &str, message: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html>\r\n<html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
<title>OC2 地图上传</title></head>\r\n\
<body style=\"font-family:system-ui,-apple-system,'Segoe UI',sans-serif;padding:48px;\
line-height:1.7;color:#1f2328\">\r\n\
<h1 style=\"font-size:20px\">{message}</h1>\r\n\
<p style=\"color:#57606a\">OC2 MOD Manager · 地图上传</p>\r\n</body></html>\r\n"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((percent_decode(key), percent_decode(value)))
        })
        .collect()
}

/// 回调里的 `code` / `state` 都是 URL 编码过的；这里只需要解 `%XX` 与 `+`。
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        output.push((high * 16 + low) as u8);
                        index += 3;
                    }
                    _ => {
                        output.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

async fn fetch_login(client: &reqwest::Client, token: &str) -> Result<String, String> {
    let response = authorized(client.get(format!("{API_ENDPOINT}/user")), token)
        .send()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let user: ApiUser = ensure_success(response, "user")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    Ok(user.login)
}

/// 读当前登录状态。前端进页面先调这个，不联网。
#[tauri::command]
pub fn map_publish_state(state: State<'_, AppState>) -> PublishAuthState {
    let config = state.config.load_or_default();
    let authenticated = state.config.publish_token(&config).is_some();
    PublishAuthState {
        authenticated,
        login: config.auth.github_publish_login.clone(),
        client_id: GITHUB_APP_CLIENT_ID.to_string(),
        install_url: PUBLISH_INSTALL_URL.to_string(),
        installed: state.config.publish_installation_id(&config) > 0,
        secret_configured: !GITHUB_APP_CLIENT_SECRET.is_empty(),
    }
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
    Ok(PublishAuthState {
        authenticated: false,
        login: String::new(),
        client_id: GITHUB_APP_CLIENT_ID.to_string(),
        install_url: PUBLISH_INSTALL_URL.to_string(),
        // 退出登录不动 installation id：用户在 GitHub 上改仓库范围时它不变，
        // 下次登录还能按它直查。
        installed: {
            let config = state.config.load_or_default();
            state.config.publish_installation_id(&config) > 0
        },
        secret_configured: !GITHUB_APP_CLIENT_SECRET.is_empty(),
    })
}

// —— 仓库 ——

/// 列出当前账号安装范围内的可写仓库。
///
/// GitHub App 的仓库来源是「安装」而不是「账号」：先读 `/user/installations`，
/// 再逐个读该安装下可访问的仓库。作者在安装时勾了哪几个，这里就只有哪几个。
///
/// 记下的 installation id 要**先直查一次再并入**，不能只信 `/user/installations`：
/// 后者是最终一致的，刚在 GitHub 勾完仓库时它可能还是空的（实测退出重登后有
/// 25 秒空窗），而按 id 直查立刻就能拿到刚勾的那一份。
#[tauri::command]
pub async fn list_publish_repositories(
    state: State<'_, AppState>,
) -> Result<PublishRepositoryList, String> {
    let service = state.config.clone();
    let token = publish_token(&state)?;
    let client = api_client()?;
    let known = {
        let config = service.load_or_default();
        service.publish_installation_id(&config)
    };
    let mut installation_ids: Vec<u64> = Vec::new();
    let mut all_repositories = false;
    let mut result: Vec<PublishRepository> = Vec::new();
    if known > 0 {
        match remembered_installation_repositories(&client, &token, known).await {
            Ok(Some((selection, repositories))) => {
                all_repositories = selection == "all";
                installation_ids.push(known);
                log::info!("按记录的安装 {known} 直查：可写仓库={}", repositories.len());
                result.extend(repositories);
            }
            Ok(None) => {
                // App 被卸载（或换了账号）：这条记忆没用了，清掉退回归档路径。
                log::info!("记录的安装 {known} 在 GitHub 上已不存在，忘记它");
                remember_installation_id(&service, &state, 0);
            }
            Err(reason) => return Err(reason),
        }
    }
    let installations: ApiInstallationList = get_json(
        &client,
        &token,
        &format!("{API_ENDPOINT}/user/installations?per_page=100"),
        "list-installations",
    )
    .await?;
    for installation in &installations.installations {
        // 「全部仓库」的安装才会把以后新建的仓库自动纳进来；只授权部分仓库时，
        // 新建出来的仓库不在安装范围里，推不上去 —— 界面据此关掉「新建仓库」。
        if installation.repository_selection == "all" {
            all_repositories = true;
        }
        if installation_ids.contains(&installation.id) {
            continue;
        }
        installation_ids.push(installation.id);
        result.extend(installation_repositories(&client, &token, installation.id).await?);
    }
    // 只有一个安装却还没记下来（老用户第一次升级就没有回调可等）：顺手记下。
    if known == 0 && installation_ids.len() == 1 {
        remember_installation_id(&service, &state, installation_ids[0]);
    }
    let installation_count = installation_ids.len();
    // 已经是地图源的仓库排前面：作者多半是回来改自己已有的地图仓库。
    result.sort_by(|left, right| {
        let left_source = left.topics.iter().any(|topic| topic == "map-source");
        let right_source = right.topics.iter().any(|topic| topic == "map-source");
        right_source
            .cmp(&left_source)
            .then_with(|| left.full_name.cmp(&right.full_name))
    });
    result.dedup_by(|left, right| left.full_name == right.full_name);
    log::info!(
        "发布页仓库列表加载完成：安装={}，可写仓库={}",
        installation_count,
        result.len()
    );
    Ok(PublishRepositoryList {
        installations: installation_count,
        all_repositories,
        repositories: result,
    })
}

/// 读一次安装下可访问的仓库（安装范围内的都按可写处理；只读的情况由写命令
/// 自己的 403 兜底）。
async fn installation_repositories(
    client: &reqwest::Client,
    token: &str,
    installation: u64,
) -> Result<Vec<PublishRepository>, String> {
    let url = format!(
        "{API_ENDPOINT}/user/installations/{installation}/repositories?per_page={RELEASE_PAGE_SIZE}"
    );
    let repositories: ApiInstallationRepositories =
        get_json(client, token, &url, "list-installation-repositories").await?;
    Ok(writable_repositories(repositories))
}

/// 按记忆里的 id 直查一次：顺带取仓库范围。404 说明这次安装已经不在（App 被
/// 卸载或换了账号），返回 `None` 让调用方清掉记忆。
async fn remembered_installation_repositories(
    client: &reqwest::Client,
    token: &str,
    installation: u64,
) -> Result<Option<(String, Vec<PublishRepository>)>, String> {
    let response = authorized(
        client.get(&format!("{API_ENDPOINT}/user/installations/{installation}")),
        token,
    )
    .send()
    .await
    .map_err(|error| request_error(&error.to_string()))?;
    // `ensure_success` 会把 404 变成 Err，所以这里必须先看状态码。
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    let detail: ApiInstallationDetail = ensure_success(response, "get-installation")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let repositories = installation_repositories(client, token, installation).await?;
    Ok(Some((detail.repository_selection, repositories)))
}

fn writable_repositories(list: ApiInstallationRepositories) -> Vec<PublishRepository> {
    list.repositories
        .into_iter()
        .filter(|repository| {
            repository
                .permissions
                .as_ref()
                .map(|permissions| permissions.push)
                .unwrap_or(true)
        })
        .map(repository_from_api)
        .collect()
}

/// 记下（或清掉，`installation == 0`）这次安装的 id。写失败只记一条 warn：
/// 记不住只意味着下次还要走一遍安装页，不该让列仓库失败。
fn remember_installation_id(
    service: &crate::config::ConfigService,
    state: &AppState,
    installation: u64,
) {
    let Ok(_guard) = state.operation.lock() else {
        log::warn!("记录安装 id 跳过：操作锁不可用");
        return;
    };
    let mut config = service.load_or_default();
    if service.publish_installation_id(&config) == installation {
        return;
    }
    service.set_publish_installation_id(&mut config, installation);
    if let Err(error) = service.save(&config) {
        log::warn!("记录安装 id 失败：{error}");
    }
}

/// 新建一个地图仓库并自动打上三个标签。
#[tauri::command]
pub async fn create_publish_repository(
    state: State<'_, AppState>,
    name: String,
    private: bool,
    description: String,
) -> Result<PublishRepository, String> {
    let token = publish_token(&state)?;
    let name = name.trim().to_string();
    if !valid_slug(&name) {
        return Err(invalid_name(&name));
    }
    let client = api_client()?;
    let response = authorized(client.post(format!("{API_ENDPOINT}/user/repos")), &token)
        .json(&json!({
            "name": name,
            "description": description.trim(),
            "private": private,
            // 必须有初始提交，否则建不了 tag（Release 会对着默认分支打 tag）。
            "auto_init": true,
        }))
        .send()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let repository: ApiRepository = ensure_success(response, "create-repository")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    log::info!("已新建地图仓库：{}", repository.full_name);

    let topics = set_repository_topics(&client, &token, &repository.full_name).await;
    let topics = match topics {
        Ok(topics) => topics,
        Err(error) => {
            // 仓库已经建成了；标签没打上只影响社区发现，不该让整个动作失败。
            log::warn!(
                "新仓库的标签设置失败（仓库已创建，可稍后重试）：{}，原因={error}",
                repository.full_name
            );
            repository.topics.clone()
        }
    };
    let mut result = repository_from_api(repository);
    result.topics = topics;
    Ok(result)
}

async fn set_repository_topics(
    client: &reqwest::Client,
    token: &str,
    full_name: &str,
) -> Result<Vec<String>, String> {
    let body = json!({ "names": PUBLISH_TOPICS });
    let response = authorized(
        client.put(format!("{API_ENDPOINT}/repos/{full_name}/topics")),
        token,
    )
    .json(&body)
    .send()
    .await
    .map_err(|error| request_error(&error.to_string()))?;
    let value: serde_json::Value = ensure_success(response, "set-topics")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    Ok(value
        .get("names")
        .and_then(|names| names.as_array())
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_else(|| {
            PUBLISH_TOPICS
                .iter()
                .map(|topic| (*topic).to_string())
                .collect()
        }))
}

// —— Release ——

#[tauri::command]
pub async fn list_publish_releases(
    state: State<'_, AppState>,
    owner: String,
    repository: String,
) -> Result<Vec<PublishRelease>, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&owner, &repository)?;
    let client = api_client()?;
    let url =
        format!("{API_ENDPOINT}/repos/{owner}/{repository}/releases?per_page={RELEASE_PAGE_SIZE}");
    let response = authorized(client.get(url.as_str()), &token)
        .send()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let releases: Vec<ApiRelease> = ensure_success(response, "list-releases")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let mut result = releases
        .into_iter()
        .map(release_from_api)
        .collect::<Vec<_>>();
    // 版本高的在前；用字符串比较只能算「大致」，但卡片本来就按版本号排，
    // 这里只需要稳定的展示顺序。
    result.sort_by(|left, right| right.tag.cmp(&left.tag));
    log::info!(
        "发布页 Release 列表加载完成：仓库={owner}/{repository}，条目={}",
        result.len()
    );
    Ok(result)
}

#[tauri::command]
pub async fn save_publish_release(
    state: State<'_, AppState>,
    request: PublishSaveRequest,
) -> Result<PublishRelease, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let tag = validate_tag(&request.tag)?;
    let mut metadata = request.metadata.clone();
    metadata.level_set_name = metadata.level_set_name.trim().to_string();
    metadata.level_set_name_zh = metadata.level_set_name_zh.trim().to_string();
    metadata.author = metadata.author.trim().to_string();
    if metadata.level_set_name.is_empty() {
        return Err("mapPublish.missingLevelSetName".to_string());
    }
    if metadata.asset.trim().is_empty() {
        // 一个版本一个 ZIP：默认按关卡集名生成模板，作者只需要照着传附件。
        let slug = metadata
            .level_set_name
            .chars()
            .map(|item| {
                if item.is_ascii_alphanumeric() {
                    item.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let slug = slug.trim_matches('-');
        metadata.asset = format!("{slug}-{VERSION_PLACEHOLDER}.zip");
    }
    let body = render_release_body(&metadata, &request.body);
    let client = api_client()?;
    let response = match request.release_id {
        Some(release_id) => {
            authorized(
                client.patch(format!(
                    "{API_ENDPOINT}/repos/{owner}/{repository}/releases/{release_id}"
                )),
                &token,
            )
            .json(&json!({
                "tag_name": tag,
                "name": request.title.trim(),
                "body": body,
                "draft": request.draft,
                "prerelease": request.prerelease,
            }))
            .send()
            .await
        }
        None => {
            authorized(
                client.post(format!(
                    "{API_ENDPOINT}/repos/{owner}/{repository}/releases"
                )),
                &token,
            )
            .json(&json!({
                "tag_name": tag,
                "name": request.title.trim(),
                "body": body,
                "draft": request.draft,
                "prerelease": request.prerelease,
            }))
            .send()
            .await
        }
    }
    .map_err(|error| request_error(&error.to_string()))?;
    let release: ApiRelease = ensure_success(response, "save-release")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    log::info!(
        "Release 已保存：仓库={owner}/{repository}，tag={}，新建={}",
        release.tag_name,
        request.release_id.is_none()
    );
    Ok(release_from_api(release))
}

#[tauri::command]
pub async fn delete_publish_release(
    state: State<'_, AppState>,
    request: PublishDeleteReleaseRequest,
) -> Result<OperationResult, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let client = api_client()?;
    let response = authorized(
        client.delete(format!(
            "{API_ENDPOINT}/repos/{owner}/{repository}/releases/{}",
            request.release_id
        )),
        &token,
    )
    .send()
    .await
    .map_err(|error| request_error(&error.to_string()))?;
    ensure_success(response, "delete-release").await?;
    log::info!(
        "Release 已删除：仓库={owner}/{repository}，id={}",
        request.release_id
    );
    if request.delete_tag {
        let tag = validate_tag(&request.tag)?;
        let response = authorized(
            client.delete(format!(
                "{API_ENDPOINT}/repos/{owner}/{repository}/git/refs/tags/{tag}"
            )),
            &token,
        )
        .send()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
        let status = response.status();
        if status.is_success() {
            log::info!("同名 tag 已删除：{tag}");
        } else {
            // 422 是「这个 tag 已经不在了」：发布用的 tag 被别人删过，结果一致。
            let body = response.text().await.unwrap_or_default();
            log::warn!(
                "同名 tag 删除失败（忽略，视为已删除）：{tag}，状态={}，详情={body}",
                status.as_u16()
            );
        }
    }
    Ok(OperationResult {
        ok: true,
        message: "message.mapPublish.releaseDeleted".to_string(),
    })
}

// —— 附件 ——

#[tauri::command]
pub async fn upload_publish_asset(
    state: State<'_, AppState>,
    request: PublishUploadRequest,
) -> Result<PublishAsset, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let name = validate_asset_name(&request.name)?;
    let path = std::path::PathBuf::from(request.path.trim());
    let file_meta = std::fs::metadata(&path).map_err(|error| invalid_asset(&error.to_string()))?;
    if !file_meta.is_file() {
        return Err(invalid_asset("not a file"));
    }
    if file_meta.len() > MAX_ASSET_BYTES {
        return Err(with_params(
            "mapPublish.assetTooLarge",
            &[("limit", &(MAX_ASSET_BYTES / 1024 / 1024).to_string())],
        ));
    }
    // 整份读进内存再上传：地图合集 ZIP 是可接受的量级（上限 256 MiB），
    // 换来的是不需要额外引 tokio 的 fs/stream 特性与第二套流式代码。
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|error| request_error(&error.to_string()))?
        .map_err(|error| invalid_asset(&error.to_string()))?;
    let client = upload_client()?;
    let url = format!(
        "{UPLOAD_ENDPOINT}/repos/{owner}/{repository}/releases/{}/assets?name={}",
        request.release_id,
        encode_component(&name)
    );
    let response = authorized(client.post(url.as_str()), &token)
        .header("Content-Type", "application/zip")
        .body(bytes)
        .send()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let asset: ApiAsset = ensure_success(response, "upload-asset")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    log::info!(
        "附件已上传：仓库={owner}/{repository}，Release={}，名称={}，大小={}",
        request.release_id,
        asset.name,
        asset.size
    );
    Ok(asset_from_api(asset))
}

#[tauri::command]
pub async fn delete_publish_asset(
    state: State<'_, AppState>,
    owner: String,
    repository: String,
    asset_id: u64,
) -> Result<OperationResult, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&owner, &repository)?;
    let client = api_client()?;
    let response = authorized(
        client.delete(format!(
            "{API_ENDPOINT}/repos/{owner}/{repository}/releases/assets/{asset_id}"
        )),
        &token,
    )
    .send()
    .await
    .map_err(|error| request_error(&error.to_string()))?;
    ensure_success(response, "delete-asset").await?;
    log::info!("附件已删除：仓库={owner}/{repository}，id={asset_id}");
    Ok(OperationResult {
        ok: true,
        message: "message.mapPublish.assetDeleted".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_note::NOTE_BLOCK_OPEN;
    use crate::models::MapPublishMetadata;

    #[test]
    fn slugs_reject_path_tricks() {
        assert!(valid_slug("CH3NGYZ"));
        assert!(valid_slug("oc2-map-source.v2"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("../etc"));
        assert!(!valid_slug("owner/repo"));
        assert!(!valid_slug(".hidden"));
        assert!(!valid_slug("名字"));
    }

    #[test]
    fn tags_are_plain_versions_without_a_v_prefix() {
        assert_eq!(validate_tag(" 1.2.0 ").unwrap(), "1.2.0");
        assert_eq!(validate_tag("1.2.0-beta.1").unwrap(), "1.2.0-beta.1");
        assert!(validate_tag("v1.2.0").is_err());
        assert!(validate_tag("").is_err());
        assert!(validate_tag("1.2.0/..").is_err());
    }

    #[test]
    fn asset_names_stay_url_safe() {
        assert_eq!(
            validate_asset_name("gaoda-1.2.0.zip").unwrap(),
            "gaoda-1.2.0.zip"
        );
        assert!(validate_asset_name("地图.zip").is_err());
        assert!(validate_asset_name("").is_err());
        assert!(validate_asset_name(".hidden").is_err());
    }

    #[test]
    fn asset_name_encoding_escapes_reserved_characters() {
        assert_eq!(encode_component("a b.zip"), "a%20b.zip");
        assert_eq!(encode_component("a&b"), "a%26b");
    }

    #[test]
    fn first_sign_in_opens_the_install_page_and_later_ones_do_not() {
        let (mode, url, verifier) = authorization_target(0, "nonce-1", false);
        assert_eq!(mode, PublishAuthMode::Install);
        assert!(url.starts_with(PUBLISH_INSTALL_URL));
        assert!(url.ends_with("?state=nonce-1"));
        // 安装页那条路没有 PKCE 挑战，硬带 verifier 会被 GitHub 判成不匹配。
        assert!(verifier.is_none());

        let (mode, url, verifier) = authorization_target(18334019, "nonce-2", false);
        assert_eq!(mode, PublishAuthMode::Authorize);
        assert!(url.starts_with(AUTHORIZE_ENDPOINT));
        assert!(url.contains("client_id=Iv23liMDOtLBrrkv781U"));
        assert!(url.contains("state=nonce-2"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(verifier.is_some());

        // 装过 App 却没记下 installation id：安装页不会再回调本机，
        // 用户点「改用 GitHub 登录」时必须走 OAuth 那条。
        let (mode, url, verifier) = authorization_target(0, "nonce-3", true);
        assert_eq!(mode, PublishAuthMode::Authorize);
        assert!(url.starts_with(AUTHORIZE_ENDPOINT));
        assert!(verifier.is_some());
    }

    #[test]
    fn release_metadata_survives_the_round_trip_through_the_block() {
        let metadata = MapPublishMetadata {
            level_set_name: "Gaoda".to_string(),
            level_set_name_zh: "高達".to_string(),
            author: "GAODA".to_string(),
            asset: "gaoda-${version}.zip".to_string(),
            requires: [("dev.gua.diylevel".to_string(), ">=4.0.0".to_string())]
                .into_iter()
                .collect(),
            ..MapPublishMetadata::default()
        };
        let body = render_release_body(&metadata, "首个版本");
        let block = crate::map_note::parse_release_note(&body)
            .unwrap()
            .expect("rendered body must contain a block");
        let parsed = publish_metadata_from_note(&block.metadata);
        assert_eq!(parsed.level_set_name, "Gaoda");
        assert_eq!(parsed.level_set_name_zh, "高達");
        assert_eq!(parsed.asset, "gaoda-${version}.zip");
        assert_eq!(
            parsed.requires.get("dev.gua.diylevel").map(String::as_str),
            Some(">=4.0.0")
        );
        assert_eq!(block.body.trim(), "首个版本");
        assert!(body.starts_with(NOTE_BLOCK_OPEN));
    }
}
