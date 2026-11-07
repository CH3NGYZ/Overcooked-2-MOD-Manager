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
//! 待授权状态只服务一次授权（最长 [`AUTHORIZE_TIMEOUT`]），不属于持久配置。
//! 模块级 `Mutex` 让同一进程的授权入口共用这一临时状态，进程退出即丢。

use std::collections::{BTreeMap, HashMap};
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

use crate::app_state::AppState;
use crate::display_path::display_path;
use crate::locale_tag::with_params;
use crate::map_note::{
    VERSION_PLACEHOLDER, publish_metadata_from_note, render_release_body,
    replace_body_keeping_block, strip_note_block,
};
use crate::models::{
    MapPublishMetadata, OperationResult, PublishAsset, PublishAuthMode, PublishAuthPoll,
    PublishAuthState, PublishAuthorization, PublishDeleteReleaseRequest, PublishRelease,
    PublishRepository, PublishRepositoryList, PublishRepositoryTopics, PublishSaveRequest,
    PublishSessionCheck, PublishSessionOutcome, PublishTokens, PublishUploadRequest,
    PublishZipInspection,
};
use crate::releases::APP_USER_AGENT;

#[path = "map_publish_zip_cache.rs"]
mod zip_cache;

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

/// 发布检查与社区发现使用同一组地图仓库标签。
const PUBLISH_TOPICS: [&str; 3] = ["overcooked-2", "bepinex", "map"];

fn publish_topics() -> Vec<String> {
    PUBLISH_TOPICS
        .iter()
        .map(|topic| (*topic).to_string())
        .collect()
}

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
///
/// 授权码换令牌与 refresh token 换令牌是同一个端点、同一种响应。App 开启了
/// 「令牌过期」（或授权时带了 `offline_access`）才会有后三个字段；关掉时只有
/// `access_token`，那张令牌不过期，也就不需要刷新。
#[derive(Deserialize)]
struct TokenResponse {
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
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// 把成功响应换算成要落盘的一组令牌；access token 为空时返回 `None`。
///
/// 剩余秒数在这里一次性换成绝对时刻：落盘的是「几点过期」，下次进页面
/// 不需要知道这张令牌是什么时候拿到的。
fn tokens_from_response(body: &TokenResponse, now: i64) -> Option<PublishTokens> {
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
async fn ensure_success(
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

/// tag 是**一张地图**在仓库里的唯一 ID，不是版本号。
///
/// 消费端（`map_sources/release_tags.rs::load_release_tag_maps`）只把这个字符串当身份写进
/// `MapRepositoryConfig.tag`；版本号来自该 Release 的附件名 —— 管理器用
/// `map_note::versioned_asset_rule(metadata.asset, tag)` 的正则从附件名里抽
/// `[0-9]+(?:\.[0-9]+)*`。所以 tag 可以是 `chan`、中文名，只要是一个合法的
/// git ref；同名 tag 下换附件就是发新版本。
///
/// 拒绝项取自 git 的 ref 规则：空格、`~ ^ : ? * [ \`、`..`、`@{`、控制字符、
/// `/`（保持「一个 tag 一张图」的平面结构，也避免进 URL 时编码歧义）、`.` 或
/// `-` 开头（前者 git 不认，后者会被当成命令行选项）、`.` 结尾、`.lock` 结尾
/// （git 自己也不允许）以及超长值。
fn validate_tag(tag: &str) -> Result<String, String> {
    let tag = tag.trim();
    let lower = tag.to_lowercase();
    let forbidden = |item: char| {
        item.is_control() || matches!(item, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    };
    let ok = !tag.is_empty()
        && tag.len() <= 100
        && !tag.starts_with('-')
        && !tag.starts_with('.')
        && !tag.ends_with('.')
        && !lower.ends_with(".lock")
        && !tag.contains('/')
        && !tag.contains("..")
        && !tag.contains("@{")
        && !tag.chars().any(forbidden);
    if !ok {
        return Err(invalid_tag(tag));
    }
    Ok(tag.to_string())
}

fn validate_new_tag(tag: &str) -> Result<String, String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.len() > 100 || !tag.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(invalid_tag(tag));
    }
    Ok(tag.to_string())
}

/// 附件名必须是单个安全文件名；旧 tag 和显式模板可包含中文。
fn validate_asset_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name.chars().all(|item| {
            !item.is_control()
                && !matches!(item, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        && !name.ends_with('.')
        && !name.ends_with(' ');
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
                Ok((tokens, login)) => {
                    {
                        let _guard = operation.lock().map_err(|_| "lock.poisoned".to_string())?;
                        let mut config = service.load_or_default();
                        service
                            .set_publish_tokens(&mut config, &tokens, &login)
                            .map_err(|error| action_error("save-token", &error.to_string()))?;
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
                    log::info!(
                        "GitHub 发布 Token 已保存：用户={login}，安装={installation:?}，可刷新={}",
                        tokens.refresh_token.is_some()
                    );
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

async fn exchange_code(
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
    let Some(tokens) = tokens_from_response(&body, chrono::Utc::now().timestamp()) else {
        return Err(action_error("access-token", "empty token"));
    };
    let login = fetch_login(&client, &tokens.access_token).await?;
    Ok((tokens, login))
}

// —— 会话检查与令牌刷新 ——

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
        .send()
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
        .send()
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
        let left_source = left.topics.iter().any(|topic| topic == "map");
        let right_source = right.topics.iter().any(|topic| topic == "map");
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

/// 仓库已有的标签里缺了哪些必需标签。GitHub 的标签一律小写，比较时不看大小写。
fn missing_topics(topics: &[String]) -> Vec<String> {
    PUBLISH_TOPICS
        .iter()
        .filter(|required| {
            !topics
                .iter()
                .any(|topic| topic.eq_ignore_ascii_case(required))
        })
        .map(|required| (*required).to_string())
        .collect()
}

async fn fetch_repository_topics(
    client: &reqwest::Client,
    token: &str,
    full_name: &str,
) -> Result<Vec<String>, String> {
    let value: serde_json::Value = get_json(
        client,
        token,
        &format!("{API_ENDPOINT}/repos/{full_name}/topics"),
        "get-topics",
    )
    .await?;
    Ok(value
        .get("names")
        .and_then(|names| names.as_array())
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn topics_report(topics: Vec<String>) -> PublishRepositoryTopics {
    let missing = missing_topics(&topics);
    PublishRepositoryTopics {
        topics,
        required: publish_topics(),
        missing,
    }
}

/// 只读检查：选中仓库后立刻问一次标签齐不齐，缺了界面顶部提醒作者自己去仓库设置里补。
#[tauri::command]
pub async fn publish_repository_topics(
    state: State<'_, AppState>,
    owner: String,
    repository: String,
) -> Result<PublishRepositoryTopics, String> {
    let (owner, repository) = validate_coordinates(&owner, &repository)?;
    let token = publish_token(&state)?;
    let client = api_client()?;
    let full_name = format!("{owner}/{repository}");
    let topics = fetch_repository_topics(&client, &token, &full_name).await?;
    Ok(topics_report(topics))
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
    // tag 是地图身份、不是版本号，按它排没有语义，只提供一个稳定顺序；
    // 真正的版本号来自各 Release 的附件名。
    result.sort_by(|left, right| right.tag.cmp(&left.tag));
    log::info!(
        "发布页 Release 列表加载完成：仓库={owner}/{repository}，条目={}",
        result.len()
    );
    Ok(result)
}

/// 排序表非空时用表尾覆盖 `latest_*`。
///
/// 表是作者刚排好的，块里那两个标量是上一版的残留；不覆盖的话新 Release 会
/// 带着旧的「最新版」发出去，管理器下次刷新就把旧版当成最新。
fn apply_table_tail(metadata: &mut MapPublishMetadata) {
    let Some(last) = metadata.asset_versions.last() else {
        return;
    };
    let mapset_version = last.mapset_version.trim();
    if !mapset_version.is_empty() {
        metadata.latest_mapset_version = mapset_version.to_string();
    }
    let info_version = last.info_version.trim();
    if !info_version.is_empty() {
        metadata.latest_info_version = info_version.to_string();
    }
}

fn validate_publish_relations(metadata: &MapPublishMetadata) -> Result<(), String> {
    let invalid = || "mapPublish.invalidRelations".to_string();
    for table in [
        &metadata.requires,
        &metadata.compatible,
        &metadata.conflicts,
    ] {
        for (guid, range) in table {
            if guid.trim().is_empty() {
                return Err(invalid());
            }
            crate::config::check_version_spec(range).map_err(|_| invalid())?;
        }
    }
    for entry in &metadata.release_relations {
        let range =
            crate::version_range::VersionRange::parse(&entry.version).map_err(|_| invalid())?;
        if range.is_any() || entry.relations.is_empty() {
            return Err(invalid());
        }
        crate::config::check_version_spec(&entry.version).map_err(|_| invalid())?;
        for relation in &entry.relations {
            if relation.guid.trim().is_empty() {
                return Err(invalid());
            }
            crate::config::check_version_spec(&relation.version).map_err(|_| invalid())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn save_publish_release(
    state: State<'_, AppState>,
    request: PublishSaveRequest,
) -> Result<PublishRelease, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let tag = if request.release_id.is_none() {
        validate_new_tag(&request.tag)?
    } else {
        validate_tag(&request.tag)?
    };
    let client = api_client()?;
    // 三个步骤各写各的：
    // - 带 metadata（「地图资料」步骤）：块与正文一起重写；
    // - 不带 metadata、改已有 Release（「Release 信息」步骤）：读远端正文，
    //   **保留原有的 oc2-map 块**，只换块外文字 —— 前端拿到的正文已剥掉块；
    // - 不带 metadata、新建：原样写正文（此时还没有块）。
    let body = match request.metadata.clone() {
        Some(mut metadata) => {
            validate_publish_relations(&metadata)?;
            metadata.level_set_name = metadata.level_set_name.trim().to_string();
            metadata.level_set_name_zh = metadata.level_set_name_zh.trim().to_string();
            metadata.author = metadata.author.trim().to_string();
            if metadata.level_set_name.is_empty() {
                return Err("mapPublish.missingLevelSetName".to_string());
            }
            // 身份锁：已有 oc2-map 块时，英文名 / 中文名 / 作者不允许被改写，
            // 否则已安装的用户会因身份不符而检测错乱。
            if let Some(release_id) = request.release_id {
                let remote: ApiRelease = get_json(
                    &client,
                    &token,
                    &format!("{API_ENDPOINT}/repos/{owner}/{repository}/releases/{release_id}"),
                    "read-release",
                )
                .await?;
                if let Ok(Some(note)) = crate::map_note::parse_release_note(&remote.body) {
                    let locked = &note.metadata;
                    if locked.level_set_name != metadata.level_set_name
                        || locked.level_set_name_zh != metadata.level_set_name_zh
                        || locked.author != metadata.author
                    {
                        return Err("mapPublish.zipIdentityMismatch".to_string());
                    }
                }
            }
            apply_table_tail(&mut metadata);
            render_release_body(&metadata, &request.body)
        }
        None => match request.release_id {
            Some(release_id) => {
                let remote: ApiRelease = get_json(
                    &client,
                    &token,
                    &format!("{API_ENDPOINT}/repos/{owner}/{repository}/releases/{release_id}"),
                    "read-release",
                )
                .await?;
                replace_body_keeping_block(&remote.body, &request.body)
            }
            None => request.body.clone(),
        },
    };
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
                "name": tag,
                "body": body,
                "draft": false,
                "prerelease": false,
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
                "name": tag,
                "body": body,
                "draft": false,
                "prerelease": false,
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
        // tag 允许中文等非 ASCII 字符，进 URL 路径前必须百分号编码。
        let tag_path = encode_component(&tag);
        let response = authorized(
            client.delete(format!(
                "{API_ENDPOINT}/repos/{owner}/{repository}/git/refs/tags/{tag_path}"
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
    on_progress: tauri::ipc::Channel<crate::models::PublishUploadProgress>,
) -> Result<PublishAsset, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let client = api_client()?;
    let remote: ApiRelease = get_json(
        &client,
        &token,
        &format!(
            "{API_ENDPOINT}/repos/{owner}/{repository}/releases/{}",
            request.release_id
        ),
        "read-release",
    )
    .await?;
    let path = std::path::PathBuf::from(request.path.trim());
    let inspect_path = path.clone();
    let inspection =
        tauri::async_runtime::spawn_blocking(move || zip_cache::inspect(&inspect_path))
            .await
            .map_err(|error| request_error(&error.to_string()))??;
    let note =
        crate::map_note::parse_release_note(&remote.body).map_err(|error| invalid_asset(&error))?;
    validate_zip_identity(note.as_ref().map(|note| &note.metadata), &inspection)?;
    let name = upload_asset_name(
        &remote.tag_name,
        note.as_ref()
            .and_then(|note| note.metadata.asset.as_deref()),
        &inspection.info_version,
    )?;
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
    let total = file_meta.len();
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|error| invalid_asset(&error.to_string()))?;
    let progress = on_progress.clone();
    let _ = progress.send(crate::models::PublishUploadProgress {
        sent: 0,
        total,
        phase: "uploading",
    });
    let stream = futures_util::stream::try_unfold((file, 0u64), move |(mut file, sent)| {
        let progress = progress.clone();
        async move {
            use tokio::io::AsyncReadExt;
            let mut buffer = vec![0u8; 64 * 1024];
            let count = file.read(&mut buffer).await?;
            if count == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            buffer.truncate(count);
            let sent = sent + count as u64;
            let _ = progress.send(crate::models::PublishUploadProgress {
                sent,
                total,
                phase: if sent >= total {
                    "confirming"
                } else {
                    "uploading"
                },
            });
            Ok(Some((buffer, (file, sent))))
        }
    });
    let client = upload_client()?;
    let url = format!(
        "{UPLOAD_ENDPOINT}/repos/{owner}/{repository}/releases/{}/assets?name={}",
        request.release_id,
        encode_component(&name)
    );
    let response = authorized(client.post(url.as_str()), &token)
        .header("Content-Type", "application/zip")
        .header("Content-Length", total)
        .body(reqwest::Body::wrap_stream(stream))
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
    let _ = on_progress.send(crate::models::PublishUploadProgress {
        sent: total,
        total,
        phase: "done",
    });
    Ok(asset_from_api(asset))
}

fn validate_zip_identity(
    metadata: Option<&crate::map_note::MapNoteMetadata>,
    inspection: &PublishZipInspection,
) -> Result<(), String> {
    if inspection.set_count != 1 {
        return Err(invalid_asset("ZIP must contain exactly one level set"));
    }
    if let Some(metadata) = metadata {
        if metadata.level_set_name != inspection.level_set_name
            || metadata.level_set_name_zh != inspection.level_set_name_zh
            || metadata.author != inspection.author
        {
            return Err("mapPublish.zipIdentityMismatch".to_string());
        }
    }
    Ok(())
}

fn upload_asset_name(tag: &str, template: Option<&str>, version: &str) -> Result<String, String> {
    if version.is_empty() {
        return Err(invalid_asset("missing info version"));
    }
    let default_template = format!("{tag}_v{VERSION_PLACEHOLDER}.zip");
    let template = template
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&default_template);
    if !template.contains(VERSION_PLACEHOLDER) {
        return Err(invalid_asset("missing version placeholder"));
    }
    let name = validate_asset_name(&template.replace(VERSION_PLACEHOLDER, version))?;
    if !name.to_ascii_lowercase().ends_with(".zip") {
        return Err(invalid_asset("extension"));
    }
    Ok(name)
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

/// 从 ZIP 文件名里取版本号：取最后一段「数字和点」。
///
/// 作者的地图包通常叫 `gaoda-1.2.0.zip`，版本号只出现在文件名里；这个名字就是
/// Release note 的 `latest_mapset_version`（地图源按附件名抽版本，见
/// `map_note::versioned_asset_rule`）。解析不出就返回空串，界面让作者手填。
///
/// 优先多段（`1.2.0` 比 `1` 更像版本号），没有多段时才退回最后一段纯数字。
/// 文件名只有日期（`2026-11-04`）时只能取到 `04` —— 这种名字本来就没有版本，
/// 界面上填错的成本只是改两个字。
fn version_from_file_name(stem: &str) -> String {
    let mut runs: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    let valid = |run: &String| {
        let trimmed = run.trim_matches('.');
        !trimmed.is_empty()
            && trimmed
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    };
    runs.iter()
        .rev()
        .find(|run| valid(run) && run.trim_matches('.').contains('.'))
        .or_else(|| runs.iter().rev().find(|run| valid(run)))
        .map(|run| run.trim_matches('.').to_string())
        .unwrap_or_default()
}

/// 从 ZIP 里读关卡集根目录的 `requires.txt`：第一行有效内容是最低 runtime 版本。
///
/// 与已安装目录同一条判定（`level_sets::read_required_runtime` 只看集目录根）：
/// 只认相对路径恰好等于 `requires.txt` 的条目，子目录里的同名文件不算。
fn zip_required_runtime(path: &std::path::Path, files: &[(String, String)]) -> Option<String> {
    let entry_name = files
        .iter()
        .find(|(_, relative)| relative.eq_ignore_ascii_case("requires.txt"))?
        .0
        .clone();
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(&entry_name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

fn inspect_publish_zip_blocking(path: &std::path::Path) -> Result<PublishZipInspection, String> {
    if !path.is_file() {
        return Err("mapPublish.zipUnreadable".to_string());
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    // 发布页只需要一张图的身份与版本；合集 ZIP 也先全部识别出来，让界面能
    // 明确告诉作者「这个包里有多张图」，而不是悄悄挑第一张。
    let sets = crate::level_sets::inspect_zip_with_progress(
        path,
        crate::metadata_read::workers_from(crate::metadata_read::AUTO_CONCURRENCY),
        crate::metadata_read::workers_from(crate::metadata_read::AUTO_CONCURRENCY),
        &|_, _, _| {},
    )?;
    let first = sets
        .first()
        .ok_or_else(|| "mapPublish.zipNoLevelSet".to_string())?;
    let requires = zip_required_runtime(path, &first.files)
        .and_then(|min_version| crate::level_sets::runtime_relation(&min_version))
        .map(|relation| {
            let mut table = BTreeMap::new();
            table.insert(relation.guid, relation.version);
            table
        })
        .unwrap_or_default();
    Ok(PublishZipInspection {
        file_size: std::fs::metadata(path)
            .map_err(|_| "mapPublish.zipUnreadable".to_string())?
            .len(),
        file_name,
        mapset_version: version_from_file_name(stem),
        info_version: first.metadata.version.clone(),
        level_set_name: first.metadata.name.clone(),
        level_set_name_zh: first.metadata.name_zh.clone().unwrap_or_default(),
        author: first.metadata.author.clone(),
        level_count: first.metadata.levels.len(),
        file_count: first.files.len(),
        set_count: sets.len(),
        requires,
    })
}

/// 选择或拖入地图 ZIP 时本地识别，不要求已有 Release。
///
/// 会解析 `info`（UnityFS + LZMA），属于重活，必须走 `spawn_blocking`：
/// 同步命令会冻住主线程（见 `AGENTS.md` 第 5 节）。
#[tauri::command]
pub async fn inspect_publish_zip(path: String) -> Result<PublishZipInspection, String> {
    let path = std::path::PathBuf::from(path.trim());
    tauri::async_runtime::spawn_blocking(move || zip_cache::inspect(&path))
        .await
        .map_err(|error| request_error(&error.to_string()))?
}

/// Release 附件暂存目录（数据根下 `cache/publish-staging`）。
const PUBLISH_STAGING_DIR: &str = "publish-staging";

/// 把作者选中的本地 ZIP 复制成 Release 附件该用的名字。
///
/// 上传时附件名必须等于块里 `asset` 模板替换出版本后的样子，而用户手里的文件
/// 名通常不是；复制到数据根的暂存目录再上传，既不改用户原文件，也让上传只认
/// 一个确定的路径（界面可以在写块之后、上传之前反复重试）。
#[tauri::command]
pub async fn stage_publish_zip(
    state: State<'_, AppState>,
    path: String,
    file_name: String,
) -> Result<String, String> {
    // `State` 不能跨 await，先把要用的路径取出来再进阻塞线程池（几十 MB 的
    // 复制是 IO 活，同步命令会冻住主线程，见 `AGENTS.md` 第 5 节）
    let staging = state.config.cache_dir().join(PUBLISH_STAGING_DIR);
    tauri::async_runtime::spawn_blocking(move || {
        stage_publish_zip_into(&staging, &path, &file_name)
    })
    .await
    .map_err(|error| request_error(&error.to_string()))?
}

/// [`stage_publish_zip`] 的同步实现（拆出来才能在临时目录上测）。
fn stage_publish_zip_into(
    staging: &std::path::Path,
    path: &str,
    file_name: &str,
) -> Result<String, String> {
    let file_name = validate_asset_name(file_name)?;
    // 只收 ZIP：发布页第二步传的就是地图包，别的文件传上去没人装得下
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Err(invalid_asset("extension"));
    }
    let source = std::path::PathBuf::from(path.trim());
    let is_zip = source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    if !is_zip || !source.is_file() {
        log::warn!(
            "暂存发布附件失败：源文件不是可读的 ZIP，路径={}",
            display_path(&source)
        );
        return Err("mapPublish.zipUnreadable".to_string());
    }
    std::fs::create_dir_all(staging).map_err(|error| {
        log::error!(
            "创建发布附件暂存目录失败：目录={}，原因={error}",
            display_path(staging)
        );
        action_error("stage", &error.to_string())
    })?;
    // 每次使用独立目录，重试或源文件本身在暂存目录时也不会覆盖原文件。
    let staging = staging.join(uuid::Uuid::new_v4().simple().to_string());
    std::fs::create_dir(&staging).map_err(|error| action_error("stage", &error.to_string()))?;
    let target = staging.join(&file_name);
    std::fs::copy(&source, &target).map_err(|error| {
        log::error!(
            "复制发布附件失败：源={}，目标={}，原因={error}",
            display_path(&source),
            display_path(&target)
        );
        action_error("stage", &error.to_string())
    })?;
    log::info!("发布附件已暂存：目标={}", display_path(&target));
    Ok(target.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_note::NOTE_BLOCK_OPEN;
    use crate::models::{AssetVersionEntry, MapPublishMetadata};

    #[test]
    fn publishing_rejects_invalid_relation_overrides_before_writing() {
        let mut metadata: MapPublishMetadata = serde_json::from_value(serde_json::json!({
            "release_relations": [{ "version": ">=0.9", "relations": [
                { "kind": "requires", "guid": "dep", "version": ">=1.7" }
            ] }]
        }))
        .unwrap();
        assert!(validate_publish_relations(&metadata).is_ok());
        for version in ["", "*", "~=1", ">=2,<1"] {
            metadata.release_relations[0].version = version.to_string();
            assert_eq!(
                validate_publish_relations(&metadata).unwrap_err(),
                "mapPublish.invalidRelations"
            );
        }
        metadata.release_relations[0].version = "==0.1".to_string();
        metadata.release_relations[0].relations[0].version = ">=2,<1".to_string();
        assert!(validate_publish_relations(&metadata).is_err());
        metadata.release_relations[0].relations.clear();
        assert!(validate_publish_relations(&metadata).is_err());
        metadata.release_relations.clear();
        metadata.requires.insert("dep".to_string(), "".to_string());
        assert!(validate_publish_relations(&metadata).is_ok());
    }

    fn token_body(json: &str) -> TokenResponse {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn expiring_token_responses_become_absolute_deadlines() {
        let body = token_body(
            r#"{"access_token":"ghu_a","expires_in":28800,"refresh_token":"ghr_b",
                "refresh_token_expires_in":15897600,"scope":"","token_type":"bearer"}"#,
        );
        let tokens = tokens_from_response(&body, 1_000).unwrap();
        assert_eq!(tokens.access_token, "ghu_a");
        assert_eq!(tokens.refresh_token.as_deref(), Some("ghr_b"));
        assert_eq!(tokens.expires_at, Some(1_000 + 28_800));
        assert_eq!(tokens.refresh_expires_at, Some(1_000 + 15_897_600));
    }

    #[test]
    fn non_expiring_token_responses_have_no_refresh_side() {
        // App 关了令牌过期：只有 access_token，不过期，也不需要刷新
        let tokens =
            tokens_from_response(&token_body(r#"{"access_token":"ghu_a"}"#), 1_000).unwrap();
        assert_eq!(tokens.refresh_token, None);
        assert_eq!(tokens.expires_at, None);
        assert_eq!(tokens.refresh_expires_at, None);
        assert!(tokens_from_response(&token_body(r#"{"access_token":"  "}"#), 1).is_none());
        assert!(tokens_from_response(&token_body(r#"{"error":"bad_refresh_token"}"#), 1).is_none());
    }

    #[test]
    fn access_tokens_count_as_expired_shortly_before_the_deadline() {
        assert!(!access_token_expired(None, 10_000));
        assert!(!access_token_expired(
            Some(10_000 + ACCESS_EXPIRY_MARGIN_SECS + 1),
            10_000
        ));
        assert!(access_token_expired(
            Some(10_000 + ACCESS_EXPIRY_MARGIN_SECS),
            10_000
        ));
        assert!(access_token_expired(Some(9_000), 10_000));
    }

    #[test]
    fn refresh_tokens_must_exist_and_be_unexpired() {
        assert!(refresh_token_usable(Some("ghr_b"), None, 10));
        assert!(refresh_token_usable(Some("ghr_b"), Some(11), 10));
        assert!(!refresh_token_usable(Some("ghr_b"), Some(10), 10));
        assert!(!refresh_token_usable(Some(" "), None, 10));
        assert!(!refresh_token_usable(None, None, 10));
    }

    #[test]
    fn refresh_rejections_map_to_reauthorize_tags() {
        assert_eq!(
            refresh_rejection_reason("bad_refresh_token", Some("expired")),
            "mapPublish.sessionExpired"
        );
        assert!(
            refresh_rejection_reason("unauthorized_client", Some("nope"))
                .starts_with("mapPublish.sessionRefreshRejected?")
        );
    }

    #[test]
    fn the_authorize_url_requests_offline_access() {
        let (_, url, _) = authorization_target(1, "nonce", false);
        assert!(url.contains("scope=offline_access"));
    }

    #[test]
    fn forbidden_without_headers_still_reports_the_github_message() {
        let tag = classify_write_failure(
            403,
            None,
            None,
            None,
            "Resource not accessible by integration",
        )
        .expect("403 必须归类");
        assert!(tag.starts_with("mapPublish.forbidden?reason="), "{tag}");
        assert!(tag.contains("Resource%20not%20accessible"), "{tag}");
    }

    #[test]
    fn accepted_permissions_become_a_permission_hint() {
        let tag = classify_write_failure(
            403,
            Some("contents=write,administration=write"),
            None,
            None,
            "Resource not accessible by integration",
        )
        .expect("403 必须归类");
        assert_eq!(
            tag,
            "mapPublish.missingPermission?permission=contents%3Dwrite%2Cadministration%3Dwrite"
        );
    }

    #[test]
    fn rate_limited_writes_carry_the_reset_time() {
        let tag = classify_write_failure(
            403,
            None,
            Some("0"),
            Some("1762264817"),
            "API rate limit exceeded",
        )
        .expect("403 必须归类");
        assert_eq!(tag, "mapPublish.rateLimited?reset=1762264817");
        // 次要限流的正文里有 "rate limit"，但没有 x-ratelimit-reset 头。
        let secondary = classify_write_failure(
            403,
            None,
            None,
            None,
            "You have exceeded a secondary rate limit",
        )
        .expect("次要限流也必须归类");
        assert_eq!(secondary, "mapPublish.rateLimited?reset=");
    }

    #[test]
    fn other_statuses_map_to_their_own_tags() {
        assert_eq!(
            classify_write_failure(401, None, None, None, "Bad credentials"),
            Some("mapPublish.unauthorized".to_string())
        );
        assert!(
            classify_write_failure(404, None, None, None, "Not Found")
                .expect("404 必须归类")
                .starts_with("mapPublish.notFound?reason=")
        );
        assert!(
            classify_write_failure(422, None, None, None, "Validation Failed")
                .expect("422 必须归类")
                .starts_with("mapPublish.rejected?reason=")
        );
        // 其它状态交给动作名兜底，分类函数不吞掉它。
        assert_eq!(classify_write_failure(500, None, None, None, "boom"), None);
    }

    #[test]
    fn zip_file_names_carry_the_version() {
        assert_eq!(version_from_file_name("gaoda"), "");
        assert_eq!(version_from_file_name("gaoda-1.2.0"), "1.2.0");
        assert_eq!(version_from_file_name("GAODA_1.0.3_beta"), "1.0.3");
        assert_eq!(version_from_file_name("地图-3.4"), "3.4");
        assert_eq!(version_from_file_name("v2"), "2");
        assert_eq!(version_from_file_name("gaoda..1.2."), "1.2");
        // 只有日期的名字本来就没有版本：取到 04，界面让作者改。
        assert_eq!(version_from_file_name("2026-11-04"), "04");
    }

    #[test]
    fn missing_topics_only_lists_the_ones_absent() {
        assert_eq!(
            missing_topics(&["overcooked-2".to_string(), "bepinex".to_string()]),
            vec!["map".to_string()]
        );
        assert!(missing_topics(&publish_topics()).is_empty());
        assert!(
            missing_topics(&[
                "Overcooked-2".to_string(),
                "bepinex".to_string(),
                "map".to_string(),
                "extra".to_string(),
            ])
            .is_empty()
        );
    }

    #[test]
    fn slugs_reject_path_tricks() {
        assert!(valid_slug("CH3NGYZ"));
        assert!(valid_slug("oc2-map.v2"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("../etc"));
        assert!(!valid_slug("owner/repo"));
        assert!(!valid_slug(".hidden"));
        assert!(!valid_slug("名字"));
    }

    #[test]
    fn tags_are_map_ids_that_only_need_to_be_git_refs() {
        // tag 是地图身份：chan、中文名都行，同名 tag 下换附件就是发新版。
        assert_eq!(validate_tag(" chan ").unwrap(), "chan");
        assert_eq!(validate_tag("大厨-合集").unwrap(), "大厨-合集");
        assert_eq!(validate_tag("1.2.0-beta.1").unwrap(), "1.2.0-beta.1");
        // 以前被当成「版本号必须无 v 前缀」拦掉的值，现在都是合法的地图 ID。
        assert_eq!(validate_tag("v1.2.0").unwrap(), "v1.2.0");
        assert!(validate_tag("").is_err());
        assert!(validate_tag("a b").is_err());
        assert!(validate_tag("-v1").is_err());
        assert!(validate_tag("1.2.0/..").is_err());
        assert!(validate_tag("maps/chan").is_err());
        assert!(validate_tag("a..b").is_err());
        assert!(validate_tag("a:b").is_err());
        assert!(validate_tag("x.lock").is_err());
    }

    #[test]
    fn asset_names_keep_unicode_but_reject_path_and_reserved_characters() {
        assert_eq!(
            validate_asset_name("gaoda-1.2.0.zip").unwrap(),
            "gaoda-1.2.0.zip"
        );
        // 作者的中文文件名原样上传；进 URL 时由 encode_component 按 UTF-8 字节编码。
        assert_eq!(validate_asset_name(" 地图.zip ").unwrap(), "地图.zip");
        assert_eq!(encode_component("地图.zip"), "%E5%9C%B0%E5%9B%BE.zip");
        for bad in [
            "",
            ".hidden",
            "a/b.zip",
            "a\\b.zip",
            "a:b.zip",
            "a?b.zip",
            "a|b.zip",
            "a\u{7}.zip",
            "map.",
        ] {
            assert!(validate_asset_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_asset_name(&"a".repeat(201)).is_err());
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

    #[test]
    fn the_table_tail_overrides_the_stale_latest_versions() {
        let mut metadata = MapPublishMetadata {
            latest_mapset_version: "0.7".to_string(),
            latest_info_version: "0.7".to_string(),
            asset_versions: vec![
                AssetVersionEntry {
                    mapset_version: "0.7".to_string(),
                    info_version: "0.7".to_string(),
                },
                AssetVersionEntry {
                    mapset_version: " 0.69 ".to_string(),
                    info_version: "0.69".to_string(),
                },
            ],
            ..MapPublishMetadata::default()
        };
        apply_table_tail(&mut metadata);
        assert_eq!(metadata.latest_mapset_version, "0.69");
        assert_eq!(metadata.latest_info_version, "0.69");
        // 表尾那一格的版本号空着时不能把已有值抹成空
        metadata.asset_versions.push(AssetVersionEntry::default());
        apply_table_tail(&mut metadata);
        assert_eq!(metadata.latest_mapset_version, "0.69");
        // 空表完全不动：老源的 latest_* 仍然只有一个来源
        metadata.asset_versions.clear();
        apply_table_tail(&mut metadata);
        assert_eq!(metadata.latest_mapset_version, "0.69");
    }

    #[test]
    fn a_staged_zip_is_copied_under_the_release_name() {
        let root = std::env::temp_dir().join(format!("oc2-publish-stage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("我的 地图-v1.zip");
        std::fs::write(&source, b"zip-bytes").unwrap();
        let staging = root.join("staging");
        let staged =
            stage_publish_zip_into(&staging, source.to_str().unwrap(), "gaoda_v1.2.0.zip").unwrap();
        assert_eq!(
            std::path::Path::new(&staged)
                .file_name()
                .and_then(|name| name.to_str()),
            Some("gaoda_v1.2.0.zip")
        );
        assert_eq!(std::fs::read(&staged).unwrap(), b"zip-bytes");
        // 用户手里的原文件原样留着
        assert!(source.is_file());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn staging_rejects_anything_that_is_not_a_readable_zip() {
        let root = std::env::temp_dir().join(format!("oc2-publish-stage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let text = root.join("notes.txt");
        std::fs::write(&text, b"nope").unwrap();
        // 附件名必须是 ZIP
        assert!(stage_publish_zip_into(&root, text.to_str().unwrap(), "gaoda_v1.0.0.txt").is_err());
        // 源文件本身也必须是 ZIP
        assert!(stage_publish_zip_into(&root, text.to_str().unwrap(), "gaoda_v1.0.0.zip").is_err());
        // 不存在的源文件
        assert!(stage_publish_zip_into(&root, "", "gaoda_v1.0.0.zip").is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
