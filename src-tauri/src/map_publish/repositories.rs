use super::http::API_ENDPOINT;
use super::http::RELEASE_PAGE_SIZE;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::get_json;
use super::http::request_error;
use super::session::publish_token;
use super::validation::validate_coordinates;
use crate::app_state::AppState;
use crate::models::PublishRepository;
use crate::models::PublishRepositoryList;
use crate::models::PublishRepositoryTopics;
use serde::Deserialize;
use tauri::State;

/// 发布检查与社区发现使用同一组地图仓库标签。
const PUBLISH_TOPICS: [&str; 3] = ["overcooked-2", "bepinex", "map"];

fn publish_topics() -> Vec<String> {
    PUBLISH_TOPICS
        .iter()
        .map(|topic| (*topic).to_string())
        .collect()
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
        // 全部仓库范围自动包含以后新建的仓库；部分范围需要作者去安装设置补选。
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

/// 只读安装范围内第一页仓库；明确 push=false 的条目被过滤，
/// 缺 permissions 时保留候选，真实写权限仍由服务器判断。
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

#[cfg(test)]
mod tests;
