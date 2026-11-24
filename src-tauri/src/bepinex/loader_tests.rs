use super::*;

#[test]
fn fixed_framework_release_uses_the_asset_platform_and_region_plan() {
    use crate::archive::DownloadRoute;
    for package in [Package::Windows, Package::MacOS] {
        let github_url = package.url(&Provider::Github);
        for provider in [Provider::Github, Provider::Gitcode] {
            let mainland = package.plan_download(&provider, "cn").unwrap();
            assert_eq!(mainland.candidates[0].url, github_url);
            assert_eq!(mainland.candidates[0].route, DownloadRoute::Direct);
            assert_eq!(
                mainland.candidates[1].url,
                format!("https://git.yylx.win/{github_url}")
            );
            assert_eq!(mainland.execution.max_parallelism, 3);
            assert_eq!(mainland.download_threads, 1);
            let international = package.plan_download(&provider, "global").unwrap();
            assert_eq!(international.candidates[0].url, github_url);
            assert_eq!(international.candidates[0].route, DownloadRoute::Direct);
            assert_ne!(
                international.candidates[0].identity,
                mainland.candidates[0].identity
            );
            assert_eq!(international.candidates.len(), 3);
            let path = github_url.strip_prefix("https://github.com").unwrap();
            for (candidate, host) in international.candidates[1..]
                .iter()
                .zip(["oc2gh.ch3ng.top", "oc2gh.5882778.xyz"])
            {
                assert_eq!(candidate.route, DownloadRoute::Proxy);
                assert_eq!(candidate.url, format!("https://{host}{path}"));
            }
            assert!(international.execution.race);
            assert_eq!(international.execution.max_parallelism, 3);
        }
        for region in ["cn", "global"] {
            let gitee = package.plan_download(&Provider::Gitee, region).unwrap();
            assert_eq!(gitee.candidates.len(), 1);
            assert_eq!(gitee.candidates[0].url, package.url(&Provider::Gitee));
            assert_eq!(gitee.candidates[0].route, DownloadRoute::Direct);
        }
    }
}

fn winhttp_state(root: &Path) -> WinhttpState {
    crate::bepinex::state(
        root,
        &GameConfig {
            path: String::new(),
            runtime: crate::models::GameRuntime::Wine,
        },
    )
}

#[test]
fn winhttp_state_machine() {
    let root = std::env::temp_dir().join(format!("oc2-winhttp-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Missing);
    fs::write(root.join("winhttp.dll"), b"loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
    fs::rename(root.join("winhttp.dll"), root.join("winhttp.dll.disabled")).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModDisabled);
    fs::rename(root.join("winhttp.dll.disabled"), root.join("winhttp.dll")).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
    assert_eq!(fs::read(root.join("winhttp.dll")).unwrap(), b"loader");
    fs::write(root.join("winhttp.dll.disabled"), b"disabled loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Conflict);
    fs::remove_file(root.join("winhttp.dll")).unwrap();
    fs::remove_file(root.join("winhttp.dll.disabled")).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Missing);
    fs::remove_dir(root).unwrap();
}

#[test]
fn winhttp_state_does_not_recognize_obsolete_marker() {
    let root = std::env::temp_dir().join(format!("oc2-winhttp-marker-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let obsolete = root.join("winhttp.dll.oc2mod.disabled");
    fs::write(&obsolete, b"obsolete loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Missing);
    fs::write(root.join("winhttp.dll"), b"loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
    fs::remove_file(root.join("winhttp.dll")).unwrap();
    fs::remove_file(obsolete).unwrap();
    fs::remove_dir(root).unwrap();
}
