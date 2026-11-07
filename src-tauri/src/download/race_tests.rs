use super::planning::plan_asset_download_candidates;
use super::{
    DownloadCandidate as AssetDownloadCandidate, candidate_progress as asset_candidate_progress,
    try_download_candidates as try_asset_download_candidates,
};
use crate::archive::{self, DownloadRoute as AssetDownloadRoute};
use crate::models::ProxyMode;
use std::path::PathBuf;

const DIRECT: &str = "https://github.com/example/mod/releases/download/v1/mod.zip";

#[test]
fn connection_progress_identifies_route_without_leaking_asset_or_secrets() {
    let candidate = AssetDownloadCandidate {
        route: AssetDownloadRoute::Proxy,
        url: "https://private-user:private-pass@proxy.example:8443/https://github.com/owner/repo/asset.zip?token=private-token#secret".into(),
    };
    let progress = asset_candidate_progress("示例 MOD", &candidate, 1, 3);
    let event = serde_json::to_value(&progress).unwrap();
    assert_eq!(event["phase"], "connecting");
    assert_eq!(event["route"], "proxy");
    assert_eq!(event["source"], "proxy.example:8443");
    assert_eq!(event["attempt"], 1);
    assert_eq!(event["attempt_total"], 3);
    assert_eq!(progress.downloaded, 0);
    assert_eq!(progress.total, None);
    assert!(!progress.finished);
    for secret in ["private-user", "private-pass", "private-token", "asset.zip"] {
        assert!(!event.to_string().contains(secret));
    }

    let fallback = asset_candidate_progress(
        "示例 MOD",
        &AssetDownloadCandidate {
            route: AssetDownloadRoute::Direct,
            url: DIRECT.into(),
        },
        3,
        3,
    );
    assert_eq!(fallback.phase, archive::DownloadPhase::Switching);
    assert_eq!(fallback.route, Some(archive::DownloadRoute::Direct));
    assert_eq!(fallback.source.as_deref(), Some("github.com"));
    assert_eq!(fallback.downloaded, 0);
}

#[tokio::test]
async fn every_fallback_candidate_can_announce_context_before_waiting() {
    let candidates = plan_asset_download_candidates(
        ProxyMode::AutoProxy,
        DIRECT,
        vec![
            "https://first.example/asset".into(),
            "https://second.example/asset".into(),
        ],
    )
    .unwrap();
    let total = candidates.len();
    let mut events = Vec::new();
    let result: Result<PathBuf, String> = try_asset_download_candidates(
        candidates,
        || false,
        |candidate, _reporter| {
            let progress =
                asset_candidate_progress("示例 MOD", &candidate, events.len() + 1, total);
            let route = candidate.route;
            events.push(progress);
            async move {
                // Simulate a request which has yielded without response
                // bytes. The endpoint announcement already exists.
                tokio::task::yield_now().await;
                if route == AssetDownloadRoute::Proxy {
                    Err("连接失败".into())
                } else {
                    Ok(PathBuf::from("downloaded"))
                }
            }
        },
        None,
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].phase, archive::DownloadPhase::Connecting);
    assert_eq!(events[1].phase, archive::DownloadPhase::Switching);
    assert_eq!(events[0].route, Some(archive::DownloadRoute::Direct));
    assert_eq!(
        events
            .iter()
            .map(|event| event.source.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("github.com"),
            Some("first.example"),
            Some("second.example")
        ]
    );
}

#[tokio::test]
async fn pool_stops_on_success_and_cancel() {
    let candidates = plan_asset_download_candidates(
        ProxyMode::AutoProxy,
        DIRECT,
        vec!["https://proxy.example/asset".into()],
    )
    .unwrap();
    let mut tried = Vec::new();
    let result: Result<PathBuf, String> = try_asset_download_candidates(
        candidates,
        || false,
        |candidate, _reporter| {
            tried.push(candidate.route);
            std::future::ready(if candidate.route == AssetDownloadRoute::Proxy {
                Err("代理失败".into())
            } else {
                Ok(PathBuf::from("downloaded"))
            })
        },
        None,
    )
    .await;
    assert_eq!(result.unwrap(), PathBuf::from("downloaded"));
    assert_eq!(
        tried,
        [AssetDownloadRoute::Direct, AssetDownloadRoute::Proxy]
    );

    let candidates = plan_asset_download_candidates(
        ProxyMode::AutoDirect,
        DIRECT,
        vec!["https://proxy.example/asset".into()],
    )
    .unwrap();
    let mut attempted = 0;
    let result: Result<PathBuf, String> = try_asset_download_candidates(
        candidates,
        || false,
        |_candidate, _reporter| {
            attempted += 1;
            std::future::ready(Err("download.cancelled".into()))
        },
        None,
    )
    .await;
    assert_eq!(result.unwrap_err(), "download.cancelled");
    // Both initial slots may start before an asynchronous cancellation
    // result is received. No later candidate may start after cancellation.
    assert_eq!(attempted, 2);

    let candidates = plan_asset_download_candidates(
        ProxyMode::AutoProxy,
        DIRECT,
        vec!["https://proxy.example/asset".into()],
    )
    .unwrap();
    let cancellation = std::cell::Cell::new(false);
    let mut attempted = 0;
    let result: Result<PathBuf, String> = try_asset_download_candidates(
        candidates,
        || cancellation.get(),
        |_candidate, _reporter| {
            attempted += 1;
            cancellation.set(true);
            std::future::ready(Err("网络失败".into()))
        },
        None,
    )
    .await;
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(attempted, 1);
}
