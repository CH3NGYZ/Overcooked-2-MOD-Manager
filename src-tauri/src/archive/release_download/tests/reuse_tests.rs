use super::*;
use crate::archive::release_download::reuse::RELEASE_ASSETS_CACHE;
use crate::models::Provider;

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

async fn fetch(
    dir: &Path,
    map_tag: Option<&str>,
    version: &str,
    assets: Vec<ReleaseAsset>,
    plan: impl Fn(&str) -> Result<RepositoryDownloadPlan, String>,
) -> PathBuf {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    download(
        &client,
        assets,
        dir,
        Some(&ReleaseCacheScope::new(
            Provider::Gitcode,
            "Owner",
            "Repo",
            map_tag,
            version,
        )),
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

    let fresh = fetch(&dir.0, None, "v1.0", vec![asset.clone()], direct).await;
    expect_staged_bytes(&dir.0, &fresh, &old);
    let cached = fetch(&dir.0, None, "v1.0", vec![asset], |_| {
        panic!("a cache hit must not plan routes")
    })
    .await;
    assert_ne!(cached, fresh);
    expect_staged_bytes(&dir.0, &cached, &old);
    assert_eq!(server.calls.load(Ordering::Relaxed), 1);

    // A publisher replaces an attachment under the same tag and filename.
    asset = served(&server, 1, "Fixture.zip", &new);
    let replacement = fetch(&dir.0, None, "v1.0", vec![asset.clone()], direct).await;
    expect_staged_bytes(&dir.0, &replacement, &new);
    assert_eq!(server.calls.load(Ordering::Relaxed), 2);

    // Existing cache entries cannot authenticate current metadata without a hash.
    asset.digest = None;
    for _ in 0..2 {
        let no_digest = fetch(&dir.0, None, "v1.0", vec![asset.clone()], direct).await;
        expect_staged_bytes(&dir.0, &no_digest, &new);
    }
    assert_eq!(server.calls.load(Ordering::Relaxed), 4);
}

#[tokio::test]
async fn split_zip_requires_hashes_for_every_part_and_reuses_the_complete_set() {
    let dir = TempDir::new();
    let (parts, _) = split_fixture();
    let server = Server::new(parts.clone(), false);
    let assets: Vec<_> = ["Fixture.z01", "Fixture.z02", "Fixture.zip"]
        .into_iter()
        .zip(&parts)
        .enumerate()
        .map(|(i, (name, bytes))| served(&server, i, name, bytes))
        .collect();

    let mut missing_part_digest = assets.clone();
    missing_part_digest[1].digest = None;
    let unauthenticated = fetch(&dir.0, Some("map"), "v1", missing_part_digest, direct).await;
    assert!(!dir.0.join(RELEASE_ASSETS_CACHE).exists());
    let fresh = fetch(&dir.0, Some("map"), "v1", assets.clone(), direct).await;
    let bytes = std::fs::read(&unauthenticated).unwrap();
    std::fs::remove_file(unauthenticated).unwrap();
    expect_staged_bytes(&dir.0, &fresh, &bytes);
    let cached = fetch(&dir.0, Some("map"), "v1", assets, |_| {
        panic!("all the local volumes should have validated")
    })
    .await;
    expect_staged_bytes(&dir.0, &cached, &bytes);
    assert_eq!(server.calls.load(Ordering::Relaxed), 6);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[tokio::test]
async fn release_tags_maps_each_keep_two_disjoint_versions() {
    let dir = TempDir::new();
    let parts: Vec<_> = (0..5).map(|index| zip(&[index])).collect();
    let server = Server::new(parts.clone(), false);
    let versions = [
        ("map-A", "1.0"),
        ("map-A", "1.1"),
        ("map-B", "1.0"),
        ("map-B", "1.1"),
        ("map-A", "1.2"),
    ];
    let assets: Vec<_> = parts
        .iter()
        .enumerate()
        .map(|(index, bytes)| served(&server, index, "Fixture.zip", bytes))
        .collect();

    for (i, &(tag, version)) in versions.iter().enumerate() {
        let fresh = fetch(&dir.0, Some(tag), version, vec![assets[i].clone()], direct).await;
        expect_staged_bytes(&dir.0, &fresh, &parts[i]);
    }
    // This corresponds to the same repository, but two independent upstream
    // map tags. The overlap in their version strings is intentional.
    for i in 1..versions.len() {
        let (tag, version) = versions[i];
        let cached = fetch(&dir.0, Some(tag), version, vec![assets[i].clone()], |_| {
            panic!("one map's third version must not evict another map's history")
        })
        .await;
        expect_staged_bytes(&dir.0, &cached, &parts[i]);
    }
    assert_eq!(server.calls.load(Ordering::Relaxed), 5);
    let evicted = fetch(
        &dir.0,
        Some("map-A"),
        "1.0",
        vec![assets[0].clone()],
        direct,
    )
    .await;
    expect_staged_bytes(&dir.0, &evicted, &parts[0]);
    assert_eq!(server.calls.load(Ordering::Relaxed), 6);
}
