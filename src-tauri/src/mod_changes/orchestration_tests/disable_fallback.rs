//! Exercise dependency disablement through real inventory, payloads and journaled writes.
use super::*;
use crate::{asset_bundle::LevelSetMetadata, level_sets};
use std::cell::Cell;

const MAP: &str = "WebMap";
const MAP_PATH: &str = "BepInEx/plugins/framework/levels/WebMap";
const EXTERNAL: &str = "BepInEx/plugins/External/External.dll";

struct DisableFixture {
    f: Fixture,
    framework: String,
    web: String,
    dependent: String,
    metadata: LevelSetMetadata,
}

fn setup() -> DisableFixture {
    let fw = mods::plugin_target::DIYLEVEL_GUID;
    let f = fixture(vec![
        repository("framework", fw, &["0.11.0", "0.10.0"], vec![]),
        repository(
            "web",
            "fixture-web",
            &["4.0.0"],
            vec![requires(fw, ">=0.11.0")],
        ),
        repository(
            "dependent",
            "fixture-dependent",
            &["1.0.0"],
            vec![requires("fixture-web", ">=4.0.0")],
        ),
        repository(
            "external-source",
            "fixture-external",
            &[],
            vec![requires("fixture-web", ">=4.0.0")],
        ),
    ]);
    let framework = install(&f, "framework", fw, "0.11.0", true);
    let web = install(&f, "web", "fixture-web", "4.0.0", true);
    let dependent = install(&f, "dependent", "fixture-dependent", "1.0.0", true);
    let metadata = LevelSetMetadata {
        name: MAP.into(),
        author: "Integration".into(),
        version: "1.0.0".into(),
        ..Default::default()
    };
    let map = f.game.join(MAP_PATH);
    write(&map.join("info"), b"fixture info metadata");
    write(&map.join("user-level"), b"user map content");
    write(
        &map.join("Inside.dll"),
        &mods::plugin_blob("fixture-inside", "Inside", "1.0.0"),
    );
    level_sets::seed_info_metadata(&map.join("info"), metadata.clone());
    write(
        &f.game.join(EXTERNAL),
        &mods::plugin_blob("fixture-external", "External", "1.0.0"),
    );
    let manifests = f.state.config.manifests_dir();
    write(
        &manifests.join(level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        &serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "entries": [{
                "dll": EXTERNAL,
                "owner": { "name": MAP, "author": metadata.author }
            }]
        }))
        .unwrap(),
    );
    archive::write_map_relation_snapshots(
        &manifests,
        &MapRelationSnapshots {
            schema_version: crate::models::default_snapshot_schema_version(),
            entries: BTreeMap::from([(
                "installed-web-map".into(),
                MapRelationSnapshot {
                    repository_id: "installed-web-map".into(),
                    level_set_name: MAP.into(),
                    level_set_name_zh: String::new(),
                    author: metadata.author.clone(),
                    group_i18n: BTreeMap::new(),
                    group: String::new(),
                    relations: vec![requires("fixture-dependent", ">=1.0.0")],
                    release_relations: vec![],
                    installed_mapset_version: "1.0.0".into(),
                },
            )]),
        },
    )
    .unwrap();
    DisableFixture {
        f,
        framework,
        web,
        dependent,
        metadata,
    }
}

fn external_id() -> String {
    format!("map-dll:{}", inventory::key(EXTERNAL))
}

fn ready_session(test: &DisableFixture) -> PlanSession {
    let f = &test.f;
    let plan = planning::create(&f.state, request("framework", Some("v0.10.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    for id in [
        "mod:web".to_string(),
        "mod:dependent".into(),
        format!("map:{MAP}"),
        external_id(),
    ] {
        let action = plan
            .result
            .actions
            .iter()
            .find(|action| action.node_id == id && action.kind == ActionKind::Disable)
            .unwrap_or_else(|| panic!("missing explicit disable for {id}: {:?}", plan.result));
        assert!(!action.reason.is_empty());
        assert!(
            !action.caused_by.is_empty(),
            "disable must identify its cause"
        );
    }
    let mut session = take(f, &plan);
    let installed = &session.trusted.inventory;
    let map = installed
        .nodes
        .iter()
        .find(|node| node.id == format!("map:{MAP}"))
        .unwrap();
    assert_eq!(map.kind, NodeKind::Map);
    assert!(map.enabled);
    assert!(
        map.current
            .as_ref()
            .unwrap()
            .relations
            .iter()
            .any(|owned| { owned.relation.guid == mods::plugin_target::DIYLEVEL_GUID })
    );
    let external = installed
        .nodes
        .iter()
        .find(|node| node.id == external_id())
        .unwrap();
    assert_eq!(external.kind, NodeKind::Mod);
    assert!(external.enabled && external.mutable);
    assert!(session.trusted.files[&external.id].manifests.is_empty());
    assert!(session.trusted.files[&external.id].map_path.is_none());

    verify_selected(f, &mut session);
    inventory::validate_solution(
        &session.trusted,
        &session.request,
        session.solved.as_ref().unwrap(),
    )
    .unwrap();
    preparation::finish(&f.state, &mut session).unwrap();
    assert!(f.game.join(&test.web).is_file());
    assert!(f.game.join(&test.dependent).is_file());
    assert!(f.game.join(EXTERNAL).is_file());
    assert!(f.game.join(MAP_PATH).join("Inside.dll").is_file());
    assert!(!level_sets::disabled_root(&f.game).join(MAP).exists());
    session
}

#[test]
fn incompatible_web_history_disables_mod_map_and_independent_external_dll_atomically() {
    let test = setup();
    let f = &test.f;
    let manifests = f.state.config.manifests_dir();
    let before_web = fs::read(manifests.join("web.toml")).unwrap();
    let before_dependent = fs::read(manifests.join("dependent.toml")).unwrap();
    let before_snapshots = fs::read(manifests.join("map-relations.json")).unwrap();
    let mut session = ready_session(&test);
    assert_eq!(
        archive::read_manifest(&manifests, "framework")
            .unwrap()
            .release_tag,
        "v0.11.0"
    );
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session).unwrap();

    for file in [test.web.as_str(), test.dependent.as_str(), EXTERNAL] {
        assert!(!f.game.join(file).exists());
        assert!(f.game.join(mods::disabled_dll_file(file)).is_file());
    }
    assert!(f.game.join(&test.framework).is_file());
    assert!(!f.game.join(MAP_PATH).exists());
    let disabled = level_sets::disabled_root(&f.game).join(MAP);
    assert_eq!(
        fs::read(disabled.join("user-level")).unwrap(),
        b"user map content"
    );
    assert!(!disabled.join("Inside.dll").exists());
    assert!(disabled.join("Inside.dll.disabled").is_file());
    assert_eq!(
        archive::read_manifest(&manifests, "framework")
            .unwrap()
            .release_tag,
        "v0.10.0"
    );
    // Toggle-only ownership records use canonical DLL names and remain valid for .disabled.
    assert_eq!(fs::read(manifests.join("web.toml")).unwrap(), before_web);
    assert_eq!(
        fs::read(manifests.join("dependent.toml")).unwrap(),
        before_dependent
    );
    assert_eq!(
        fs::read(manifests.join("map-relations.json")).unwrap(),
        before_snapshots
    );
    let registry = level_sets::read_registered_plugins(&manifests);
    assert_eq!(registry.len(), 1);
    assert_eq!(registry[0].dll, EXTERNAL);
    assert_eq!(registry[0].owner.name, MAP);

    // The scanner keys fixture info by its actual physical path after the move.
    level_sets::seed_info_metadata(&disabled.join("info"), test.metadata.clone());
    let actual = inventory::build(&f.state).unwrap();
    for id in [
        "mod:web".to_string(),
        "mod:dependent".into(),
        format!("map:{MAP}"),
        external_id(),
    ] {
        let node = actual
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap();
        assert!(
            !node.enabled,
            "{id} must remain disabled after a real rescan"
        );
    }
    assert_eq!(actual.files["mod:web"].manifests[0].repository_id, "web");
    let framework = actual
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == "mod:framework")
        .unwrap();
    assert!(framework.enabled);
    assert_eq!(
        framework.current.as_ref().unwrap().version.as_deref(),
        Some("0.10.0")
    );
    assert!(!has_pending(&manifests.parent().unwrap().join("mod-change-transactions")).unwrap());
}

#[test]
fn cancelling_frozen_disable_batch_restores_real_mod_map_registry_and_manifest_files() {
    let test = setup();
    let f = &test.f;
    let manifests = f.state.config.manifests_dir();
    let game_before = Fingerprint::capture(&f.game).unwrap();
    let manifests_before = Fingerprint::capture(&manifests).unwrap();
    let mut session = ready_session(&test);
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), game_before);
    assert_eq!(Fingerprint::capture(&manifests).unwrap(), manifests_before);
    let mut changes = session.mutations.take().unwrap();
    let framework_path = f.game.join(&test.framework);
    let framework_before = fs::read(&framework_path).unwrap();
    let framework_after = changes
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::File(change)
                if change.target.root == RootKind::Game
                    && change.target.relative == test.framework =>
            {
                change
                    .source
                    .as_ref()
                    .map(|source| fs::read(source).unwrap())
            }
            _ => None,
        })
        .expect("production mutations must write the actual framework payload");
    assert_ne!(framework_after, framework_before);
    // Keep one existing, independent disable last so the real payload write has
    // a subsequent cancellation boundary. No synthetic operation is introduced.
    let web_disable = changes
        .iter()
        .position(|mutation| {
            matches!(mutation, Mutation::Move(change)
            if change.from.root == RootKind::Game
                && inventory::key(&change.from.relative) == inventory::key(&test.web))
        })
        .expect("production mutations must disable Web by renaming its DLL");
    let web_disable = changes.remove(web_disable);
    changes.push(web_disable);
    assert!(
        changes.len() > 4,
        "fixture must exercise several physical mutations"
    );
    let data = manifests.parent().unwrap().to_path_buf();
    let journal_root = data.join("mod-change-transactions");
    let _operation = f.state.operation.lock().unwrap();
    super::super::guard::ensure_recovered(&f.state).unwrap();
    let transaction = prepare(
        &journal_root,
        &TransactionRoots {
            game_root: f.game.clone(),
            data_root: data,
        },
        changes,
    )
    .unwrap();
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), game_before);
    assert_eq!(Fingerprint::capture(&manifests).unwrap(), manifests_before);
    let observed_payload = Cell::new(false);
    let error = transaction
        .apply_with_cancel(|| {
            if !fs::read(&framework_path).is_ok_and(|bytes| bytes == framework_after) {
                return false;
            }
            observed_payload.set(true);
            assert_eq!(
                archive::read_manifest(&manifests, "framework")
                    .unwrap()
                    .release_tag,
                "v0.10.0"
            );
            for file in [test.dependent.as_str(), EXTERNAL] {
                assert!(!f.game.join(file).exists());
                assert!(f.game.join(mods::disabled_dll_file(file)).is_file());
            }
            assert!(!f.game.join(MAP_PATH).exists());
            assert!(
                level_sets::disabled_root(&f.game)
                    .join(MAP)
                    .join("Inside.dll.disabled")
                    .is_file()
            );
            assert!(f.game.join(&test.web).is_file());
            true
        })
        .unwrap_err();

    assert!(
        observed_payload.get(),
        "cancellation must follow the live payload replacement"
    );
    assert_eq!(error, "download.cancelled");
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), game_before);
    assert_eq!(Fingerprint::capture(&manifests).unwrap(), manifests_before);
    assert!(f.game.join(&test.web).is_file());
    assert!(f.game.join(EXTERNAL).is_file());
    assert!(f.game.join(MAP_PATH).join("Inside.dll").is_file());
    assert!(!has_pending(&journal_root).unwrap());
}
