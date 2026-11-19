//! A relative card identity alone cannot authorize a switch in another game installation.
use super::*;
use crate::{asset_bundle::LevelSetMetadata, level_sets};

const MAP_ID: &str = "map:SharedName";
const FRAMEWORK: &str = "BepInEx/plugins/First/DIYLevel.dll";

fn write_map(game: &Path, framework: &str, version: &str) -> std::path::PathBuf {
    let dll = game.join(framework);
    write(
        &dll,
        &mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", version),
    );
    let map = dll.parent().unwrap().join("levels/SharedName");
    let info = map.join("info");
    write(&info, b"fixture map");
    level_sets::seed_info_metadata(
        &info,
        LevelSetMetadata {
            name: "SharedName".into(),
            version: "1.0.0".into(),
            ..Default::default()
        },
    );
    map
}

fn publish_scan(f: &Fixture, game: &Path) -> u64 {
    let config = f.state.config.load_or_default();
    let mods = mods::scan_mods_with_manifests(game, &config, Some(&f.state.config.manifests_dir()));
    let mut scans = f.state.local_mod_snapshot.lock().unwrap();
    let generation = scans.begin(Some(game));
    scans.publish(generation, Some(game), mods).unwrap();
    generation
}

fn evaluated_map_toggle(f: &Fixture, generation: u64) -> PlanRequest {
    let evaluation = planning::evaluate(&f.state, vec![]).unwrap();
    assert!(evaluation.nodes.iter().any(|node| node.node_id == MAP_ID));
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: MAP_ID.into(),
                candidate_id: None,
                enabled: false,
                member_key: None,
            },
            asset_name: None,
        }],
        toggle_scope: Some(ToggleScope {
            generation,
            revision: evaluation.revision,
        }),
        ..Default::default()
    }
}

fn rejected_before_staging(f: &Fixture, request: PlanRequest) {
    assert_eq!(
        planning::create(&f.state, request).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert!(f.state.mod_changes.lock().unwrap().plans.is_empty());
    assert!(
        !f.state
            .config
            .config_path()
            .parent()
            .unwrap()
            .join("mod-change-staging")
            .exists()
    );
}

#[test]
fn a_map_click_cannot_rebind_to_a_same_name_map_in_another_game() {
    let f = fixture(vec![]);
    let original = write_map(&f.game, FRAMEWORK, "1.0.0");
    let generation = publish_scan(&f, &f.game);
    let before_switch = evaluated_map_toggle(&f, generation);
    let other_game = f.game.with_file_name("other-game");
    let other = write_map(&other_game, FRAMEWORK, "1.0.0");
    write(
        &other_game.join("BepInEx/core/BepInEx.dll"),
        b"fixture loader",
    );
    let mut config = f.state.config.load_or_default();
    config.game.path = other_game.to_string_lossy().into_owned();
    f.state.config.save(&config).unwrap();

    rejected_before_staging(&f, before_switch);
    // Both roots resolve `map:SharedName`; a fresh evaluation still cannot
    // move the card because its click carries the previous root's scan.
    rejected_before_staging(&f, evaluated_map_toggle(&f, generation));
    let fresh = publish_scan(&f, &other_game);
    let plan = planning::create(&f.state, evaluated_map_toggle(&f, fresh)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    assert_eq!(plan.result.actions.len(), 1);
    assert!(original.is_dir() && other.is_dir());
}

#[test]
fn a_map_click_cannot_rebind_to_a_new_diy_level_directory_in_the_same_game() {
    let f = fixture(vec![]);
    let original = write_map(&f.game, FRAMEWORK, "1.0.0");
    let generation = publish_scan(&f, &f.game);
    let other = write_map(&f.game, "BepInEx/plugins/New/DIYLevel.dll", "1.1.0");
    rejected_before_staging(&f, evaluated_map_toggle(&f, generation));
    assert!(original.is_dir() && other.is_dir());
}

#[test]
fn a_replaced_scan_or_changed_inventory_blocks_old_switch_requests() {
    let f = fixture(vec![]);
    let original = write_map(&f.game, FRAMEWORK, "1.0.0");
    let generation = publish_scan(&f, &f.game);
    let stale_generation = evaluated_map_toggle(&f, generation);
    let generation = publish_scan(&f, &f.game);
    rejected_before_staging(&f, stale_generation);
    let stale_revision = evaluated_map_toggle(&f, generation);
    write(&original.join("new-file"), b"map edit after evaluation");
    rejected_before_staging(&f, stale_revision);
    let plan = planning::create(&f.state, evaluated_map_toggle(&f, generation)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    assert_eq!(plan.result.actions.len(), 1);
}
