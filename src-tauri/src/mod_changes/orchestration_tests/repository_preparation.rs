use super::super::progress::{ItemStage, PreparationOutcome, PreparationStage};
use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served, split_zip_parts};
use std::io::{Cursor, Write};
use std::sync::{Arc, Mutex};

#[path = "repository_preparation/missing_dll.rs"]
mod missing_dll;

const TAG: &str = "v2.0.0";
const REPO: &str = "FixtureMod";
const GUID: &str = "com.example.preparation";
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
async fn hashless_release_reports_occupied_targets_without_touching_the_game() {
    let dll = mod_dll();
    let asset = "Selected.zip";
    let entry = format!("{TAG}/{DLL}");
    let bytes = zip(&[(&entry, &dll)]);
    let target = format!("BepInEx/plugins/{REPO}/{entry}");
    for (obstacle, tag) in [
        ("directory", "modChanges.installTargetIsDirectory"),
        ("both states", "modChanges.installTargetHasBothStates"),
        ("old record", "modChanges.installTargetClaimedWithManifests"),
    ] {
        let server = Server::new(vec![bytes.clone()], false);
        let mut repo = repository(REPO, GUID, &["2.0.0"], vec![]);
        let mut release_asset = served(&server, 0, asset, &bytes);
        release_asset.digest = None;
        repo.cached_releases[0].assets = vec![release_asset];
        let f = versioned_fixture(repo);
        let manifests = f.state.config.manifests_dir();
        match obstacle {
            "directory" => fs::create_dir_all(f.game.join(&target)).unwrap(),
            "both states" => {
                write(&f.game.join(&target), b"unrecognized enabled DLL");
                write(
                    &f.game.join(mods::disabled_dll_file(&target)),
                    b"unrecognized disabled DLL",
                );
            }
            "old record" => archive::write_manifest(
                &manifests,
                &archive::new_manifest(
                    "AnotherRepository",
                    TAG,
                    BTreeMap::new(),
                    BTreeMap::new(),
                    vec![target.clone()],
                    vec![],
                ),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let before = (
            Fingerprint::capture(&f.game).unwrap(),
            Fingerprint::capture(&manifests).unwrap(),
        );
        let plan = select_plan(&f, REPO, TAG, asset);
        let stages = Arc::new(Mutex::new(Vec::new()));
        let observed = stages.clone();
        let outcome = preparation::prepare(f.state.clone(), plan.plan_id, move |progress| {
            observed.lock().unwrap().push(progress.stage);
        })
        .await;
        let stages = stages.lock().unwrap();
        assert_eq!(
            outcome.progress.stage,
            PreparationStage::Failed,
            "{obstacle}: {stages:?}"
        );
        assert!(
            !stages.contains(&PreparationStage::Finalizing),
            "{obstacle}: {stages:?}"
        );
        assert!(outcome.plan.is_none());
        assert_eq!(server.request_count(), 1);
        assert_eq!(
            (
                Fingerprint::capture(&f.game).unwrap(),
                Fingerprint::capture(&manifests).unwrap(),
            ),
            before,
        );
        let mut params = vec![("path", target.as_str())];
        let both_paths = format!("{target}\n{}", mods::disabled_dll_file(&target));
        if obstacle == "old record" {
            params.push(("manifests", "manifests/AnotherRepository.toml"));
        } else if obstacle == "both states" {
            params = vec![("paths", both_paths.as_str())];
        }
        assert_eq!(
            outcome.error,
            Some(crate::locale_tag::with_params(tag, &params)),
            "{obstacle}"
        );
    }
}

#[tokio::test]
async fn clean_cycle_downloads_dependent_before_framework_then_commits_both() {
    const FRAMEWORK: &str = "FixtureFramework";
    const FRAMEWORK_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
    const FRAMEWORK_DLL: &str = "Framework.dll";
    let dll = mod_dll();
    let mut framework_dll = PE.to_vec();
    framework_dll.extend(mods::plugin_blob(FRAMEWORK_GUID, FRAMEWORK, "2.0.0"));
    let assets = [
        ("Selected.zip", zip(&[(DLL, &dll)])),
        ("Framework.zip", zip(&[(FRAMEWORK_DLL, &framework_dll)])),
    ];
    let server = Server::new(
        assets.iter().map(|(_, bytes)| bytes.clone()).collect(),
        false,
    );
    let mut repos = vec![
        repository(
            REPO,
            GUID,
            &["2.0.0"],
            vec![requires(FRAMEWORK_GUID, ">=2.0.0")],
        ),
        repository(
            FRAMEWORK,
            FRAMEWORK_GUID,
            &["2.0.0"],
            vec![requires(GUID, ">=2.0.0")],
        ),
    ];
    for (index, (repo, (name, bytes))) in repos.iter_mut().zip(&assets).enumerate() {
        repo.asset_rules[0].value = (*name).into();
        repo.cached_releases[0].assets = vec![served(&server, index, name, bytes)];
    }
    let f = fixture(repos);
    write(&f.game.join("BepInEx/core/BepInEx.dll"), PE);
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = select_plan(&f, REPO, TAG, assets[0].0);
    assert_eq!(plan.result.actions.len(), 2);
    let nodes = [format!("mod:{REPO}"), format!("mod:{FRAMEWORK}")];
    let resolve_passes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let trace = resolve_passes.clone();
    let outcome = preparation::prepare(f.state.clone(), plan.plan_id, move |progress| {
        if progress.stage == PreparationStage::Resolving {
            trace.lock().unwrap().push(
                progress
                    .items
                    .into_iter()
                    .map(|item| (item.node_id, item.stage))
                    .collect::<Vec<_>>(),
            );
        }
    })
    .await;
    assert_private_staging(&f, &before);
    assert_eq!(
        outcome.error,
        None,
        "{} asset requests",
        server.request_count()
    );
    assert_eq!(outcome.progress.stage, PreparationStage::Ready);
    assert_eq!(server.request_count(), assets.len());
    let trace = resolve_passes.lock().unwrap();
    assert!(trace.contains(&vec![
        (nodes[0].clone(), ItemStage::Ready),
        (nodes[1].clone(), ItemStage::Queued)
    ]));
    assert!(trace.contains(&vec![
        (nodes[0].clone(), ItemStage::Ready),
        (nodes[1].clone(), ItemStage::Ready)
    ]));
    let ready = outcome.plan.unwrap();
    assert!(ready.prepared);
    assert_eq!(ready.result.status, SolveStatus::Ready);
    assert_eq!(ready.result.actions.len(), 2);
    let mut session = take(&f, &ready);
    assert_eq!(session.payloads.len(), 2);
    assert!(session.mutations.is_some());
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    let manifests = archive::read_manifests(&f.state.config.manifests_dir());
    let expected = [
        (REPO, format!("BepInEx/plugins/{REPO}/{DLL}"), dll),
        (
            FRAMEWORK,
            format!("BepInEx/plugins/{FRAMEWORK}/{FRAMEWORK_DLL}"),
            framework_dll,
        ),
    ];
    assert_eq!(manifests.len(), expected.len());
    for (repo, path, bytes) in expected {
        assert_eq!(fs::read(f.game.join(&path)).unwrap(), bytes);
        let manifest = manifests
            .iter()
            .find(|manifest| manifest.repository_id == repo)
            .unwrap();
        assert_eq!(manifest.release_tag, TAG);
        assert_eq!(manifest.repository_binding.as_ref().unwrap().tag, TAG);
        assert_eq!(manifest.files, [path]);
    }
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
    let resource = "payload-data.bin";
    let parts = split_zip_parts(&[(DLL, &dll), (resource, &padding)]);
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
                    .join(format!("BepInEx/plugins/{REPO}/Split/{resource}"))
            )
            .unwrap(),
            padding
        );
        super::super::execution::cleanup(&f.state, &session.staging).unwrap();
    }
}
