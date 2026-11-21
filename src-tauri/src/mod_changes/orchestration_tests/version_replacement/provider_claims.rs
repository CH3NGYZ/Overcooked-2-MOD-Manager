//! A verified package can replace a loose provider only at the claimed path.
use super::*;
use crate::mod_changes::evaluate::{evaluate, targets};

const CONSUMER: &str = "CompanionConsumer";
const CONSUMER_GUID: &str = "com.example.companion-consumer";
const CONSUMER_NODE: &str = "mod:CompanionConsumer";
const OUTSIDE: &str = "Standalone";
const OUTSIDE_NODE: &str = "mod:Standalone";

fn install_consumer(f: &Fixture) -> String {
    let mut config = f.state.config.load_or_default();
    config.repositories.push(repository(
        CONSUMER,
        CONSUMER_GUID,
        &["1.0.0"],
        vec![requires(COMPANION_GUID, ">=1.0.0,<3.0.0")],
    ));
    f.state.config.save(&config).unwrap();
    install(f, CONSUMER, CONSUMER_GUID, "1.0.0", true)
}

fn assert_provider(f: &Fixture, owner: &str, version: &str) {
    let live = inventory::build(&f.state).unwrap().inventory;
    let providers = targets(&live, COMPANION_GUID, true);
    let [(index, plugin)] = providers.as_slice() else {
        panic!("expected one enabled provider: {providers:?}");
    };
    assert_eq!(live.nodes[*index].id, owner);
    assert_eq!(plugin.version.as_deref(), Some(version));
    let evaluations = evaluate(&live);
    let consumer = evaluations
        .iter()
        .find(|node| node.node_id == CONSUMER_NODE)
        .unwrap();
    let [requirement] = consumer.runtime.as_slice() else {
        panic!("expected one active requirement: {consumer:?}");
    };
    assert!(consumer.enabled);
    assert_eq!(requirement.state, EvaluationState::Satisfied);
}

#[tokio::test]
async fn verified_downgrade_replaces_the_same_path_provider_without_disabling_its_consumer() {
    let test = replacement("2.0.0", true, false, None, &[]);
    let f = &test.fixture;
    let consumer = install_consumer(f);
    let consumer_bytes = fs::read(f.game.join(&consumer)).unwrap();
    let loose = format!("dll:{}", inventory::key(COMPANION));
    assert_provider(f, &loose, "2.0.0");

    let plan = test.ready(ActionKind::Downgrade).await;
    assert_overwrites(&plan, vec![COMPANION, RESOURCE]);
    assert!(
        plan.result
            .actions
            .iter()
            .all(|action| action.node_id == NODE),
        "the replacement must not schedule a standalone switch: {:?}",
        plan.result.actions
    );
    execution::commit(&f.state, &mut take(f, &plan), |_| {}).unwrap();

    assert_provider(f, NODE, "1.0.0");
    assert_eq!(fs::read(f.game.join(&consumer)).unwrap(), consumer_bytes);
    assert_eq!(
        fs::read(f.game.join(COMPANION)).unwrap(),
        test.new_companion
    );
    assert!(!f.game.join(DISABLED_COMPANION).exists());
    let manifest = archive::read_manifest(&f.state.config.manifests_dir(), REPO).unwrap();
    assert_eq!(manifest.release_tag, TAG);
    assert!(manifest.files.iter().any(|file| file == COMPANION));
    no_pending(f);
}

#[tokio::test]
async fn verified_downgrade_cannot_consume_an_explicitly_kept_provider_at_another_path() {
    let test = replacement("2.0.0", true, false, None, &[]);
    let f = &test.fixture;
    install_consumer(f);
    // There is no existing DLL at the incoming path to claim. Give the
    // independent copy a source and known empty declarations so the initial
    // exact request can pin it before the archive adds a second provider.
    fs::remove_file(f.game.join(COMPANION)).unwrap();
    let mut config = f.state.config.load_or_default();
    config
        .repositories
        .push(repository(OUTSIDE, COMPANION_GUID, &["2.0.0"], vec![]));
    f.state.config.save(&config).unwrap();
    install(f, OUTSIDE, COMPANION_GUID, "2.0.0", true);
    assert_provider(f, OUTSIDE_NODE, "2.0.0");

    let mut req = exact_request();
    req.roots.push(RootRequest {
        intent: RootIntent {
            node_id: OUTSIDE_NODE.into(),
            candidate_id: None,
            enabled: true,
            member_key: None,
        },
        asset_name: None,
    });
    let outcome = test.prepare_request(req, ActionKind::Downgrade).await;
    assert_eq!(outcome.error, None);
    let plan = outcome.plan.unwrap();
    assert!(
        !plan.prepared,
        "prepared with duplicate providers: {:?}",
        plan.result
    );
    assert_ne!(plan.result.status, SolveStatus::Ready);
    assert!(
        plan.result
            .blockers
            .iter()
            .any(|blocker| blocker.target_guid.as_deref() == Some(COMPANION_GUID)),
        "the separate provider must remain part of the dependency graph: {:?}",
        plan.result.blockers
    );
    assert_provider(f, OUTSIDE_NODE, "2.0.0");
    no_pending(f);
}
