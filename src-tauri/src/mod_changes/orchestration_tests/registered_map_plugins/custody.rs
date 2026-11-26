use super::*;

const OTHER: &str = "Other";
const OTHER_ID: &str = "map:Other";
const OTHER_MAP_PATH: &str = "BepInEx/plugins/DIYLevel/levels/Other";
const OTHER_DLL: &str = "BepInEx/plugins/OtherLevelRuntimeLoader/OtherLevels.dll";

fn two_maps_with_same_guid() -> Fixture {
    let fixture = setup();
    write_map(&level_sets::disabled_root(&fixture.game).join(OTHER), OTHER);
    write(
        &fixture.game.join(mods::disabled_dll_file(OTHER_DLL)),
        &mods::plugin_blob(GUID, "OtherLevelsRuntime", "2.1.0"),
    );
    register_plugins(&fixture, &[(MAP, BUNDLED), (OTHER, OTHER_DLL)]);
    fixture
}

#[tokio::test]
async fn maps_with_the_same_dll_guid_switch_exclusive_copies_as_a_batch() {
    let fixture = two_maps_with_same_guid();
    let active = fixture.game.join(BUNDLED);
    let disabled = fixture.game.join(mods::disabled_dll_file(OTHER_DLL));
    let original = fs::read(&active).unwrap();
    let replacement = fs::read(&disabled).unwrap();
    let registry = fs::read(registry_path(&fixture)).unwrap();
    let mut request = toggle(false);
    let mut other = toggle(true).roots.remove(0);
    other.intent.node_id = OTHER_ID.into();
    request.roots.push(other);
    request.bulk_toggle = true;

    let plan = planning::create(&fixture.state, request).unwrap();
    let prepared = prepare(&fixture, &plan).await;
    for (node, kind) in [
        (MAP_ID.into(), ActionKind::Disable),
        (bundled_id(), ActionKind::Disable),
        (OTHER_ID.into(), ActionKind::Enable),
        (
            format!("map-dll:{}", inventory::key(OTHER_DLL)),
            ActionKind::Enable,
        ),
    ] {
        assert!(
            prepared
                .result
                .actions
                .iter()
                .any(|action| action.node_id == node && action.kind == kind),
            "missing {kind:?} action for {node}: {:?}",
            prepared.result
        );
    }
    super::super::super::execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {})
        .unwrap();

    assert!(!active.exists() && !disabled.exists());
    assert!(!fixture.game.join(MAP_PATH).exists());
    assert!(
        !level_sets::disabled_root(&fixture.game)
            .join(OTHER)
            .exists()
    );
    assert_eq!(
        fs::read(
            level_sets::disabled_root(&fixture.game)
                .join(MAP)
                .join("user-level")
        )
        .unwrap(),
        MAP_CONTENT
    );
    assert_eq!(
        fs::read(fixture.game.join(OTHER_MAP_PATH).join("user-level")).unwrap(),
        MAP_CONTENT
    );
    assert_eq!(
        fs::read(fixture.game.join(mods::disabled_dll_file(BUNDLED))).unwrap(),
        original
    );
    assert_eq!(fs::read(fixture.game.join(OTHER_DLL)).unwrap(), replacement);
    assert_eq!(fs::read(registry_path(&fixture)).unwrap(), registry);
}

#[tokio::test]
async fn enabling_a_map_does_not_steal_the_active_maps_same_guid_copy() {
    let fixture = two_maps_with_same_guid();
    let mut request = toggle(true);
    request.roots[0].intent.node_id = OTHER_ID.into();
    assert_request_refused(&fixture, request).await;
}

#[tokio::test]
async fn duplicate_registry_path_cannot_select_one_of_two_owners() {
    let fixture = setup();
    write_map(&fixture.game.join(OTHER_MAP_PATH), OTHER);
    register_plugins(&fixture, &[(MAP, BUNDLED), (OTHER, BUNDLED)]);
    assert_toggle_refused(&fixture).await;
}

#[tokio::test]
async fn a_mod_cannot_activate_a_disabled_maps_exclusive_plugin() {
    let fixture = setup();
    let disabled = level_sets::disabled_root(&fixture.game);
    fs::create_dir_all(&disabled).unwrap();
    fs::rename(fixture.game.join(MAP_PATH), disabled.join(MAP)).unwrap();
    fs::rename(
        fixture.game.join(BUNDLED),
        fixture.game.join(mods::disabled_dll_file(BUNDLED)),
    )
    .unwrap();
    let dependent = "Dependent";
    let dependent_guid = "fixture.map-runtime-dependent";
    let mut config = fixture.state.config.load_or_default();
    config.repositories.push(repository(
        dependent,
        dependent_guid,
        &["1.0.0"],
        vec![requires(GUID, ">=1.7.0")],
    ));
    fixture.state.config.save(&config).unwrap();
    install(&fixture, dependent, dependent_guid, "1.0.0", false);
    assert_request_refused(&fixture, request(dependent, None, true)).await;
}
