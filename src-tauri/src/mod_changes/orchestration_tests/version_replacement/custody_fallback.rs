//! Preserve explicit selections while backtracking over an auto-selected release.
use super::*;

const ROOT: &str = "CustodyRoot";
const ROOT_NODE: &str = "mod:CustodyRoot";
const ROOT_GUID: &str = "fixture-custody-root";
const ROOT_ASSET: &str = "CustodyRoot.zip";
const ROOT_DLL: &str = "CustodyRoot.dll";
const ROOT_PATH: &str = "BepInEx/plugins/CustodyRoot/CustodyRoot.dll";
const CONSUMER: &str = "CustodyConsumer";
const CONSUMER_NODE: &str = "mod:CustodyConsumer";
const CONSUMER_GUID: &str = "fixture-custody-consumer";
const OWNER: &str = "CustodyOwner";
const OWNER_GUID: &str = "fixture-custody-owner";
const OWNED_PATH: &str = "BepInEx/plugins/Replacement/owned-resource";
const OWNED_BYTES: &[u8] = b"file entrusted to the other package";

struct CustodyFixture {
    f: Fixture,
    root: Server,
    newer: Server,
    older: Server,
    root_core: Vec<u8>,
    older_core: Vec<u8>,
    unchanged: Vec<(std::path::PathBuf, Vec<u8>)>,
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn online_assets(repo: &mut ModRepositoryConfig, assets: Vec<ReleaseAsset>) {
    assert_eq!(repo.cached_releases.len(), assets.len());
    repo.asset_rules[0].value.clone_from(&assets[0].name);
    for (release, asset) in repo.cached_releases.iter_mut().zip(assets) {
        release.assets = vec![asset];
    }
    let direct = PlanReference::Override(PlanOverride {
        release: Some(RouteOverride {
            route: Some(vec![PlanStep::Direct]),
            max_parallelism: Some(1),
            race: Some(false),
            download_threads: Some(1),
        }),
        ..Default::default()
    });
    repo.network_plan = NetworkPlanSelection {
        mainland: Some(direct.clone()),
        international: Some(direct),
    };
}

fn custody_fixture(resource_exists: bool) -> CustodyFixture {
    let root_core = mods::plugin_blob(ROOT_GUID, ROOT, "1.0.0");
    let older_core = mods::plugin_blob(GUID, REPO, "1.0.0");
    let newer_core = mods::plugin_blob(GUID, REPO, "2.0.0");
    let incoming_helper = mods::plugin_blob(COMPANION_GUID, "Companion", "2.0.0");
    let root_latest = zip(&[(ROOT_DLL, &mods::plugin_blob(ROOT_GUID, ROOT, "2.0.0"))]);
    let root_exact = zip(&[(ROOT_DLL, &root_core)]);
    let newer_zip = zip(&[
        ("Replacement.dll", &newer_core),
        ("Companion.dll", &incoming_helper),
        ("owned-resource", b"replacement must not write this"),
    ]);
    let older_zip = zip(&[("Replacement.dll", &older_core)]);
    let root = Server::new(vec![root_latest.clone(), root_exact.clone()], false);
    let newer = Server::new(vec![newer_zip.clone()], false);
    let older = Server::new(vec![older_zip.clone()], false);

    let mut exact = repository(
        ROOT,
        ROOT_GUID,
        &["2.0.0", "1.0.0"],
        vec![requires(GUID, ">=1.0.0")],
    );
    online_assets(
        &mut exact,
        vec![
            served(&root, 0, ROOT_ASSET, &root_latest),
            served(&root, 1, ROOT_ASSET, &root_exact),
        ],
    );
    let mut dependency = repository(REPO, GUID, &["2.0.0", "1.0.0"], vec![]);
    online_assets(
        &mut dependency,
        vec![
            served(&newer, 0, ASSET, &newer_zip),
            served(&older, 0, ASSET, &older_zip),
        ],
    );
    let consumer = repository(
        CONSUMER,
        CONSUMER_GUID,
        &["1.0.0"],
        vec![requires(COMPANION_GUID, ">=1.0.0,<3.0.0")],
    );
    let f = fixture(vec![
        exact,
        dependency,
        consumer,
        repository(OWNER, OWNER_GUID, &[], vec![]),
    ]);
    write(&f.game.join("BepInEx/core/BepInEx.dll"), PE);
    install(&f, REPO, GUID, "0.5.0", true);
    let consumer = install(&f, CONSUMER, CONSUMER_GUID, "1.0.0", true);
    let owner = install(&f, OWNER, OWNER_GUID, "1.0.0", true);
    write(
        &f.game.join(COMPANION),
        &mods::plugin_blob(COMPANION_GUID, "Companion", "1.0.0"),
    );
    if resource_exists {
        write(&f.game.join(OWNED_PATH), OWNED_BYTES);
    }
    let manifests = f.state.config.manifests_dir();
    let mut manifest = archive::read_manifest(&manifests, OWNER).unwrap();
    manifest.files.push(OWNED_PATH.into());
    archive::write_manifest(&manifests, &manifest).unwrap();
    let unchanged = [
        f.game.join(COMPANION),
        f.game.join(consumer),
        f.game.join(owner),
        manifests.join(format!("{CONSUMER}.toml")),
        manifests.join(format!("{OWNER}.toml")),
    ]
    .into_iter()
    .chain(resource_exists.then(|| f.game.join(OWNED_PATH)))
    .map(|path| {
        let bytes = fs::read(&path).unwrap();
        (path, bytes)
    })
    .collect();

    CustodyFixture {
        f,
        root,
        newer,
        older,
        root_core,
        older_core,
        unchanged,
    }
}

fn assert_action(plan: &ChangePlan, node_id: &str, kind: ActionKind, tag: &str) {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(
        plan.result
            .actions
            .iter()
            .any(|action| action.node_id == node_id
                && action.kind == kind
                && action.to_tag.as_deref() == Some(tag)),
        "expected {node_id} {kind:?} {tag}: {:?}",
        plan.result.actions
    );
}

fn assert_consumer_has_one_active_helper(f: &Fixture) {
    let snapshot = planning::evaluate(&f.state, vec![]).unwrap();
    let consumer = snapshot
        .evaluations
        .iter()
        .find(|node| node.node_id == CONSUMER_NODE)
        .unwrap();
    assert!(consumer.enabled);
    let [requirement] = consumer.runtime.as_slice() else {
        panic!("expected one active requirement: {consumer:?}");
    };
    assert_eq!(requirement.state, EvaluationState::Satisfied);
    assert_eq!(
        requirement.targets,
        [format!("dll:{}", inventory::key(COMPANION))]
    );
}

#[tokio::test]
async fn verified_auto_candidate_with_foreign_resource_falls_back_without_changing_exact_root() {
    assert_auto_fallback(true).await;
}

#[tokio::test]
async fn verified_auto_candidate_with_missing_foreign_resource_falls_back() {
    assert_auto_fallback(false).await;
}

async fn assert_auto_fallback(resource_exists: bool) {
    let test = custody_fixture(resource_exists);
    let f = &test.f;
    assert_consumer_has_one_active_helper(f);
    let before = fingerprint(f);
    let mut req = request(ROOT, Some(TAG), true);
    req.roots[0].asset_name = Some(ROOT_ASSET.into());
    let plan = planning::create(&f.state, req).unwrap();
    assert_action(&plan, ROOT_NODE, ActionKind::Install, TAG);
    assert_action(&plan, NODE, ActionKind::Upgrade, OLD_TAG);
    assert!(!plan.prepared);
    let initial_id = plan.plan_id.clone();
    let outcome = preparation::prepare(f.state.clone(), plan.plan_id, |_| {}).await;

    assert_eq!(fingerprint(f), before, "prepare must not write live files");
    assert_eq!(test.root.request_count(), 1);
    assert_eq!(test.newer.request_count(), 1);
    assert_eq!(outcome.error, None);
    let ready = outcome.plan.unwrap();
    assert!(ready.prepared);
    assert_ne!(
        ready.plan_id, initial_id,
        "fallback requires a new approval"
    );
    assert_eq!(ready.result.actions.len(), 2, "{:?}", ready.result.actions);
    assert_action(&ready, ROOT_NODE, ActionKind::Install, TAG);
    assert_action(&ready, NODE, ActionKind::Upgrade, TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(test.older.request_count(), 1);

    execution::commit(&f.state, &mut take(f, &ready), |_| {}).unwrap();
    assert_eq!(fs::read(f.game.join(ROOT_PATH)).unwrap(), test.root_core);
    assert_eq!(fs::read(f.game.join(CORE)).unwrap(), test.older_core);
    for (path, bytes) in test.unchanged {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
    }
    assert_eq!(f.game.join(OWNED_PATH).exists(), resource_exists);
    assert!(!f.game.join(DISABLED_COMPANION).exists());
    assert_consumer_has_one_active_helper(f);
    let manifests = f.state.config.manifests_dir();
    for (repo, path) in [(ROOT, ROOT_PATH), (REPO, CORE)] {
        let manifest = archive::read_manifest(&manifests, repo).unwrap();
        assert_eq!(manifest.release_tag, TAG);
        assert_eq!(manifest.repository_binding.unwrap().tag, TAG);
        assert_eq!(manifest.files, [path]);
    }
    no_pending(f);
}

#[tokio::test]
async fn changed_map_plugin_registry_stops_before_downloading_another_candidate() {
    use super::super::super::progress::ItemStage;

    let test = custody_fixture(true);
    let f = &test.f;
    let before_game = fingerprint(f).0;
    let registry = f
        .state
        .config
        .manifests_dir()
        .join("level-set-plugins.json");
    let mut req = request(ROOT, Some(TAG), true);
    req.roots[0].asset_name = Some(ROOT_ASSET.into());
    let plan = planning::create(&f.state, req).unwrap();
    assert_action(&plan, NODE, ActionKind::Upgrade, OLD_TAG);
    let registry_for_callback = registry.clone();
    let outcome = preparation::prepare(f.state.clone(), plan.plan_id, move |progress| {
        if progress.items.iter().any(|item| {
            item.node_id == NODE
                && item.tag.as_deref() == Some(OLD_TAG)
                && item.stage == ItemStage::Ready
        }) && !registry_for_callback.exists()
        {
            write(&registry_for_callback, b"invalid map plugin registry");
        }
    })
    .await;

    assert_eq!(outcome.error.as_deref(), Some("modChanges.targetChanged"));
    assert!(outcome.plan.is_none());
    assert_eq!(test.newer.request_count(), 1);
    assert_eq!(test.older.request_count(), 0);
    assert_eq!(fingerprint(f).0, before_game);
    assert_eq!(fs::read(registry).unwrap(), b"invalid map plugin registry");
    for (path, bytes) in test.unchanged {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
    }
    no_pending(f);
}

#[tokio::test]
async fn exact_release_with_foreign_resource_is_rejected_without_trying_older_release() {
    for resource_exists in [true, false] {
        let test = custody_fixture(resource_exists);
        let f = &test.f;
        let before = fingerprint(f);
        let mut req = request(REPO, Some(OLD_TAG), true);
        req.roots[0].asset_name = Some(ASSET.into());
        let plan = planning::create(&f.state, req).unwrap();
        assert_action(&plan, NODE, ActionKind::Upgrade, OLD_TAG);
        let outcome = preparation::prepare(f.state.clone(), plan.plan_id, |_| {}).await;

        assert_eq!(
            outcome.error,
            Some(blocked("modChanges.installTargetClaimed", OWNED_PATH))
        );
        assert!(outcome.plan.is_none());
        assert_eq!(test.newer.request_count(), 1);
        assert_eq!(test.older.request_count(), 0);
        assert_eq!(test.root.request_count(), 0);
        assert_eq!(fingerprint(f), before);
        no_pending(f);
    }
}
