use super::*;

#[test]
fn official_mirrors_can_share_a_manifest_and_plan_by_package_provider() {
    for manifest_provider in [Provider::Gitee, Provider::Github, Provider::Gitcode] {
        for package_provider in [Provider::Gitee, Provider::Github, Provider::Gitcode] {
            let mut document = fixture(package_provider.clone());
            if package_provider == Provider::Gitcode {
                // The supplied GitCode manifest and package use different official repositories.
                document["platforms"]["windows-x86_64-nsis"]["url"] =
                    "https://gitcode.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe".into();
            }
            let info = inspect_installer_manifest(
                &direct(manifest_provider.clone()),
                &document,
                target(),
                &semver::Version::new(1, 0, 0),
            )
            .unwrap()
            .unwrap();
            assert_eq!(info.provider, Some(package_provider.clone()));
            let candidates = package_channels(&info, InstallerRegion::MainlandChina);
            if package_provider != Provider::Github {
                assert_eq!(candidates.len(), 1);
            }
            assert_eq!(candidates[0].route, AssetDownloadRoute::Direct);
            assert_eq!(
                Some(candidates[0].endpoint.as_str()),
                info.installer_url.as_deref()
            );
        }
    }
}

#[tokio::test]
async fn anonymous_latest_json_follows_mainland_order_and_closes_unused_statuses() {
    for winner in [Provider::Gitee, Provider::Gitcode, Provider::Github] {
        let paths = [
            "/gitee/latest.json",
            "/gitcode/latest.json",
            "/github/latest.json",
        ];
        let providers = [Provider::Gitee, Provider::Gitcode, Provider::Github];
        let winner_index = providers
            .iter()
            .position(|provider| provider == &winner)
            .unwrap();
        let server = Server::new(
            paths
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    (
                        path.to_string(),
                        if index == winner_index {
                            response(200, serde_json::to_vec(&fixture(winner.clone())).unwrap())
                        } else {
                            response(503, Vec::new())
                        },
                    )
                })
                .collect(),
        );
        let mut channels =
            installer_manifest_sources(&direct_test_policy(), InstallerRegion::MainlandChina);
        assert_eq!(
            channels
                .iter()
                .map(|channel| channel.provider.clone())
                .collect::<Vec<_>>(),
            providers
        );
        for (channel, path) in channels.iter_mut().zip(paths) {
            channel.endpoint = format!("{}{path}", server.base);
        }
        let mut statuses = Vec::new();
        let result = run_installer_manifest_routes(
            &reqwest::Client::new(),
            &channels,
            target(),
            &semver::Version::new(1, 0, 0),
            || false,
            |status| statuses.push(status),
            |info| std::future::ready(Ok(info)),
            &crate::models::NetworkConfig::default(),
        )
        .await
        .unwrap();
        let InstallerRouteResult::Updated(info) = result else {
            panic!("no update: {statuses:?}")
        };
        assert_eq!(info.provider, Some(winner));
        assert_eq!(info.version, "9.0.0");
        assert_eq!(info.body.as_deref(), Some("Release notes"));
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), winner_index + 1);
        for (request, path) in requests.iter().zip(paths) {
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(!request.contains("access_token"));
        }
        for (index, channel) in channels.iter().enumerate() {
            let status = statuses
                .iter()
                .rev()
                .find(|status| status.id == channel.id)
                .unwrap();
            assert_eq!(
                status.status,
                if index < winner_index {
                    "failed"
                } else if index == winner_index {
                    "update_available"
                } else {
                    "skipped"
                }
            );
        }
    }
}

#[tokio::test]
async fn sequential_manifest_fallback_starts_with_a_fresh_timeout() {
    let manifest = serde_json::to_vec(&fixture(Provider::Github)).unwrap();
    let server = Server::new(HashMap::from([
        ("/slow".into(), delayed(200, manifest.clone(), 1000)),
        ("/next".into(), delayed(200, manifest, 100)),
    ]));
    let mut channels = [server.route("/slow", None), server.route("/next", None)];
    for channel in &mut channels {
        channel.execution = DownloadExecution {
            max_parallelism: 1,
            race: false,
        };
    }
    let timeout = Duration::from_millis(500);
    let started = std::time::Instant::now();
    let mut statuses = Vec::new();
    let result = run_installer_manifest_routes(
        &crate::releases::build_client(timeout).unwrap(),
        &channels,
        target(),
        &semver::Version::new(1, 0, 0),
        || false,
        |status| statuses.push(status),
        |info| std::future::ready(Ok(info)),
        &NetworkConfig::default(),
    )
    .await
    .unwrap();
    let InstallerRouteResult::Updated(info) = result else {
        panic!("no update: {statuses:?}");
    };
    assert_eq!(info.source_channel.as_deref(), Some("/next"));
    assert!(started.elapsed() >= timeout);
    assert_eq!(
        statuses
            .iter()
            .map(|status| status.status.as_str())
            .collect::<Vec<_>>(),
        ["checking", "failed", "checking", "update_available"]
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /slow "));
    assert!(requests[1].starts_with("GET /next "));
}

#[tokio::test]
async fn newest_manual_release_wins_over_an_older_signed_package() {
    let key = target().manifest_key();
    let mut without_signature = fixture(Provider::Github);
    without_signature["platforms"][&key]
        .as_object_mut()
        .unwrap()
        .remove("signature");
    let mut without_package = fixture(Provider::Github);
    without_package["platforms"]
        .as_object_mut()
        .unwrap()
        .remove(&key);
    let mut without_url = fixture(Provider::Github);
    without_url["platforms"][&key]
        .as_object_mut()
        .unwrap()
        .remove("url");
    compare_newest_release(without_signature, false).await;
    compare_newest_release(without_package, false).await;
    compare_newest_release(without_url, false).await;
}

#[tokio::test]
async fn discovery_compares_all_providers_and_keeps_the_newest_release() {
    compare_newest_release(fixture(Provider::Github), true).await;
}

async fn compare_newest_release(latest: serde_json::Value, signed: bool) {
    let key = target().manifest_key();
    let mut outdated = fixture(Provider::Gitee);
    outdated["version"] = "8.0.0".into();
    outdated["platforms"][&key]["url"] =
        "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager/releases/download/v8.0.0/OC2ModManager_8.0.0_x64-setup.exe"
            .into();
    outdated["platforms"][&key]["signature"] = "outdated signature".into();
    let server = Server::new(HashMap::from([
        (
            "/gitee/latest.json".into(),
            response(200, serde_json::to_vec(&outdated).unwrap()),
        ),
        (
            "/github/latest.json".into(),
            response(200, serde_json::to_vec(&latest).unwrap()),
        ),
    ]));
    let mut channels =
        installer_manifest_sources(&direct_test_policy(), InstallerRegion::MainlandChina)
            .into_iter()
            .filter(|channel| channel.provider != Provider::Gitcode)
            .collect::<Vec<_>>();
    for channel in &mut channels {
        channel.endpoint = match channel.provider {
            Provider::Gitee => format!("{}/gitee/latest.json", server.base),
            _ => format!("{}/github/latest.json", server.base),
        };
        channel.execution = DownloadExecution {
            max_parallelism: 1,
            race: false,
        };
    }
    let mut statuses = Vec::new();
    let result = compare_all_installer_manifest_routes(
        &reqwest::Client::new(),
        &channels,
        target(),
        &semver::Version::new(1, 0, 0),
        |status| statuses.push(status),
        &NetworkConfig::default(),
    )
    .await
    .unwrap();
    let InstallerRouteResult::Updated(info) = result else {
        panic!("no update: {statuses:?}")
    };
    assert_eq!(info.version, "9.0.0");
    assert_eq!(info.manifest_url, channels[1].endpoint);
    assert_eq!(info.has_signed_package(), signed);
    assert_eq!(
        statuses
            .iter()
            .map(|status| (status.id.as_str(), status.status.as_str()))
            .collect::<Vec<_>>(),
        [
            (channels[0].id.as_str(), "checking"),
            (channels[0].id.as_str(), "update_available"),
            (channels[1].id.as_str(), "checking"),
            (channels[1].id.as_str(), "update_available"),
        ]
    );
}

#[tokio::test]
async fn equal_versions_prefer_the_signed_package_across_providers() {
    let mut manual = fixture(Provider::Gitee);
    manual["platforms"]["windows-x86_64-nsis"]["signature"] = " ".into();
    let signed = fixture(Provider::Github);
    let server = Server::new(HashMap::from([
        (
            "/gitee/latest.json".into(),
            response(200, serde_json::to_vec(&manual).unwrap()),
        ),
        (
            "/github/latest.json".into(),
            response(200, serde_json::to_vec(&signed).unwrap()),
        ),
    ]));
    let mut channels =
        installer_manifest_sources(&direct_test_policy(), InstallerRegion::MainlandChina)
            .into_iter()
            .filter(|channel| channel.provider != Provider::Gitcode)
            .collect::<Vec<_>>();
    for channel in &mut channels {
        channel.endpoint = match channel.provider {
            Provider::Gitee => format!("{}/gitee/latest.json", server.base),
            _ => format!("{}/github/latest.json", server.base),
        };
        channel.execution = DownloadExecution {
            max_parallelism: 1,
            race: false,
        };
    }
    let result = compare_all_installer_manifest_routes(
        &reqwest::Client::new(),
        &channels,
        target(),
        &semver::Version::new(1, 0, 0),
        |_| {},
        &NetworkConfig::default(),
    )
    .await
    .unwrap();
    let InstallerRouteResult::Updated(info) = result else {
        panic!("no update")
    };
    assert!(info.has_signed_package());
    assert_eq!(info.provider, Some(Provider::Github));
}
