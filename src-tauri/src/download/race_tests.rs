use super::planning::plan_repository_download;
use super::{
    DownloadCandidate as AssetDownloadCandidate,
    try_download_candidates as try_asset_download_candidates,
};
use crate::archive::{self, DownloadRoute as AssetDownloadRoute};
use crate::models::{NetworkConfig, NetworkPlanSelection, Provider, RepositoryKind};
use std::path::PathBuf;

const DIRECT: &str = "https://github.com/example/mod/releases/download/v1/mod.zip";

fn release_race(limit: usize) -> Vec<AssetDownloadCandidate> {
    let mut candidates = plan_repository_download(
        &Provider::Github,
        RepositoryKind::Mod,
        &NetworkPlanSelection::default(),
        "cn",
        DIRECT,
        None,
        None,
        None,
    )
    .unwrap()
    .candidates;
    candidates.truncate(limit);
    candidates
}

#[tokio::test]
async fn connection_progress_identifies_route_without_leaking_asset_or_secrets() {
    let candidate = AssetDownloadCandidate {
        route: AssetDownloadRoute::Proxy,
        url: "https://private-user:private-pass@proxy.example:8443/https://github.com/owner/repo/asset.zip?token=private-token#secret".into(),
        identity: None,
    };
    let mut progress = None;
    let result = super::try_download_candidates_with_progress(
        vec![
            candidate,
            AssetDownloadCandidate {
                route: AssetDownloadRoute::Direct,
                url: DIRECT.into(),
                identity: None,
            },
        ],
        || false,
        |_, _| std::future::ready(Ok(vec![42])),
        &NetworkConfig::default(),
        Some(super::DownloadContext {
            execution: None,
            label: "示例 MOD",
            file_name: None,
            expected_size: None,
            race_session: None,
        }),
        |event| {
            progress.get_or_insert(event);
        },
    )
    .await;
    assert_eq!(result.unwrap(), vec![42]);
    let progress = progress.unwrap();
    let event = serde_json::to_value(&progress).unwrap();
    assert_eq!(event["phase"], "connecting");
    assert_eq!(event["route"], "proxy");
    assert_eq!(event["source"], "proxy.example:8443");
    let race = progress.race.as_ref().unwrap();
    assert_eq!(race.primary_id, Some(1));
    assert_eq!(race.sources.len(), 2);
    assert_eq!(race.sources[1].source.as_deref(), Some("github.com"));
    assert_eq!(progress.downloaded, 0);
    assert_eq!(progress.total, None);
    assert!(!progress.finished);
    for secret in ["private-user", "private-pass", "private-token", "asset.zip"] {
        assert!(!event.to_string().contains(secret));
    }
}

#[tokio::test]
async fn download_candidate_failure_logs_partial_bytes_and_hides_query_and_url() {
    let candidate = AssetDownloadCandidate {
        route: AssetDownloadRoute::Proxy,
        url: "https://private-download-fixture.example/private-file?token=fixture-secret".into(),
        identity: None,
    };
    let capture = crate::http_diagnostics::capture_test_logs(
        "app_lib::download",
        "下载候选失败，补充其他候选：任务=fixture-download-log",
    );
    let result: Result<Vec<u8>, String> = try_asset_download_candidates(
        vec![candidate],
        || false,
        |_, reporter| {
            reporter.received(13);
            std::future::ready(Err("download.httpStatus?status=403".into()))
        },
        &NetworkConfig::default(),
        Some(super::DownloadContext {
            execution: None,
            label: "fixture-download-log",
            file_name: None,
            expected_size: Some(20),
            race_session: None,
        }),
    )
    .await;
    assert_eq!(result.unwrap_err(), "download.httpStatus?status=403");
    let messages = capture.messages();
    assert_eq!(messages.len(), 1, "{messages:?}");
    for field in [
        "private-download-fixture.example",
        "已收字节=13",
        "预期字节=Some(20)",
        "原因=download.httpStatus",
    ] {
        assert!(messages[0].contains(field), "{messages:?}");
    }
    assert!(!messages[0].contains("private-file"));
    assert!(!messages[0].contains("fixture-secret"));
}

#[tokio::test]
async fn every_fallback_candidate_can_announce_context_before_waiting() {
    let candidates = release_race(3);
    let mut events = Vec::new();
    let result: Result<PathBuf, String> = super::try_download_candidates_with_progress(
        candidates,
        || false,
        |candidate, _reporter| {
            let route = candidate.route;
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
        &NetworkConfig::default(),
        Some(super::DownloadContext {
            execution: None,
            label: "示例 MOD",
            file_name: None,
            expected_size: None,
            race_session: None,
        }),
        |event| events.push(event),
    )
    .await;
    assert!(result.is_ok());
    let rows = &events[0].race.as_ref().unwrap().sources;
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|row| row.phase == archive::DownloadSourcePhase::Connecting)
    );
    assert_eq!(rows[0].route, archive::DownloadRoute::Direct);
    assert_eq!(
        rows.iter()
            .map(|event| event.source.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("github.com"),
            Some("git.yylx.win"),
            Some("cdn.gh-proxy.org")
        ]
    );
}

#[tokio::test]
async fn pool_stops_on_success_and_cancel() {
    let candidates = release_race(2);
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
        &NetworkConfig::default(),
        None,
    )
    .await;
    assert_eq!(result.unwrap(), PathBuf::from("downloaded"));
    assert_eq!(
        tried,
        [AssetDownloadRoute::Direct, AssetDownloadRoute::Proxy]
    );

    let candidates = release_race(2);
    let mut attempted = 0;
    let result: Result<PathBuf, String> = try_asset_download_candidates(
        candidates,
        || false,
        |_candidate, _reporter| {
            attempted += 1;
            std::future::ready(Err("download.cancelled".into()))
        },
        &NetworkConfig::default(),
        None,
    )
    .await;
    assert_eq!(result.unwrap_err(), "download.cancelled");
    // Both initial slots may start before an asynchronous cancellation
    // result is received. No later candidate may start after cancellation.
    assert_eq!(attempted, 2);

    let candidates = release_race(2);
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
        &NetworkConfig::default(),
        None,
    )
    .await;
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(attempted, 1);
}
