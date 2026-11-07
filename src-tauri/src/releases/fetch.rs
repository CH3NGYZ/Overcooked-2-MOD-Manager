use super::coordinates::repository_file_url;
use super::coordinates::repository_release_api_url;
use super::coordinates::repository_release_api_url_page;
use super::error::ReleaseError;
use super::error::release_error_kind;
use super::transport::api_request_targets;
use super::transport::api_target_kind;
use super::transport::fetch_api_with_retries;
use super::transport::fetch_release_api_with_retries;
use crate::models::GitHubProxy;
use crate::models::Provider;
use crate::models::ReleaseInfo;
use crate::models::RepositoryConfig;
use reqwest::Client;
use serde_json::Value;
use url::Url;

pub async fn fetch_releases(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let per_page = repo.max_release_count.clamp(1, 100);
    fetch_releases_page(
        client,
        repo,
        per_page,
        None,
        token,
        official_api_only,
        api_proxies,
    )
    .await
}

/// 分页读取一个仓库的全部 Release。
///
/// 只有「一个仓库多张地图」的源需要它：那种源的地图数量与 Release 数量同阶，
/// 只读第 1 页会漏掉排在后面的地图。MOD 与普通地图仍走 `fetch_releases`
/// （只取第 1 页），不因为这条新路径变成每次都翻几十页。
///
/// 停止条件三条，缺一不可：
/// 1. 解析后的 Release 数不足一页（跳过无 tag 条目也会缩短这一计数）；
/// 2. 翻到 `max_pages` 上限 —— 防止一次刷新把几千个 Release 全拉下来；
/// 3. 中途失败 —— 已经取到的部分照常返回：前几页的地图不会因为第 N 页
///    限流而整块消失（首屏换一次 warn 日志）。
pub async fn fetch_all_releases(
    client: &Client,
    repo: &RepositoryConfig,
    per_page: u32,
    max_pages: u32,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let per_page = per_page.clamp(1, 100);
    let max_pages = max_pages.clamp(1, 100);
    let mut all = Vec::new();
    for index in 0..max_pages {
        let page = index + 1;
        let requested = (page > 1).then_some(page);
        let page_releases = match fetch_releases_page(
            client,
            repo,
            per_page,
            requested,
            token,
            official_api_only,
            api_proxies,
        )
        .await
        {
            Ok(releases) => releases,
            Err(error) => {
                if all.is_empty() {
                    return Err(error);
                }
                log::warn!(
                    "分页读取 Release 中途失败，保留已取到的部分: repository_id={} page={} error_kind={} got={}",
                    repo.id,
                    page,
                    release_error_kind(&error),
                    all.len()
                );
                return Ok(all);
            }
        };
        let received = page_releases.len();
        all.extend(page_releases);
        if received < per_page as usize {
            break;
        }
    }
    Ok(all)
}

async fn fetch_releases_page(
    client: &Client,
    repo: &RepositoryConfig,
    per_page: u32,
    page: Option<u32>,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let per_page = per_page.clamp(1, 100);
    let official = match repository_release_api_url_page(
        &repo.provider,
        &repo.username,
        &repo.repository,
        per_page,
        page,
    ) {
        Ok(url) => url,
        Err(error) => {
            log::warn!(
                "Release API 仓库地址校验失败: repository_id={} provider={:?} error_kind={}",
                repo.id,
                repo.provider,
                release_error_kind(&error)
            );
            return Err(error);
        }
    };
    let request_targets = api_request_targets(&official, token, official_api_only, api_proxies);
    log::debug!(
        "开始获取 Release: repository_id={} provider={:?} channel={:?} per_page={} page={} token_configured={} official_api_only={} api_target_count={}",
        repo.id,
        repo.provider,
        repo.channel,
        per_page,
        page.unwrap_or(1),
        token.is_some(),
        official_api_only,
        request_targets.len()
    );
    fetch_releases_from_targets(client, repo, &official, request_targets).await
}

async fn fetch_releases_from_targets(
    client: &Client,
    repo: &RepositoryConfig,
    official: &str,
    request_targets: Vec<(String, Option<&str>)>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let mut last_error = None;
    let mut skip_gitee_official = false;
    let gate = crate::api_rate_gate::global_gate();
    for (index, (url, request_token)) in request_targets.into_iter().enumerate() {
        if skip_gitee_official && url == official {
            continue;
        }
        // 官方地址才受限流闸门约束；代理请求不消耗官方配额
        let gate_kind = if url == official {
            if request_token.is_some() {
                crate::api_rate_gate::GateKind::Token
            } else {
                crate::api_rate_gate::GateKind::Anonymous
            }
        } else {
            crate::api_rate_gate::GateKind::Proxy
        };
        if let Err(wait) = gate.check(&repo.provider, gate_kind) {
            log::warn!(
                "Release API 被限流闸门跳过: repository_id={} provider={:?} target_kind={} 剩余等待={}秒",
                repo.id,
                repo.provider,
                api_target_kind(official, &url, request_token),
                wait.as_secs()
            );
            last_error = Some(ReleaseError::Http("source.apiRateLimited".into()));
            continue;
        }
        let target_kind = api_target_kind(official, &url, request_token);
        log::debug!(
            "尝试 Release API: repository_id={} provider={:?} target_kind={} target_index={} token_attached={}",
            repo.id,
            repo.provider,
            target_kind,
            index + 1,
            request_token.is_some()
        );
        match fetch_release_api_with_retries(client, &url, &repo.provider, request_token).await {
            Ok(mut releases) => {
                gate.record_success(&repo.provider, gate_kind);
                for release in &mut releases {
                    release.assets.retain(|asset| {
                        !crate::community::is_source_archive(repo, &release.tag_name, &asset.name)
                    });
                }
                log::info!(
                    "Release API 获取成功: repository_id={} provider={:?} target_kind={} release_count={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    releases.len()
                );
                return Ok(releases);
            }
            Err(error) => {
                // 命中限流特征 → 给该类凭据上锁，后续同类官方请求改走代理
                if let Some(signal) = error.rate_limit_signal() {
                    if let Some(block) = gate.record_forbidden(&repo.provider, gate_kind, signal) {
                        log::warn!(
                            "Release API 触发限流锁: provider={:?} target_kind={} 锁定={}秒",
                            repo.provider,
                            target_kind,
                            block.as_secs()
                        );
                    }
                }
                log::warn!(
                    "Release API 目标失败: repository_id={} provider={:?} target_kind={} error_kind={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    release_error_kind(&error)
                );
                if repo.provider == Provider::Gitee && error.is_forbidden() {
                    if url != official {
                        log::warn!(
                            "Gitee API 代理返回 403，结束刷新: repository_id={}",
                            repo.id
                        );
                        return Err(error);
                    }
                    log::debug!(
                        "Gitee 官方 API 返回 403，跳过官方 Token 目标并转 API 代理: repository_id={}",
                        repo.id
                    );
                    skip_gitee_official = true;
                }
                last_error = Some(error);
            }
        }
    }
    let error = last_error.expect("release API request targets must not be empty");
    log::error!(
        "Release API 所有目标均失败: repository_id={} provider={:?} error_kind={}",
        repo.id,
        repo.provider,
        release_error_kind(&error)
    );
    Err(error)
}

pub async fn fetch_repository_default_branch(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    api_proxies: &[GitHubProxy],
) -> Result<String, ReleaseError> {
    let mut url = Url::parse(&repository_release_api_url(
        &repo.provider,
        &repo.username,
        &repo.repository,
        1,
    )?)
    .map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    url.path_segments_mut()
        .expect("validated repository API URL")
        .pop();
    url.set_query(None);
    let official = url.to_string();
    let mut last_error = None;
    let mut skip_official = false;
    for (url, request_token) in api_request_targets(&official, token, false, api_proxies) {
        if skip_official && url == official {
            continue;
        }
        log::info!(
            "正在读取仓库默认分支：仓库={}，渠道={}",
            repo.id,
            api_target_kind(&official, &url, request_token)
        );
        let fetched =
            fetch_api_with_retries(client, &url, &repo.provider, request_token, |value| {
                let branch = value
                    .get("default_branch")
                    .and_then(Value::as_str)
                    .filter(|branch| !branch.trim().is_empty())
                    .ok_or_else(|| ReleaseError::Json("source.defaultBranchMissing".into()))?;
                repository_file_url(
                    &repo.provider,
                    &repo.username,
                    &repo.repository,
                    branch,
                    "README.md",
                )?;
                Ok(branch.to_string())
            })
            .await;
        match fetched {
            Ok(branch) => return Ok(branch),
            Err(error) => {
                if repo.provider == Provider::Gitee && error.is_forbidden() {
                    if url != official {
                        return Err(error);
                    }
                    skip_official = true;
                }
                last_error = Some(error);
            }
        }
    }
    Err(last_error.expect("repository API always has an official target"))
}

#[cfg(test)]
mod tests;
