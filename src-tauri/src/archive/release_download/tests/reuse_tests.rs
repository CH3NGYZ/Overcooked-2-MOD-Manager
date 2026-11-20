use super::*;
use crate::archive::release_download::reuse::RELEASE_ASSETS_CACHE;
use crate::models::{MapRepositoryConfig, Provider};

fn zip(payload: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("sample", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap().into_inner()
}

pub(crate) fn served(server: &Server, index: usize, name: &str, bytes: &[u8]) -> ReleaseAsset {
    ReleaseAsset {
        name: name.into(),
        size: bytes.len() as u64,
        digest: Some(format!("sha256:{:x}", Sha256::digest(bytes))),
        download_count: None,
        download_url: format!("{}/{index}", server.url),
    }
}

pub(crate) fn direct(url: &str) -> Result<RepositoryDownloadPlan, String> {
    Ok(RepositoryDownloadPlan {
        candidates: vec![DownloadCandidate {
            route: DownloadRoute::Direct,
            url: url.into(),
        }],
        download_threads: 1,
        execution: DownloadExecution {
            max_parallelism: 1,
            race: false,
        },
    })
}

fn mod_scope(version: &str) -> ReleaseCacheScope {
    ReleaseCacheScope::for_mod(Provider::Gitcode, "Owner", "Repo", version)
}

fn map(tag: &str) -> MapRepositoryConfig {
    serde_json::from_value(serde_json::json!({
        "provider": "gitcode",
        "username": "Owner",
        "repository": "Repo",
        "tag": tag,
        "release_derived": true,
        "author": "Fixture",
        "level_set_name": "Map",
        "asset_rules": [{"type":"regex", "value":"^[A-Z]+_v([0-9.]+)\\.zip$"}]
    }))
    .unwrap()
}

async fn fetch(
    dir: &Path,
    scope: ReleaseCacheScope,
    assets: Vec<ReleaseAsset>,
    plan: impl Fn(&str) -> Result<RepositoryDownloadPlan, String>,
) -> PathBuf {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    download(
        &client,
        assets,
        dir,
        Some(&scope),
        "cached fixture",
        None,
        &NetworkConfig::default(),
        plan,
    )
    .await
    .unwrap()
    .into_path()
}

fn expect_staged_bytes(dir: &Path, path: &Path, bytes: &[u8]) {
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(
        crate::level_sets::staging::cache_file_to_discard(dir, path.to_str().unwrap()).unwrap(),
        Some(path.canonicalize().unwrap())
    );
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn matching_api_digest_reuses_zip_without_planning_and_changes_require_network() {
    let dir = TempDir::new();
    let old = zip(b"first release content");
    let new = zip(b"newer release bytes!!");
    let server = Server::new(vec![old.clone(), new.clone()], false);
    let mut asset = served(&server, 0, "Fixture.zip", &old);

    let fresh = fetch(&dir.0, mod_scope("v1.0"), vec![asset.clone()], direct).await;
    expect_staged_bytes(&dir.0, &fresh, &old);
    let cached = fetch(&dir.0, mod_scope("v1.0"), vec![asset], |_| {
        panic!("a cache hit must not plan routes")
    })
    .await;
    assert_ne!(cached, fresh);
    expect_staged_bytes(&dir.0, &cached, &old);
    assert_eq!(server.calls.load(Ordering::Relaxed), 1);

    // A publisher replaces an attachment under the same tag and filename.
    asset = served(&server, 1, "Fixture.zip", &new);
    let replacement = fetch(&dir.0, mod_scope("v1.0"), vec![asset.clone()], direct).await;
    expect_staged_bytes(&dir.0, &replacement, &new);
    assert_eq!(server.calls.load(Ordering::Relaxed), 2);

    // Existing cache entries cannot authenticate current metadata without a hash.
    asset.digest = None;
    for _ in 0..2 {
        let no_digest = fetch(&dir.0, mod_scope("v1.0"), vec![asset.clone()], direct).await;
        expect_staged_bytes(&dir.0, &no_digest, &new);
    }
    assert_eq!(server.calls.load(Ordering::Relaxed), 4);
}

#[tokio::test]
async fn split_zip_requires_hashes_for_every_part_and_reuses_the_complete_set() {
    let dir = TempDir::new();
    let (parts, _) = split_fixture();
    let server = Server::new(parts.clone(), false);
    let assets: Vec<_> = ["MAP_v1.z01", "MAP_v1.z02", "MAP_v1.zip"]
        .into_iter()
        .zip(&parts)
        .enumerate()
        .map(|(i, (name, bytes))| served(&server, i, name, bytes))
        .collect();

    let mut missing_part_digest = assets.clone();
    missing_part_digest[1].digest = None;
    let scope = || ReleaseCacheScope::for_map(&map("maps"), "1", &assets[2]).unwrap();
    let unauthenticated = fetch(&dir.0, scope(), missing_part_digest, direct).await;
    assert!(!dir.0.join(RELEASE_ASSETS_CACHE).exists());
    let fresh = fetch(&dir.0, scope(), assets.clone(), direct).await;
    let bytes = std::fs::read(&unauthenticated).unwrap();
    std::fs::remove_file(unauthenticated).unwrap();
    expect_staged_bytes(&dir.0, &fresh, &bytes);
    let cached = fetch(&dir.0, scope(), assets, |_| {
        panic!("all the local volumes should have validated")
    })
    .await;
    expect_staged_bytes(&dir.0, &cached, &bytes);
    assert_eq!(server.calls.load(Ordering::Relaxed), 6);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[tokio::test]
async fn map_split_versions_share_three_slots_by_tag_and_main_zip_prefix() {
    let dir = TempDir::new();
    let versions = [
        ("maps", "RED", "1.0"),
        ("maps", "RED", "1.1"),
        ("maps", "BLUE", "1.0"),
        ("other-tag", "RED", "1.0"),
        ("maps", "RED", "1.2"),
        ("maps", "RED", "1.3"),
    ];
    let (parts, _) = split_fixture();
    let server = Server::new(parts.clone(), false);
    let fixture = |(tag, prefix, version): (&str, &str, &str)| {
        let assets: Vec<_> = ["z01", "z02", "zip"]
            .into_iter()
            .zip(&parts)
            .enumerate()
            .map(|(i, (suffix, bytes))| {
                served(&server, i, &format!("{prefix}_v{version}.{suffix}"), bytes)
            })
            .collect();
        let main = assets.last().unwrap();
        let scope = ReleaseCacheScope::for_map(&map(tag), version, main).unwrap();
        (scope, asset_set(main, &assets).unwrap())
    };
    let check = |path: &Path| {
        let archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        drop(archive);
        std::fs::remove_file(path).unwrap();
    };

    for version in versions {
        let (scope, assets) = fixture(version);
        check(&fetch(&dir.0, scope, assets, direct).await);
    }
    // RED has three full sets in `maps`. BLUE and the other tag also stay
    // cached, even though their version strings and contents coincide.
    for version in &versions[1..] {
        let (scope, assets) = fixture(*version);
        let cached = fetch(&dir.0, scope, assets, |_| {
            panic!("the three retained versions and independent prefixes/tags must hit")
        })
        .await;
        check(&cached);
    }
    assert_eq!(server.request_count(), 18);
    let (scope, assets) = fixture(versions[0]);
    check(&fetch(&dir.0, scope, assets, direct).await);
    assert_eq!(server.request_count(), 21);
}

#[tokio::test]
async fn verified_release_dll_uses_mod_cache_without_changing_staging() {
    let dir = TempDir::new();
    let bytes = include_bytes!(
        "../../../mod_changes/orchestration_tests/duplicate_release_target/fastinit_version.pe"
    );
    let server = Server::new(vec![bytes.to_vec()], false);
    let mut asset = served(&server, 0, "FastInit.DLL", bytes);
    let downloaded = fetch(&dir.0, mod_scope("v1.9.6"), vec![asset.clone()], direct).await;
    expect_staged_bytes(&dir.0, &downloaded, bytes);
    let cached = fetch(&dir.0, mod_scope("v1.9.6"), vec![asset.clone()], |_| {
        panic!("a verified DLL cache hit must not plan routes")
    })
    .await;
    expect_staged_bytes(&dir.0, &cached, bytes);
    assert_eq!(server.request_count(), 1);

    asset.digest = None;
    let no_digest = fetch(&dir.0, mod_scope("v1.9.6"), vec![asset], direct).await;
    expect_staged_bytes(&dir.0, &no_digest, bytes);
    assert_eq!(server.request_count(), 2);
}
