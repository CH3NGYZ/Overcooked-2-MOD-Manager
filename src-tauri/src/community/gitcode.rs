use crate::api_scheduler::ApiRequestExt;
pub const GITCODE_SEARCH_ENDPOINT: &str = "https://api.atomgit.com/api/v5/search/repositories";

use super::catalog::{SearchHit, SearchResults, classify_provider_item};
use super::state::SearchRateLimits;
use crate::models::{CommunityDiscovery, Provider};
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

/// AtomGit documents keyword search, not GitHub's topic query syntax. Require
/// explicit topic data before committing a new discovery snapshot.
pub(super) async fn discover(
    client: &reqwest::Client,
    token: Option<&str>,
    limits: &mut SearchRateLimits,
) -> Result<SearchResults, String> {
    let token = token
        .filter(|value| !value.trim().is_empty())
        .ok_or("community.gitcodeTokenRequired")?;
    let mut result = SearchResults {
        repositories: Vec::new(),
        map_repositories: Vec::new(),
        discovery: CommunityDiscovery::default(),
    };
    let mut seen = HashSet::new();
    for page in 1..=100 {
        limits.before_request("official").await?;
        let mut url = url::Url::parse(GITCODE_SEARCH_ENDPOINT).expect("fixed official URL");
        url.query_pairs_mut()
            .append_pair("q", "overcooked")
            .append_pair("per_page", "50")
            .append_pair("page", &page.to_string());
        let response =
            crate::providers::api_request(client, url.as_str(), &Provider::Gitcode, Some(token))
                .map_err(|_| "community.searchUrlInvalid")?
                .timeout(Duration::from_secs(5))
                .send_api(crate::models::Provider::Gitcode)
                .await
                .map_err(|error| error.without_url().to_string())?;
        limits.observe("official", response.status(), response.headers());
        if !response.status().is_success() {
            return Err(crate::locale_tag::with_params(
                "community.searchHttpStatus",
                &[("status", &response.status().to_string())],
            ));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| error.without_url().to_string())?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("community.searchResponseTooLarge".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "community.searchResponseInvalid")?;
        let items = value
            .as_array()
            .filter(|items| items.len() <= 50)
            .ok_or("community.searchResponseInvalid")?;
        result.discovery.total_count += items.len() as u64;
        for item in items {
            if item.get("topics").and_then(Value::as_array).is_none() {
                return Err("community.gitcodeTopicsUnavailable".into());
            }
            // Display names are not repository paths on AtomGit.
            let mut item = item.clone();
            if let Some(path) = item.get("path").cloned() {
                item["name"] = path;
            }
            match classify_provider_item(&item, &Provider::Gitcode) {
                SearchHit::Mod(repository) => {
                    if seen.insert(repository.id.clone()) {
                        result.repositories.push(repository);
                    }
                }
                SearchHit::Map(repository) => {
                    let key =
                        format!("{}/{}", repository.username, repository.repository).to_lowercase();
                    if seen.insert(key) {
                        result.map_repositories.push(repository);
                    }
                }
                SearchHit::Skip => {}
                SearchHit::Invalid => return Err("community.searchResponseInvalid".into()),
            }
        }
        if items.len() < 50 {
            return Ok(result);
        }
    }
    result.discovery.truncated = true;
    Ok(result)
}
