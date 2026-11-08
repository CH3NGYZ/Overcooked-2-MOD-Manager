use super::github::SEARCH_ENDPOINT;
use super::github::search_all;
use super::merge::merge_results;
use crate::app_state::AppState;
use crate::models::Provider;
use crate::models::PublicConfig;
use crate::releases;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::AppHandle;
use tauri::State;

#[tauri::command]
pub async fn refresh_community_repositories(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Option<Provider>,
) -> Result<PublicConfig, String> {
    let provider = provider.unwrap_or(Provider::Github);
    let search = state.community.for_provider(&provider);
    let mut limits = search
        .limits
        .try_lock()
        .map_err(|_| "community.searchInProgress")?;
    search.cancelled.store(false, Ordering::Release);
    let config = {
        let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
        state.config.load_or_default()
    };
    let token = state.config.api_token(&config, &provider);
    let results = tokio::select! {
        results = async {
            match provider {
                Provider::Github => {
                    let client = releases::build_client(config.network.timeout_seconds).map_err(|error| error.to_string())?;
                    search_all(&client, SEARCH_ENDPOINT, token.as_deref(), &config.github_proxies.github_api_proxies, &mut limits).await
                },
                Provider::Gitee => crate::gitee_community::search_all(&app, state.config.cache_dir().join("gitee-webview"), &mut limits).await,
                Provider::Gitcode => {
                    let client = releases::build_client(config.network.timeout_seconds).map_err(|error| error.to_string())?;
                    super::gitcode::discover(&client, token.as_deref(), &mut limits).await
                },
            }
        } => results?,
        _ = async { while !search.cancelled.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(100)).await; } } => return Err("community.searchCancelled".into()),
    };
    let _cache_guard = state.repository_cache.lock().await;
    let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
    if search.cancelled.load(Ordering::Acquire) {
        return Err("community.searchCancelled".into());
    }
    let mut config = state.config.load_or_default();
    let manifests = crate::archive::read_manifests(&state.config.manifests_dir());
    merge_results(&mut config, &provider, results, &manifests);
    state
        .config
        .save(&config)
        .map_err(|error| error.to_string())?;
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn cancel_community_search(state: State<'_, AppState>, provider: Option<Provider>) {
    state
        .community
        .for_provider(&provider.unwrap_or(Provider::Github))
        .cancelled
        .store(true, Ordering::Release);
}
