use super::http::API_ENDPOINT;
use super::http::REPOSITORY_PAGE_SIZE;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::get_json;
use super::http::request_error;
use super::session::publish_token;
use crate::api_scheduler::ApiRequestExt;
use crate::app_state::AppState;
use crate::models::PublishProvider;
use crate::models::PublishRepository;
use crate::models::PublishRepositoryList;
use serde::Deserialize;
use tauri::State;

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
    #[serde(deserialize_with = "api_bool")]
    push: bool,
}

#[derive(Deserialize)]
struct ApiRepository {
    name: String,
    full_name: String,
    #[serde(default)]
    #[serde(deserialize_with = "api_bool")]
    private: bool,
    #[serde(default)]
    default_branch: String,
    #[serde(default)]
    #[serde(alias = "permission")]
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
    provider: PublishProvider,
) -> Result<PublishRepositoryList, String> {
    let service = state.config.clone();
    let token = publish_token(&state, provider)?;
    let client = api_client()?;
    if provider == PublishProvider::Gitcode {
        return gitcode_repositories(&client, &token).await;
    }
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
        PublishProvider::Github,
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
    result.sort_by(|left, right| left.full_name.cmp(&right.full_name));
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
        "{API_ENDPOINT}/user/installations/{installation}/repositories?per_page={REPOSITORY_PAGE_SIZE}"
    );
    let repositories: ApiInstallationRepositories = get_json(
        client,
        token,
        PublishProvider::Github,
        &url,
        "list-installation-repositories",
    )
    .await?;
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
        PublishProvider::Github,
    )
    .send_api(crate::models::Provider::Github)
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

#[cfg(test)]
mod tests;

fn api_bool<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(value) => Ok(value),
        serde_json::Value::Number(value) if value.as_u64() == Some(0) => Ok(false),
        serde_json::Value::Number(value) if value.as_u64() == Some(1) => Ok(true),
        _ => Err(serde::de::Error::custom("invalid permission")),
    }
}
async fn gitcode_repositories(
    client: &reqwest::Client,
    token: &str,
) -> Result<PublishRepositoryList, String> {
    let mut result = Vec::new();
    let mut page = 1;
    loop {
        let url = format!(
            "{}/user/repos?per_page=100&page={page}",
            PublishProvider::Gitcode.api_endpoint()
        );
        let repositories: Vec<ApiRepository> = get_json(
            client,
            token,
            PublishProvider::Gitcode,
            &url,
            "list-repositories",
        )
        .await?;
        let count = repositories.len();
        result.extend(writable_repositories(ApiInstallationRepositories {
            repositories,
        }));
        if count < 100 {
            break;
        }
        page += 1;
    }
    result.sort_by(|left, right| left.full_name.cmp(&right.full_name));
    result.dedup_by(|left, right| left.full_name == right.full_name);
    Ok(PublishRepositoryList {
        installations: 0,
        all_repositories: false,
        repositories: result,
    })
}
