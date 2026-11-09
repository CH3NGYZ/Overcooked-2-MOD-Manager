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
            assert_eq!(
                info.manifest_url,
                installer_manifest_url(&manifest_provider)
            );
            let candidates =
                installer_download_candidates(&crate::github_proxies::bundled_defaults(), &info);
            if package_provider != Provider::Github {
                assert_eq!(candidates.len(), 1);
            }
            assert_eq!(candidates[0].route, AssetDownloadRoute::Direct);
            assert_eq!(
                Some(candidates[0].url.as_str()),
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
        let mut channels = installer_manifest_sources(
            &GitHubProxyConfig::default(),
            InstallerRegion::MainlandChina,
        );
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
