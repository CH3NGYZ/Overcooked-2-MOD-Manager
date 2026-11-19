use super::super::progress::{ItemStage, PreparationOutcome, PreparationStage};
use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served, split_zip_parts};
use std::io::{Cursor, Write};

const TAG: &str = "v2.0.0";
const REPO: &str = "FixtureMod";
const GUID: &str = "fixture.preparation";
const DLL: &str = "Fixture.dll";
const PE: &[u8] = include_bytes!("duplicate_release_target/fastinit_version.pe");

fn versioned_fixture(repo: ModRepositoryConfig) -> Fixture {
    let f = fixture(vec![repo]);
    // The real repository downloader checks the loader's PE version.
    write(&f.game.join("BepInEx/core/BepInEx.dll"), PE);
    f
}

fn mod_dll() -> Vec<u8> {
    let mut dll = PE.to_vec();
    dll.extend(mods::plugin_blob(GUID, REPO, "2.0.0"));
    dll
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        out.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn select_plan(f: &Fixture, repo: &str, tag: &str, asset: &str) -> ChangePlan {
    let mut selection = request(repo, Some(tag), true);
    selection.roots[0].asset_name = Some(asset.into());
    let plan = planning::create(&f.state, selection).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(!plan.prepared);
    plan
}

fn prepared_plan(outcome: PreparationOutcome, tag: &str) -> ChangePlan {
    assert_eq!(outcome.error, None);
    assert_eq!(outcome.progress.stage, PreparationStage::Ready);
    let [item] = outcome.progress.items.as_slice() else {
        panic!("expected one prepared item: {:?}", outcome.progress.items);
    };
    assert_eq!(item.stage, ItemStage::Ready);
    assert_eq!(item.tag.as_deref(), Some(tag));
    let plan = outcome.plan.unwrap();
    assert!(plan.prepared);
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    plan
}

fn assert_private_staging(f: &Fixture, before: &Fingerprint) {
    assert_eq!(&Fingerprint::capture(&f.game).unwrap(), before);
    assert!(!f.state.config.manifests_dir().exists());
    assert!(!crate::mod_preview::staging::preview_root(&f.state).exists());
    // Only persistent cache directories may remain; the downloaded temp is owned
    // by preparation and must be gone before the next plan.
    assert!(
        fs::read_dir(f.state.config.cache_dir())
            .unwrap()
            .all(|entry| { entry.unwrap().file_type().unwrap().is_dir() })
    );
}

#[tokio::test]
async fn exact_release_and_asset_prepare_directly_then_reuse_and_commit() {
    let dll = mod_dll();
    let bytes = zip(&[(DLL, &dll), ("readme.txt", b"preserved resource")]);
    let other = zip(&[("different.txt", b"not the selected asset")]);
    let server = Server::new(vec![other.clone(), bytes.clone()], false);
    let mut repo = repository(REPO, GUID, &["3.0.0", "2.0.0"], vec![]);
    repo.asset_rules[0].value = "Default.zip".into();
    let default = served(&server, 0, "Default.zip", &other);
    repo.cached_releases[0].assets = vec![default.clone()];
    // Either dropping the exact tag or falling back to the default name would
    // pick the wrong bytes. The chosen package's name also determines its layout.
    let asset = "Selected.zip";
    repo.cached_releases[1].assets = vec![default, served(&server, 1, asset, &bytes)];
    let f = versioned_fixture(repo);
    let before = Fingerprint::capture(&f.game).unwrap();
    let relative = format!("BepInEx/plugins/{REPO}/Selected/{DLL}");

    for hit in [false, true] {
        let plan = select_plan(&f, REPO, TAG, asset);
        let id = plan.plan_id.clone();
        let ready = prepared_plan(
            preparation::prepare(f.state.clone(), id.clone(), |_| {}).await,
            TAG,
        );
        assert_ne!(
            id, ready.plan_id,
            "actual DLL metadata requires new approval"
        );
        assert_eq!(server.request_count(), 1);
        assert_private_staging(&f, &before);
        let mut session = take(&f, &ready);
        let payload = &session.payloads[&format!("mod:{REPO}")];
        assert_eq!(payload.candidate.version.as_deref(), Some("2.0.0"));
        assert_eq!(payload.candidate.provides[0].guid, GUID);
        assert!(payload.release_files.is_some());
        assert_eq!(fs::read(payload.stage_root.join(&relative)).unwrap(), dll);
        preparation::verify_fingerprints(&session.payload_fingerprints).unwrap();
        assert_eq!(
            SessionLease::take(&f.state, &plan.plan_id).err().as_deref(),
            Some("modChanges.planExpired"),
        );
        if !hit {
            super::super::execution::cleanup(&f.state, &session.staging).unwrap();
            continue;
        }
        super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
        assert_eq!(fs::read(f.game.join(&relative)).unwrap(), dll);
        let manifests = archive::read_manifests(&f.state.config.manifests_dir());
        let [manifest] = manifests.as_slice() else {
            panic!("expected one committed manifest: {manifests:?}");
        };
        assert_eq!(manifest.release_tag, TAG);
        assert_eq!(manifest.repository_binding.as_ref().unwrap().tag, TAG);
    }
}

#[tokio::test]
async fn split_release_proof_reaches_private_staging_on_download_and_cache_hit() {
    let dll = mod_dll();
    let padding = vec![b'x'; 150_000];
    let parts = split_zip_parts(&[(DLL, &dll), ("resource.bin", &padding)]);
    let server = Server::new(parts.clone(), false);
    let mut repo = repository(REPO, GUID, &["2.0.0"], vec![]);
    repo.cached_releases[0].assets = ["Split.z01", "Split.z02", "Split.zip"]
        .into_iter()
        .zip(&parts)
        .enumerate()
        .map(|(i, (name, bytes))| served(&server, i, name, bytes))
        .collect();
    let f = versioned_fixture(repo);
    let before = Fingerprint::capture(&f.game).unwrap();
    for _ in 0..2 {
        let plan = select_plan(&f, REPO, TAG, "Split.zip");
        let ready = prepared_plan(
            preparation::prepare(f.state.clone(), plan.plan_id, |_| {}).await,
            TAG,
        );
        assert_eq!(server.request_count(), parts.len());
        assert_private_staging(&f, &before);
        let session = take(&f, &ready);
        let payload = &session.payloads[&format!("mod:{REPO}")];
        assert_eq!(payload.candidate.version.as_deref(), Some("2.0.0"));
        assert!(
            payload.release_files.is_some(),
            "every volume provides an API digest"
        );
        assert_eq!(
            fs::read(
                payload
                    .stage_root
                    .join(format!("BepInEx/plugins/{REPO}/Split/resource.bin"))
            )
            .unwrap(),
            padding
        );
        super::super::execution::cleanup(&f.state, &session.staging).unwrap();
    }
}
